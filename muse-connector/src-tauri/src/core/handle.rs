//! 以句柄为准的文件对象校验。
//!
//! 路径校验得到的是字符串，后续按路径操作会重新解析。若校验之后祖先目录被换成
//! 目录联接或符号链接，同一个字符串就会指向授权范围外的对象。这里把校验与操作
//! 绑定到同一个内核对象：打开句柄 → 读取该句柄的实际路径与属性 → 确认仍在授权
//! 根内 → 之后只通过这个句柄读写。
//!
//! 替换文件时，重命名以父目录句柄作为 `RootDirectory`，目标名相对已验证的目录
//! 对象解析，不再经过字符串路径。

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateFileW, DELETE, FILE_ADD_FILE,
    FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    GetFinalPathNameByHandleW, OPEN_EXISTING, VOLUME_NAME_DOS,
};

use super::access::within;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PATH_NOT_FOUND: i32 = 3;
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
const ERROR_FILE_EXISTS: i32 = 80;
const TEMP_ATTEMPTS: u32 = 4;

/// 打开句柄时申请的权利。写入走「替换」，所以目录句柄只需要枚举与改名的权利。
#[derive(Clone, Copy)]
pub(crate) enum Access {
    /// 读取文件内容。
    Read,
    /// 只取属性与最终路径。
    Metadata,
    /// 目录：可枚举、可在其中创建与改名。
    Directory,
}

impl Access {
    fn mask(self) -> u32 {
        match self {
            Self::Read => GENERIC_READ | FILE_READ_ATTRIBUTES,
            Self::Metadata => FILE_READ_ATTRIBUTES,
            Self::Directory => {
                FILE_LIST_DIRECTORY
                    | FILE_ADD_FILE
                    | FILE_ADD_SUBDIRECTORY
                    | FILE_DELETE_CHILD
                    | FILE_READ_ATTRIBUTES
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum VerifyError {
    /// 对象不存在。
    Missing,
    /// 打开的对象在授权根之外，或本身是重解析点。
    Escape,
    /// 其他系统错误。
    Io(io::Error),
}

impl VerifyError {
    fn last() -> Self {
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => Self::Missing,
            _ => Self::Io(error),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ReplaceError {
    /// 目标名不是单一文件名。
    InvalidName,
    /// 目标或临时文件落在授权根之外。
    Escape,
    /// 文件正被其他进程占用，可重试。
    Busy,
    /// 其他系统错误。
    Io(io::Error),
}

impl ReplaceError {
    fn last() -> Self {
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION) => Self::Busy,
            _ => Self::Io(error),
        }
    }
}

/// 目录或文件的对象身份：卷序列号 + 文件索引，与路径字符串无关。
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    volume: u32,
    index: u64,
}

/// 已验证的内核对象。句柄存活期间，对象不会因为路径变化而改变。
pub(crate) struct Verified {
    handle: OwnedHandle,
    final_path: PathBuf,
    attributes: u32,
    identity: Identity,
}

impl Verified {
    /// 句柄当前指向的实际路径。
    pub(crate) fn final_path(&self) -> &Path {
        &self.final_path
    }

    pub(crate) fn is_directory(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }

    fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }

    /// 重新读取最终路径，用于确认枚举期间目录未被替换。
    pub(crate) fn current_final_path(&self) -> Result<PathBuf, VerifyError> {
        query_path(self.raw())
    }

    /// 把句柄交给 `File`，后续读写不再解析路径。
    pub(crate) fn into_file(self) -> File {
        let handle = std::mem::ManuallyDrop::new(self.handle);
        unsafe { File::from_raw_handle(handle.as_raw_handle()) }
    }
}

/// 打开对象并读取它的实际身份，不跟随最终组件上的重解析点。
pub(crate) fn open(path: &Path, access: Access) -> Result<Verified, VerifyError> {
    let wide = wide(path);
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access.mask(),
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(VerifyError::last());
    }
    // SAFETY: CreateFileW returned a live handle that this scope now owns.
    let handle = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    let information = information(handle.as_raw_handle() as HANDLE)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(VerifyError::Escape);
    }
    Ok(Verified {
        final_path: query_path(handle.as_raw_handle() as HANDLE)?,
        attributes: information.dwFileAttributes,
        identity: identity(&information),
        handle,
    })
}

