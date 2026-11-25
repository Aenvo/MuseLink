//! Windows 写受限执行沙箱：Restricted Token (WRITE_RESTRICTED) + 目录 ACL + Job Object。
//! 沙箱初始化失败直接返回错误，绝不静默降级为无隔离裸跑。
#![cfg(windows)]

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GENERIC_ALL, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, HLOCAL,
    INVALID_HANDLE_VALUE, LocalFree, STILL_ACTIVE, SetHandleInformation, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, CreateRestrictedToken,
    DACL_SECURITY_INFORMATION, DISABLE_MAX_PRIVILEGE, EqualSid, GetAce, GetAclInformation,
    GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SID_AND_ATTRIBUTES,
    SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DEFAULT_DACL,
    TOKEN_DUPLICATE, TOKEN_GROUPS, TOKEN_QUERY, TokenDefaultDacl, TokenGroups, TokenLogonSid,
    WRITE_RESTRICTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL, FILE_DELETE_CHILD,
    FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
    CreateProcessW, GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, PROCESS_INFORMATION,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};

use super::job::{ChildJob, JobLimits};

/// MuseLink 专用合成限制 SID，仅用于标记写受限沙箱允许写入的工作目录与隔离临时目录。
const SANDBOX_SID_STR: &str = "S-1-5-21-3892149017-1290471821-2910478123-9001";
/// Windows 内置 RESTRICTED SID 与 World/Everyone SID：用于通过系统 DLL、控制台与运行时早期初始化，
/// 普通用户目录、桌面、启动项、.ssh 及注册表均不授予二者写权限。
const RESTRICTED_SID_STR: &str = "S-1-5-12";
const WORLD_SID_STR: &str = "S-1-1-0";

const OBJECT_INHERIT_ACE_FLAG: u32 = 0x1;
const CONTAINER_INHERIT_ACE_FLAG: u32 = 0x2;
const SUB_CONTAINERS_AND_OBJECTS_INHERIT_FLAG: u32 =
    OBJECT_INHERIT_ACE_FLAG | CONTAINER_INHERIT_ACE_FLAG;
const SANDBOX_WRITE_MASK: u32 =
    FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE | FILE_DELETE_CHILD;

pub(crate) const DEFAULT_EXEC_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const DEFAULT_EXEC_MAX_PROCESSES: u32 = 64;

pub(crate) struct SpawnConfig {
    pub(crate) application: PathBuf,
    pub(crate) command_line: String,
    pub(crate) cwd: PathBuf,
    pub(crate) temp_dir: PathBuf,
    pub(crate) sandboxed: bool,
    pub(crate) limits: JobLimits,
}

pub(crate) struct SpawnedProcess {
    process: OwnedHandle,
    job: ChildJob,
    pub(crate) stdout: Option<File>,
    pub(crate) stderr: Option<File>,
}

impl SpawnedProcess {
    pub(crate) fn take_pipes(&mut self) -> Option<(File, File)> {
        Some((self.stdout.take()?, self.stderr.take()?))
    }

    pub(crate) fn try_wait(&self) -> io::Result<Option<i32>> {
        // SAFETY: `self.process` is a valid live process handle.
        let waited = unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) };
        if waited != WAIT_OBJECT_0 {
            return Ok(None);
        }
        let mut code: u32 = 0;
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if code == STILL_ACTIVE as u32 {
            return Ok(None);
        }
        Ok(Some(code as i32))
    }

    pub(crate) fn wait_timeout(&self, timeout_ms: u32) -> io::Result<Option<i32>> {
        // SAFETY: `self.process` is a valid live process handle.
        let waited = unsafe { WaitForSingleObject(self.process.as_raw_handle(), timeout_ms) };
        if waited != WAIT_OBJECT_0 {
            return Ok(None);
        }
        self.try_wait()
    }

    pub(crate) fn terminate(&self) {
        self.job.terminate();
    }
}

struct LocalSid(PSID);