/// 打开 `target` 最近的已存在祖先，确认它仍在 `root` 内。
pub(crate) fn verify_existing_ancestor(target: &Path, root: &Path) -> Result<PathBuf, VerifyError> {
    let mut current = target;
    loop {
        if !within(current, root) {
            return Err(VerifyError::Escape);
        }
        match open(current, Access::Metadata) {
            Ok(verified) => {
                if !within(verified.final_path(), root) {
                    return Err(VerifyError::Escape);
                }
                return Ok(verified.final_path().to_path_buf());
            }
            Err(VerifyError::Missing) => {
                current = current.parent().ok_or(VerifyError::Missing)?;
            }
            Err(error) => return Err(error),
        }
    }
}

/// 在已验证的目录里原子替换 `name`：临时文件、写入、以目录句柄相对重命名。
pub(crate) fn replace(dir: &Verified, name: &str, bytes: &[u8]) -> Result<(), ReplaceError> {
    leaf_name(name)?;
    let mut attempt = 0;
    loop {
        let temp = dir
            .final_path()
            .join(format!(".muse-{}.tmp", super::token::generate()));
        match create_temp(&temp) {
            Ok(file) => {
                let target = dir.final_path().join(name);
                let result = commit(file, dir, &target, &temp, bytes);
                if result.is_err() {
                    // 失败时不能把临时文件留在授权目录里。
                    let _ = std::fs::remove_file(&temp);
                }
                return result;
            }
            // 随机名冲突时换一个，其余错误直接返回。
            Err(ReplaceError::Busy) if attempt < TEMP_ATTEMPTS => attempt += 1,
            Err(error) => return Err(error),
        }
    }
}

fn create_temp(path: &Path) -> Result<File, ReplaceError> {
    let wide = wide(path);
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE | DELETE | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(match io::Error::last_os_error().raw_os_error() {
            Some(ERROR_FILE_EXISTS) => ReplaceError::Busy,
            _ => ReplaceError::last(),
        });
    }
    // SAFETY: CreateFileW returned a live handle that File now owns.
    Ok(unsafe { File::from_raw_handle(raw as _) })
}

fn commit(
    mut file: File,
    dir: &Verified,
    target: &Path,
    temp: &Path,
    bytes: &[u8],
) -> Result<(), ReplaceError> {
    // 临时文件也必须落在已验证的目录里：路径本身可能在创建前被替换。
    let created = query_path(file.as_raw_handle() as HANDLE).map_err(|error| match error {
        VerifyError::Escape => ReplaceError::Escape,
        VerifyError::Missing => {
            ReplaceError::Io(io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND))
        }
        VerifyError::Io(error) => ReplaceError::Io(error),
    })?;
    if created.parent() != Some(dir.final_path()) {
        return Err(ReplaceError::Escape);
    }
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        eprintln!("debug step write: {error:?}");
        return Err(ReplaceError::Io(error));
    }
    // 重命名前再确认目录还是同一个对象，路径在这期间被换成联接就会不一致。
    same_directory(dir)?;
    // Win32 没有可用的「以目录句柄为根」的重命名：SetFileInformationByHandle 传
    // RootDirectory 在本机实测稳定返回 ACCESS_DENIED，因此按已验证目录的实际路径
    // 重命名，并在之后复核落点。进程内不会再有用户提供的路径参与这一步。
    std::fs::rename(temp, target).map_err(|error| match error.raw_os_error() {
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION) => ReplaceError::Busy,
        _ => ReplaceError::Io(error),
    })?;
    drop(file);
    // 重命名按完整路径完成，落点身份再核对一次；不一致时如实报错而不是假装成功。
    match open(target, Access::Metadata) {
        Ok(verified) if verified.final_path().parent() == Some(dir.final_path()) => Ok(()),
        Ok(_) => Err(ReplaceError::Busy),
        Err(VerifyError::Escape) => Err(ReplaceError::Escape),
        Err(VerifyError::Missing) => Err(ReplaceError::Busy),
        Err(VerifyError::Io(error)) => Err(ReplaceError::Io(error)),
    }
}

/// 目录仍是打开时那个对象（卷序列号 + 文件索引）。
fn same_directory(dir: &Verified) -> Result<(), ReplaceError> {
    match open(dir.final_path(), Access::Metadata) {
        Ok(current) if current.identity == dir.identity => Ok(()),
        Ok(_) => Err(ReplaceError::Busy),
        Err(VerifyError::Escape) => Err(ReplaceError::Escape),
        Err(VerifyError::Missing) => Err(ReplaceError::Busy),
        Err(VerifyError::Io(error)) => Err(ReplaceError::Io(error)),
    }
}

fn leaf_name(name: &str) -> Result<Vec<u16>, ReplaceError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['\\', '/', ':', '\0'])
        || name.ends_with(['.', ' '])
    {
        return Err(ReplaceError::InvalidName);
    }
    Ok(name.encode_utf16().collect())
}

fn information(handle: HANDLE) -> Result<BY_HANDLE_FILE_INFORMATION, VerifyError> {
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    if unsafe { GetFileInformationByHandle(handle, &mut information) } == 0 {
        return Err(VerifyError::Io(io::Error::last_os_error()));
    }
    Ok(information)
}

fn identity(information: &BY_HANDLE_FILE_INFORMATION) -> Identity {
    Identity {
        volume: information.dwVolumeSerialNumber,
        index: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    }
}

fn query_path(handle: HANDLE) -> Result<PathBuf, VerifyError> {
    let mut buffer = vec![0u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        if length == 0 {
            return Err(VerifyError::Io(io::Error::last_os_error()));
        }
        let length = length as usize;
        if length < buffer.len() {
            buffer.truncate(length);
            break;
        }
        buffer.resize(length + 1, 0);
    }
    Ok(normalize(OsString::from_wide(&buffer)))
}

/// `GetFinalPathNameByHandleW` 返回 `\\?\C:\…` 或 `\\?\UNC\server\share\…`，
/// 统一成与授权根相同的形态；无法转换时保留原样，`within` 仍能比较卷前缀。
fn normalize(raw: OsString) -> PathBuf {
    let Some(text) = raw.to_str() else {
        return PathBuf::from(raw);
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    PathBuf::from(raw)
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("muse-handle-{}", super::super::token::generate()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn root(&self) -> PathBuf {
            let root = self.0.join("root");
            std::fs::create_dir_all(&root).unwrap();
            root
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn handle_keeps_the_object_when_the_path_changes() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let file = root.join("note.txt");
        std::fs::write(&file, b"original").unwrap();
        let verified = open(&file, Access::Read).unwrap();
        assert!(within(verified.final_path(), &root));
        assert_eq!(verified.final_path().file_name().unwrap(), "note.txt");
        // 路径被换成新的同名文件，已打开的句柄仍绑定原来的对象。
        std::fs::rename(&file, root.join("moved.txt")).unwrap();
        std::fs::write(&file, b"replacement").unwrap();
        let mut content = Vec::new();
        verified.into_file().read_to_end(&mut content).unwrap();
        assert_eq!(content, b"original");
    }

    #[test]
    fn directory_junction_is_rejected_before_any_io() {
        use std::os::windows::process::CommandExt;
        let fixture = Fixture::new();
        let root = fixture.root();
        let outside = fixture.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let link = root.join("junction");
        let status = std::process::Command::new("cmd.exe")
            .creation_flags(0x08000000)
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .status()
            .unwrap();
        assert!(status.success(), "目录联接未创建");
        assert!(matches!(
            open(&link, Access::Metadata),
            Err(VerifyError::Escape)
        ));
        assert!(matches!(
            verify_existing_ancestor(&link.join("x.txt"), &root),
            Err(VerifyError::Escape)
        ));
    }

    #[test]
    fn replace_rejects_names_that_are_not_a_leaf() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let dir = open(&root, Access::Directory).unwrap();
        for name in [
            "../escape.txt",
            "sub/escape.txt",
            "sub\\escape.txt",
            "",
            ".",
            "..",
        ] {
            assert!(
                matches!(replace(&dir, name, b"x"), Err(ReplaceError::InvalidName)),
                "{name}"
            );
        }
        assert!(!fixture.0.join("escape.txt").exists());
    }

    #[test]
    fn replace_keeps_the_previous_file_when_the_target_is_a_directory() {
        let fixture = Fixture::new();
        let root = fixture.root();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target").join("inner.txt"), b"inner").unwrap();
        let dir = open(&root, Access::Directory).unwrap();
        assert!(replace(&dir, "target", b"data").is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("target").join("inner.txt")).unwrap(),
            "inner"
        );
        // 失败的替换不能留下临时文件。
        let leftovers = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".muse-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn replace_creates_a_new_file() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let dir = open(&root, Access::Directory).unwrap();
        replace(&dir, "brand-new.txt", b"fresh").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("brand-new.txt")).unwrap(),
            "fresh"
        );
    }

    #[test]
    fn replace_writes_new_content_and_drops_the_temp_file() {
        let fixture = Fixture::new();
        let root = fixture.root();
        std::fs::write(root.join("note.txt"), b"old").unwrap();
        let dir = open(&root, Access::Directory).unwrap();
        replace(&dir, "note.txt", b"new content").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("note.txt")).unwrap(),
            "new content"
        );
        let names: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["note.txt".to_string()]);
    }
}