impl LocalSid {
    fn from_str(sid_str: &str) -> io::Result<Self> {
        let wide = to_wide(sid_str);
        let mut sid: PSID = null_mut();
        // SAFETY: `wide` is null-terminated; Windows allocates the SID via LocalAlloc.
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 || sid.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(sid))
    }

    fn sandbox() -> io::Result<Self> {
        Self::from_str(SANDBOX_SID_STR)
    }

    fn restricted() -> io::Result<Self> {
        Self::from_str(RESTRICTED_SID_STR)
    }

    fn world() -> io::Result<Self> {
        Self::from_str(WORLD_SID_STR)
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` was allocated by ConvertStringSidToSidW.
            unsafe { LocalFree(self.0 as HLOCAL) };
        }
    }
}

pub(crate) fn spawn(config: &SpawnConfig) -> io::Result<SpawnedProcess> {
    std::fs::create_dir_all(&config.temp_dir)?;
    let sandbox_sid = if config.sandboxed {
        let sid = LocalSid::sandbox()?;
        ensure_sandbox_write_access(&config.cwd, sid.0)?;
        ensure_sandbox_write_access(&config.temp_dir, sid.0)?;
        Some(sid)
    } else {
        None
    };

    let (stdout_read, stdout_write) = create_pipe_pair()?;
    let (stderr_read, stderr_write) = create_pipe_pair()?;
    let nul_in = open_nul_stdin()?;
    let env_block = build_clean_env(&config.temp_dir);

    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = nul_in.as_raw_handle();
    startup.hStdOutput = stdout_write.as_raw_handle();
    startup.hStdError = stderr_write.as_raw_handle();

    let app_wide = to_wide(&strip_verbatim(&config.application));
    let mut cmd_wide = to_wide(&config.command_line);
    let cwd_wide = to_wide(&strip_verbatim(&config.cwd));
    let flags = CREATE_NO_WINDOW | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let created = if let Some(sid) = &sandbox_sid {
        let restricted = create_write_restricted_token(sid.0)?;
        // SAFETY: all wide strings, env_block, and handles remain alive across the call.
        unsafe {
            CreateProcessAsUserW(
                restricted.as_raw_handle(),
                app_wide.as_ptr(),
                cmd_wide.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                env_block.as_ptr().cast(),
                cwd_wide.as_ptr(),
                &startup,
                &mut pi,
            )
        }
    } else {
        // SAFETY: all wide strings, env_block, and handles remain alive across the call.
        unsafe {
            CreateProcessW(
                app_wide.as_ptr(),
                cmd_wide.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                env_block.as_ptr().cast(),
                cwd_wide.as_ptr(),
                &startup,
                &mut pi,
            )
        }
    };

    // 父进程立即关闭已传递给子进程的管道写端与 NUL 输入端，确保子进程退出时读端能收到 EOF。
    drop(stdout_write);
    drop(stderr_write);
    drop(nul_in);

    if created == 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: CreateProcess* returned valid owned process and thread handles.
    let process = unsafe { OwnedHandle::from_raw_handle(pi.hProcess) };
    let thread = unsafe { OwnedHandle::from_raw_handle(pi.hThread) };

    let job = match ChildJob::attach_raw_with_limits(process.as_raw_handle(), config.limits) {
        Ok(job) => job,
        Err(error) => {
            unsafe {
                let _ = TerminateProcess(process.as_raw_handle(), 1);
            }
            return Err(error);
        }
    };

    if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
        let error = io::Error::last_os_error();
        job.terminate();
        return Err(error);
    }

    Ok(SpawnedProcess {
        process,
        job,
        stdout: Some(File::from(stdout_read)),
        stderr: Some(File::from(stderr_read)),
    })
}

fn create_write_restricted_token(sid: PSID) -> io::Result<OwnedHandle> {
    let mut current: HANDLE = null_mut();
    let access = TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY | TOKEN_QUERY | TOKEN_ADJUST_DEFAULT;
    // SAFETY: GetCurrentProcess returns the current pseudo-handle.
    if unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut current) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let current = unsafe { OwnedHandle::from_raw_handle(current) };

    // 参考 OpenAI Codex Windows 写受限沙箱配方：
    // 保留 RESTRICTED (S-1-5-12)、Everyone (S-1-1-0) 与当前会话 Logon SID 以通过 Windows
    // DLL/控制台/运行时初始化，并叠加 MuseLink 专用合成 SID（仅授予目标工作区与隔离临时目录写权限）。
    let restricted_code_sid = LocalSid::restricted()?;
    let world_sid = LocalSid::world()?;
    let (logon_buf, logon_sid) = read_logon_sid(current.as_raw_handle());
    let _keep_alive = &logon_buf;
    let mut restrict = vec![
        SID_AND_ATTRIBUTES {
            Sid: sid,
            Attributes: 0,
        },
        SID_AND_ATTRIBUTES {
            Sid: restricted_code_sid.0,
            Attributes: 0,
        },
        SID_AND_ATTRIBUTES {
            Sid: world_sid.0,
            Attributes: 0,
        },
    ];
    if let Some(lsid) = logon_sid {
        restrict.push(SID_AND_ATTRIBUTES {
            Sid: lsid,
            Attributes: 0,
        });
    }

    let mut restricted: HANDLE = null_mut();
    // SAFETY: `restrict` and `logon_buf` stay alive for the synchronous call.
    if unsafe {
        CreateRestrictedToken(
            current.as_raw_handle(),
            DISABLE_MAX_PRIVILEGE | WRITE_RESTRICTED,
            0,
            null(),
            0,
            null(),
            restrict.len() as u32,
            restrict.as_ptr(),
            &mut restricted,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let restricted = unsafe { OwnedHandle::from_raw_handle(restricted) };
    apply_sandbox_default_dacl(
        restricted.as_raw_handle(),
        sid,
        restricted_code_sid.0,
        logon_sid,
    )?;
    Ok(restricted)
}

fn apply_sandbox_default_dacl(
    token: HANDLE,
    sandbox_sid: PSID,
    restricted_sid: PSID,
    logon_sid: Option<PSID>,
) -> io::Result<()> {
    let mut needed: u32 = 0;
    unsafe {
        GetTokenInformation(token, TokenDefaultDacl, null_mut(), 0, &mut needed);
    }
    let mut dacl_buf = vec![0u8; needed as usize];
    let existing_dacl = if needed > 0
        && unsafe {
            GetTokenInformation(
                token,
                TokenDefaultDacl,
                dacl_buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } != 0
    {
        let info = unsafe { &*(dacl_buf.as_ptr() as *const TOKEN_DEFAULT_DACL) };
        info.DefaultDacl
    } else {
        null_mut()
    };

    let make_ea = |target_sid: PSID| EXPLICIT_ACCESS_W {
        grfAccessPermissions: GENERIC_ALL,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: 0,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: target_sid.cast(),
        },
    };
    let mut entries = vec![make_ea(sandbox_sid), make_ea(restricted_sid)];
    if let Some(lsid) = logon_sid {
        entries.push(make_ea(lsid));
    }

    let mut new_dacl: *mut ACL = null_mut();
    let status = unsafe {
        SetEntriesInAclW(
            entries.len() as u32,
            entries.as_ptr(),
            existing_dacl,
            &mut new_dacl,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let td = TOKEN_DEFAULT_DACL {
        DefaultDacl: new_dacl,
    };
    let ok = unsafe {
        SetTokenInformation(
            token,
            TokenDefaultDacl,
            (&td as *const TOKEN_DEFAULT_DACL).cast(),
            std::mem::size_of::<TOKEN_DEFAULT_DACL>() as u32,
        )
    };
    let err = if ok == 0 {
        Some(io::Error::last_os_error())
    } else {
        None
    };
    if !new_dacl.is_null() {
        unsafe { LocalFree(new_dacl as HLOCAL) };
    }
    if let Some(e) = err {
        return Err(e);
    }
    Ok(())
}

fn read_logon_sid(token: HANDLE) -> (Vec<u8>, Option<PSID>) {
    // 先查 TokenLogonSid
    let mut needed: u32 = 0;
    unsafe {
        GetTokenInformation(token, TokenLogonSid, null_mut(), 0, &mut needed);
    }
    if needed > 0 {
        let mut buf = vec![0u8; needed as usize];
        if unsafe {
            GetTokenInformation(
                token,
                TokenLogonSid,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } != 0
        {
            let groups = unsafe { &*(buf.as_ptr() as *const TOKEN_GROUPS) };
            if groups.GroupCount > 0 && !groups.Groups[0].Sid.is_null() {
                let sid = groups.Groups[0].Sid;
                return (buf, Some(sid));
            }
        }
    }
    // 兜底从 TokenGroups 查找带 SE_GROUP_LOGON_ID (0xC0000000) 标志的 SID
    const SE_GROUP_LOGON_ID: u32 = 0xC000_0000;
    needed = 0;
    unsafe {
        GetTokenInformation(token, TokenGroups, null_mut(), 0, &mut needed);
    }
    if needed > 0 {
        let mut buf = vec![0u8; needed as usize];
        if unsafe {
            GetTokenInformation(
                token,
                TokenGroups,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } != 0
        {
            let groups = unsafe { &*(buf.as_ptr() as *const TOKEN_GROUPS) };
            let slice = unsafe {
                std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize)
            };
            for g in slice {
                if (g.Attributes & SE_GROUP_LOGON_ID) == SE_GROUP_LOGON_ID && !g.Sid.is_null() {
                    return (buf, Some(g.Sid));
                }
            }
        }
    }
    (Vec::new(), None)
}

/// 幂等检查并为目标目录授予沙箱 SID 的可继承读写删除权限，避免重复递归刷 ACL。
fn ensure_sandbox_write_access(dir: &Path, sid: PSID) -> io::Result<()> {
    let mut wide = to_wide(&strip_verbatim(dir));
    let mut old_dacl: *mut ACL = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut old_dacl,
            null_mut(),
            &mut sd,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }

    let already_granted = unsafe { dacl_grants_sandbox_sid(old_dacl, sid) };
    if already_granted {
        if !sd.is_null() {
            unsafe { LocalFree(sd as HLOCAL) };
        }
        return Ok(());
    }

    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: SANDBOX_WRITE_MASK,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT_FLAG,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.cast(),
        },
    };
    let mut new_dacl: *mut ACL = null_mut();
    let set_acl_status = unsafe { SetEntriesInAclW(1, &ea, old_dacl, &mut new_dacl) };
    if set_acl_status != ERROR_SUCCESS {
        if !sd.is_null() {
            unsafe { LocalFree(sd as HLOCAL) };
        }
        return Err(io::Error::from_raw_os_error(set_acl_status as i32));
    }

    let apply_status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            new_dacl,
            null_mut(),
        )
    };
    if !new_dacl.is_null() {
        unsafe { LocalFree(new_dacl as HLOCAL) };
    }
    if !sd.is_null() {
        unsafe { LocalFree(sd as HLOCAL) };
    }
    if apply_status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(apply_status as i32));
    }
    Ok(())
}

unsafe fn dacl_grants_sandbox_sid(dacl: *const ACL, sid: PSID) -> bool {
    if dacl.is_null() {
        return false;
    }
    let mut info: ACL_SIZE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return false;
    }
    for index in 0..info.AceCount {
        let mut ace_ptr: *mut std::ffi::c_void = null_mut();
        if unsafe { GetAce(dacl, index, &mut ace_ptr) } == 0 || ace_ptr.is_null() {
            continue;
        }
        let ace = unsafe { &*(ace_ptr as *const ACCESS_ALLOWED_ACE) };
        // ACCESS_ALLOWED_ACE_TYPE == 0
        if ace.Header.AceType != 0 {
            continue;
        }
        let inherit = u32::from(ace.Header.AceFlags);
        if inherit & SUB_CONTAINERS_AND_OBJECTS_INHERIT_FLAG
            != SUB_CONTAINERS_AND_OBJECTS_INHERIT_FLAG
        {
            continue;
        }
        let mask = ace.Mask;
        let has_write = (mask & FILE_ALL_ACCESS) == FILE_ALL_ACCESS
            || (mask & SANDBOX_WRITE_MASK) == SANDBOX_WRITE_MASK;
        if !has_write {
            continue;
        }
        let ace_sid = std::ptr::addr_of!(ace.SidStart) as PSID;
        if unsafe { EqualSid(ace_sid, sid) } != 0 {
            return true;
        }
    }
    false
}

fn create_pipe_pair() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let mut read: HANDLE = null_mut();
    let mut write: HANDLE = null_mut();
    if unsafe { CreatePipe(&mut read, &mut write, &sa, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // 仅允许写端被子进程继承，父进程读端绝不继承。
    if unsafe { SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0) } == 0 {
        let error = io::Error::last_os_error();
        unsafe {
            CloseHandle(read);
            CloseHandle(write);
        }
        return Err(error);
    }
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

fn open_nul_stdin() -> io::Result<OwnedHandle> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let nul = to_wide("NUL");
    let handle = unsafe {
        CreateFileW(
            nul.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &sa,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(io::Error::from_raw_os_error(unsafe {
            GetLastError() as i32
        }));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

fn build_clean_env(temp_dir: &Path) -> Vec<u16> {
    let mut vars: BTreeMap<String, (OsString, OsString)> = BTreeMap::new();
    for (k, v) in std::env::vars_os() {
        let upper = k.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("MUSE_CONNECTOR_")
            || upper.starts_with("MUSE_LINK_")
            || upper.contains("AGENT_TOKEN")
        {
            continue;
        }
        vars.insert(upper, (k, v));
    }
    let clean_tmp = OsString::from(strip_verbatim(temp_dir));
    vars.insert("TEMP".into(), (OsString::from("TEMP"), clean_tmp.clone()));
    vars.insert("TMP".into(), (OsString::from("TMP"), clean_tmp));

    let mut block = Vec::new();
    for (_, (k, v)) in vars {
        block.extend(k.encode_wide());
        block.push(b'=' as u16);
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

pub(crate) fn strip_verbatim(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
}

fn to_wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain([0]).collect()
}

/// 按 Windows CreateProcess / CommandLineToArgvW 规则构造命令行字符串。
pub(crate) fn build_windows_cmdline(program: &str, args: &[String]) -> String {
    let mut out = quote_windows_arg(program);
    for arg in args {
        out.push(' ');
        out.push_str(&quote_windows_arg(arg));
    }
    out
}

pub(crate) fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .bytes()
            .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'"'))
    {
        return arg.to_owned();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else if ch == '"' {
            for _ in 0..backslashes * 2 + 1 {
                out.push('\\');
            }
            out.push('"');
            backslashes = 0;
        } else {
            for _ in 0..backslashes {
                out.push('\\');
            }
            backslashes = 0;
            out.push(ch);
        }
    }
    for _ in 0..backslashes * 2 {
        out.push('\\');
    }
    out.push('"');
    out
}

pub(crate) fn system_executable(relative: &str) -> io::Result<PathBuf> {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let exe = root.join(relative);
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("system tool not found: {relative}"),
        ))
    }
}

/// 解析直接调用的可执行文件（优先 .exe/.com，其次 .cmd/.bat）。
pub(crate) fn resolve_program(name: &str) -> Option<(PathBuf, bool)> {
    let path_var = std::env::var_os("PATH")?;
    let lower = name.to_ascii_lowercase();
    let has_ext = lower.ends_with(".exe")
        || lower.ends_with(".com")
        || lower.ends_with(".cmd")
        || lower.ends_with(".bat");
    let exts: &[&str] = if has_ext {
        &[""]
    } else {
        &[".exe", ".com", ".cmd", ".bat"]
    };
    for ext in exts {
        let candidate_name = format!("{name}{ext}");
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(&candidate_name);
            if candidate.is_file() {
                let is_batch = candidate
                    .extension()
                    .and_then(OsStr::to_str)
                    .is_some_and(|e| {
                        e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")
                    });
                return Some((candidate, is_batch));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn write_restricted_sandbox_allows_workspace_and_blocks_outside_writes() {
        let base = std::env::temp_dir().join(format!(
            "muse-sandbox-test-{}",
            super::super::token::generate()
        ));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        let temp_dir = base.join("job-tmp");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let cmd_exe = system_executable(r"System32\cmd.exe").unwrap();
        let outside_file = outside.join("escape.txt");
        let inside_file = workspace.join("ok.txt");

        // 1) 在写受限沙箱内写入工作区：必须成功
        let inside_proc = spawn(&SpawnConfig {
            application: cmd_exe.clone(),
            command_line: format!(
                "{} /d /s /c \"echo hello-sandbox> {}\"",
                quote_windows_arg(&strip_verbatim(&cmd_exe)),
                quote_windows_arg(&strip_verbatim(&inside_file))
            ),
            cwd: workspace.clone(),
            temp_dir: temp_dir.clone(),
            sandboxed: true,
            limits: JobLimits {
                memory_bytes: Some(DEFAULT_EXEC_MEMORY_BYTES),
                max_processes: Some(DEFAULT_EXEC_MAX_PROCESSES),
            },
        })
        .unwrap();
        let code = inside_proc.wait_timeout(10_000).unwrap().unwrap();
        assert_eq!(code, 0);
        assert!(
            std::fs::read_to_string(&inside_file)
                .unwrap()
                .contains("hello-sandbox")
        );

        // 2) 在写受限沙箱内尝试通过 cmd.exe 写入工作区外部：必须被 Windows 内核拒绝
        let mut outside_proc = spawn(&SpawnConfig {
            application: cmd_exe.clone(),
            command_line: format!(
                "{} /d /s /c \"echo pwn> {}\"",
                quote_windows_arg(&strip_verbatim(&cmd_exe)),
                quote_windows_arg(&strip_verbatim(&outside_file))
            ),
            cwd: workspace.clone(),
            temp_dir: temp_dir.clone(),
            sandboxed: true,
            limits: JobLimits {
                memory_bytes: Some(DEFAULT_EXEC_MEMORY_BYTES),
                max_processes: Some(DEFAULT_EXEC_MAX_PROCESSES),
            },
        })
        .unwrap();
        let code = outside_proc.wait_timeout(10_000).unwrap().unwrap();
        let mut err_buf = String::new();
        if let Some(mut stderr) = outside_proc.stderr.take() {
            let _ = stderr.read_to_string(&mut err_buf);
        }
        assert_ne!(code, 0, "expected outside write to fail, stderr={err_buf}");
        assert!(
            !outside_file.exists(),
            "sandboxed cmd.exe escaped workspace write boundary!"
        );

        // 3) 在写受限沙箱内尝试通过 powershell.exe 写入工作区外部：同样必须被内核拒绝
        if let Ok(ps_exe) = system_executable(r"System32\WindowsPowerShell\v1.0\powershell.exe") {
            let ps_outside = outside.join("ps-escape.txt");
            let ps_cmd = format!(
                "Set-Content -LiteralPath '{}' -Value 'pwn' -ErrorAction Stop",
                strip_verbatim(&ps_outside).replace('\'', "''")
            );
            let ps_proc = spawn(&SpawnConfig {
                application: ps_exe.clone(),
                command_line: build_windows_cmdline(
                    &strip_verbatim(&ps_exe),
                    &[
                        "-NoProfile".into(),
                        "-NonInteractive".into(),
                        "-Command".into(),
                        ps_cmd,
                    ],
                ),
                cwd: workspace.clone(),
                temp_dir: temp_dir.clone(),
                sandboxed: true,
                limits: JobLimits {
                    memory_bytes: Some(DEFAULT_EXEC_MEMORY_BYTES),
                    max_processes: Some(DEFAULT_EXEC_MAX_PROCESSES),
                },
            })
            .unwrap();
            let ps_code = ps_proc.wait_timeout(15_000).unwrap().unwrap();
            assert_ne!(ps_code, 0, "expected powershell outside write to fail");
            assert!(
                !ps_outside.exists(),
                "sandboxed powershell.exe escaped workspace write boundary!"
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}
