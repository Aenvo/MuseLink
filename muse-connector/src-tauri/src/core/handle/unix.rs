//! Unix filesystem operations from an authorized root, using fd-relative openat traversal.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path};

use crate::core::path_guard::Resolved;

#[derive(Debug)]
pub(crate) enum Error {
    Missing,
    NotDirectory,
    IsDirectory,
    Escape,
    Changed,
    Exists,
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::Missing,
            io::ErrorKind::NotADirectory => Self::NotDirectory,
            io::ErrorKind::IsADirectory => Self::IsDirectory,
            io::ErrorKind::AlreadyExists => Self::Exists,
            _ if error.raw_os_error() == Some(libc::ELOOP) => Self::Escape,
            _ => Self::Io(error),
        }
    }
}

fn map_errno(errno: i32) -> Error {
    Error::from(io::Error::from_raw_os_error(errno))
}

fn last_error() -> Error {
    Error::from(io::Error::last_os_error())
}

fn cstring(name: &OsStr) -> Result<CString, Error> {
    CString::new(name.as_bytes()).map_err(|_| Error::Escape)
}

fn relative(target: &Resolved) -> Result<&Path, Error> {
    let canonical = target
        .path
        .strip_prefix(&target.root)
        .map_err(|_| Error::Escape)?;
    Ok(if target.reject_reparse {
        &target.requested_relative
    } else {
        canonical
    })
}

fn fstat(file: &File) -> Result<libc::stat, Error> {
    let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut st) } != 0 {
        return Err(last_error());
    }
    Ok(st)
}

fn path_stat(path: &Path) -> Result<libc::stat, Error> {
    let c_path = cstring(path.as_os_str())?;
    let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::stat(c_path.as_ptr(), &mut st) } != 0 {
        return Err(last_error());
    }
    Ok(st)
}

fn is_dir(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFDIR
}

fn is_reg(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFREG
}

fn is_lnk(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFLNK
}

fn not_regular_file() -> Error {
    Error::from(io::Error::new(
        io::ErrorKind::InvalidData,
        "not a regular file",
    ))
}

/// Pointer to this thread's errno cell.
///
/// glibc exposes it as `__errno_location()`; macOS (and the BSDs) call the
/// same cell `__error()`. Keeping the split in one helper leaves the
/// readdir clear/check sequence below platform-neutral.
fn errno_location() -> *mut libc::c_int {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::__errno_location()
    }
    #[cfg(not(target_os = "linux"))]
    unsafe {
        libc::__error()
    }
}

/// Read the process umask without permanently changing it.
fn current_umask() -> libc::mode_t {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                let Some(value) = line.strip_prefix("Umask:") else {
                    continue;
                };
                if let Ok(parsed) = libc::mode_t::from_str_radix(value.trim(), 8) {
                    return parsed;
                }
            }
        }
        0o022
    }
    #[cfg(not(target_os = "linux"))]
    {
        // No /proc on macOS: read the mask by swapping it to 0 and restoring
        // it immediately (the only portable query POSIX offers). The window
        // between the two calls is two syscalls; a file created by another
        // thread of this process inside that window could briefly get a
        // wider mode, but this backend always fchmod()s the files it creates
        // to their final mode, so its own outputs are unaffected. The
        // alternative (assuming 0o022) silently produces wrong modes for
        // every user whose umask differs.
        unsafe {
            let current = libc::umask(0);
            libc::umask(current);
            current
        }
    }
}

fn fchmod(file: &File, mode: libc::mode_t) -> Result<(), Error> {
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } != 0 {
        return Err(last_error());
    }
    Ok(())
}

fn fstatat_name(directory: &File, name: &OsStr) -> Result<libc::stat, Error> {
    let c_name = cstring(name)?;
    let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            c_name.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(last_error());
    }
    Ok(st)
}

fn owned_file(fd: libc::c_int) -> File {
    // SAFETY: caller transferred ownership of a valid open fd.
    unsafe { File::from(OwnedFd::from_raw_fd(fd)) }
}

fn open_flags(directory: bool) -> libc::c_int {
    let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    if directory {
        flags |= libc::O_DIRECTORY;
    } else {
        // Prevent FIFO/device opens from blocking forever when no peer is present.
        flags |= libc::O_NONBLOCK;
    }
    flags
}

fn open_at(parent: &File, name: &OsStr, directory: bool) -> Result<File, Error> {
    let c_name = cstring(name)?;
    let fd = unsafe { libc::openat(parent.as_raw_fd(), c_name.as_ptr(), open_flags(directory)) };
    if fd < 0 {
        let err = last_error();
        // Linux reports ENOTDIR (not ELOOP) for O_NOFOLLOW|O_DIRECTORY on a symlink.
        if directory && matches!(err, Error::NotDirectory) {
            let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
            if unsafe {
                libc::fstatat(
                    parent.as_raw_fd(),
                    c_name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } == 0
                && is_lnk(st.st_mode)
            {
                return Err(Error::Escape);
            }
        }
        return Err(err);
    }
    Ok(owned_file(fd))
}

fn open_path_absolute(path: &Path, directory: bool) -> Result<File, Error> {
    let c_path = cstring(path.as_os_str())?;
    let fd = unsafe { libc::open(c_path.as_ptr(), open_flags(directory)) };
    if fd < 0 {
        return Err(last_error());
    }
    Ok(owned_file(fd))
}

fn open_from_root(path: &Path) -> Result<File, Error> {
    let mut current = open_path_absolute(Path::new("/"), true)?;
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                current = open_at(&current, name, true)?;
            }
            _ => return Err(Error::Escape),
        }
    }
    Ok(current)
}

fn verify_root(file: &File, canonical_root: &Path) -> Result<(), Error> {
    let actual = fstat(file)?;
    if !is_dir(actual.st_mode) {
        return Err(Error::NotDirectory);
    }
    let expected = path_stat(canonical_root)?;
    if actual.st_dev != expected.st_dev || actual.st_ino != expected.st_ino {
        return Err(Error::Changed);
    }
    Ok(())
}

fn open_root(target: &Resolved) -> Result<File, Error> {
    // On macOS the requested (lexical) root can cross the OS alias symlinks
    // (/var -> /private/var etc.), which a nofollow walk must reject, so the
    // strict walk uses the canonical root instead. Its components are real
    // directories by construction, verify_root() below still pins the
    // opened fd to the canonical root's device/inode, and traversal below
    // the root stays nofollow, so a swapped component still fails closed.
    #[cfg(target_os = "macos")]
    let strict_root = &target.root;
    #[cfg(not(target_os = "macos"))]
    let strict_root = &target.requested_root;
    let file = if target.reject_reparse {
        open_from_root(strict_root)?
    } else {
        open_path_absolute(&target.root, true)?
    };
    verify_root(&file, &target.root)?;
    Ok(file)
}

fn normal_components(path: &Path) -> Result<Vec<&OsStr>, Error> {
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => names.push(name),
            Component::CurDir => continue,
            _ => return Err(Error::Escape),
        }
    }
    Ok(names)
}

fn traverse(root: &File, path: &Path, final_directory: bool) -> Result<File, Error> {
    let names = normal_components(path)?;
    if names.is_empty() {
        return if final_directory {
            root.try_clone().map_err(Error::from)
        } else {
            Err(Error::IsDirectory)
        };
    }
    let mut current = root.try_clone().map_err(Error::from)?;
    let last = names.len() - 1;
    for (index, name) in names.into_iter().enumerate() {
        let directory = index < last || final_directory;
        current = open_at(&current, name, directory)?;
    }
    if !final_directory {
        let st = fstat(&current)?;
        if is_dir(st.st_mode) {
            return Err(Error::IsDirectory);
        }
        if !is_reg(st.st_mode) {
            return Err(not_regular_file());
        }
    }
    Ok(current)
}

/// Confirm `directory` is still the same inode reached by walking `parent_path` from a fresh root.
fn revalidate_directory(
    target: &Resolved,
    parent_path: &Path,
    directory: &File,
) -> Result<(), Error> {
    let root = open_root(target)?;
    let rewalked = match traverse(&root, parent_path, true) {
        Ok(file) => file,
        Err(Error::Missing | Error::NotDirectory | Error::Escape) => return Err(Error::Changed),
        Err(error) => return Err(error),
    };
    let expected = fstat(directory)?;
    let actual = fstat(&rewalked)?;
    if expected.st_dev != actual.st_dev || expected.st_ino != actual.st_ino {
        return Err(Error::Changed);
    }
    Ok(())
}

pub(crate) fn read(target: &Resolved) -> Result<File, Error> {
    let path = relative(target)?;
    if path.as_os_str().is_empty() {
        return Err(Error::IsDirectory);
    }
    let root = open_root(target)?;
    traverse(&root, path, false)
}

pub(crate) struct Entry {
    pub(crate) name: OsString,
    pub(crate) size: u64,
    pub(crate) mtime: f64,
    pub(crate) is_directory: bool,
    pub(crate) is_reparse: bool,
}

pub(crate) fn list(target: &Resolved, mut visit: impl FnMut(Entry)) -> Result<(), Error> {
    let path = relative(target)?;
    let root = open_root(target)?;
    let directory = traverse(&root, path, true)?;

    // fdopendir takes ownership of the fd; duplicate so `directory` stays valid for cleanup.
    let dup = unsafe { libc::dup(directory.as_raw_fd()) };
    if dup < 0 {
        return Err(last_error());
    }
    let dir = unsafe { libc::fdopendir(dup) };
    if dir.is_null() {
        let err = last_error();
        unsafe {
            libc::close(dup);
        }
        return Err(err);
    }

    let result = (|| {
        loop {
            // readdir uses errno: clear before call, then distinguish end vs error.
            unsafe {
                *errno_location() = 0;
            }
            let entry = unsafe { libc::readdir(dir) };
            if entry.is_null() {
                let errno = unsafe { *errno_location() };
                if errno == 0 {
                    return Ok(());
                }
                return Err(map_errno(errno));
            }
            let record = unsafe { &*entry };
            let name = unsafe { CStr::from_ptr(record.d_name.as_ptr()) };
            let name_bytes = name.to_bytes();
            if name_bytes == b"." || name_bytes == b".." {
                continue;
            }
            let os_name = OsString::from_vec(name_bytes.to_vec());
            let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
            let rc = unsafe {
                libc::fstatat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if rc != 0 {
                let err = last_error();
                // Entry deleted between readdir and fstatat: skip it instead
                // of failing the whole listing.
                if matches!(err, Error::Missing) {
                    continue;
                }
                return Err(err);
            }
            let directory_entry = is_dir(st.st_mode);
            visit(Entry {
                name: os_name,
                size: if directory_entry {
                    0
                } else {
                    st.st_size.max(0) as u64
                },
                mtime: st.st_mtime as f64 + (st.st_mtime_nsec as f64) / 1_000_000_000.0,
                is_directory: directory_entry,
                is_reparse: is_lnk(st.st_mode),
            });
        }
    })();

    unsafe {
        libc::closedir(dir);
    }
    result
}

fn open_or_create_directory(parent: &File, name: &OsStr) -> Result<File, Error> {
    match open_at(parent, name, true) {
        Err(Error::Missing) => {
            let c_name = cstring(name)?;
            let rc = unsafe { libc::mkdirat(parent.as_raw_fd(), c_name.as_ptr(), 0o777) };
            if rc != 0 {
                let err = last_error();
                if matches!(err, Error::Exists) {
                    return open_at(parent, name, true);
                }
                return Err(err);
            }
            open_at(parent, name, true)
        }
        result => result,
    }
}

fn ensure_directory(target: &Resolved, path: &Path) -> Result<File, Error> {
    let mut current = open_root(target)?;
    for name in normal_components(path)? {
        current = open_or_create_directory(&current, name)?;
    }
    Ok(current)
}

pub(crate) fn mkdir(target: &Resolved) -> Result<(), Error> {
    ensure_directory(target, relative(target)?)?;
    Ok(())
}

fn unlink_at(directory: &File, name: &OsStr) -> Result<(), Error> {
    let c_name = cstring(name)?;
    if unsafe { libc::unlinkat(directory.as_raw_fd(), c_name.as_ptr(), 0) } != 0 {
        return Err(last_error());
    }
    Ok(())
}

pub(crate) fn write(target: &Resolved, bytes: &[u8]) -> Result<(), Error> {
    let path = relative(target)?;
    let name = path.file_name().ok_or(Error::IsDirectory)?;
    let parent_path = path.parent().ok_or(Error::Escape)?;
    let directory = ensure_directory(target, parent_path)?;
    revalidate_directory(target, parent_path, &directory)?;

    let mut existing_mode: Option<libc::mode_t> = None;
    match open_at(&directory, name, false) {
        Ok(existing) => {
            let mode = fstat(&existing)?.st_mode;
            if is_dir(mode) {
                return Err(Error::IsDirectory);
            }
            if !is_reg(mode) {
                return Err(not_regular_file());
            }
            existing_mode = Some(mode & 0o0777);
        }
        Err(Error::Missing) => (),
        Err(error) => return Err(error),
    }

    let (temp_name, mut temp) = (0..4)
        .find_map(|_| {
            let temp_name = format!(".muse-{}.tmp", crate::core::token::generate());
            let c_name = match cstring(OsStr::new(&temp_name)) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            // Mode 0o600 keeps the temp private while being written; fchmod defeats umask.
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    c_name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                let err = last_error();
                return if matches!(err, Error::Exists) {
                    None
                } else {
                    Some(Err(err))
                };
            }
            let file = owned_file(fd);
            if let Err(error) = fchmod(&file, 0o600) {
                let _ = unlink_at(&directory, OsStr::new(&temp_name));
                return Some(Err(error));
            }
            Some(Ok((temp_name, file)))
        })
        .ok_or(Error::Changed)??;

    let result = (|| {
        temp.write_all(bytes)?;
        temp.sync_all()?;
        revalidate_directory(target, parent_path, &directory)?;

        let final_mode = existing_mode.unwrap_or_else(|| 0o666 & !current_umask());
        fchmod(&temp, final_mode)?;

        let c_temp = cstring(OsStr::new(&temp_name))?;
        let c_final = cstring(name)?;
        if unsafe {
            libc::renameat(
                directory.as_raw_fd(),
                c_temp.as_ptr(),
                directory.as_raw_fd(),
                c_final.as_ptr(),
            )
        } != 0
        {
            return Err(last_error());
        }

        // Capture inode identity from the still-open temp fd, then confirm the final name.
        let temp_st = fstat(&temp)?;
        drop(temp);
        let final_st = fstatat_name(&directory, name)?;
        if temp_st.st_dev != final_st.st_dev || temp_st.st_ino != final_st.st_ino {
            return Err(Error::Changed);
        }
        revalidate_directory(target, parent_path, &directory)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = unlink_at(&directory, OsStr::new(&temp_name));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("muse-handle-{}", crate::core::token::generate()));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }

        fn target(&self, path: &str) -> Resolved {
            let roots = [("root".to_owned(), self.0.clone())].into();
            let mut target = crate::core::path_guard::resolve(&roots, "root", path).unwrap();
            target.reject_reparse = true;
            target
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn file_mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn roundtrip_write_read_list_mkdir() {
        let fixture = Fixture::new();
        let file_target = fixture.target("notes/hello.txt");
        write(&file_target, b"payload").unwrap();

        let mut file = read(&file_target).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"payload");

        let dir_target = fixture.target("notes");
        let mut names = Vec::new();
        list(&dir_target, |entry| names.push(entry.name)).unwrap();
        assert!(names.iter().any(|name| name == "hello.txt"));

        let nested = fixture.target("notes/extra");
        mkdir(&nested).unwrap();
        assert!(fixture.0.join("notes/extra").is_dir());
    }

    #[test]
    fn symlink_component_escapes_when_traversing() {
        let fixture = Fixture::new();
        std::fs::create_dir(fixture.0.join("real")).unwrap();
        std::fs::write(fixture.0.join("real/note.txt"), b"secret").unwrap();
        symlink(fixture.0.join("real"), fixture.0.join("link")).unwrap();

        let target = fixture.target("link/note.txt");
        assert!(matches!(read(&target), Err(Error::Escape)));
    }

    #[test]
    fn missing_file_maps_to_missing() {
        let fixture = Fixture::new();
        let target = fixture.target("absent.txt");
        assert!(matches!(read(&target), Err(Error::Missing)));
    }

    #[test]
    fn reading_a_directory_is_rejected() {
        let fixture = Fixture::new();
        let target = fixture.target("folder");
        mkdir(&target).unwrap();
        assert!(matches!(read(&target), Err(Error::IsDirectory)));
    }

    #[test]
    fn write_rejects_existing_directory_target() {
        let fixture = Fixture::new();
        let target = fixture.target("folder");
        mkdir(&target).unwrap();
        std::fs::write(target.path.join("note.txt"), b"original").unwrap();
        assert!(matches!(
            write(&target, b"replacement"),
            Err(Error::IsDirectory)
        ));
        assert_eq!(
            std::fs::read(target.path.join("note.txt")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn revalidate_directory_detects_moved_parent() {
        let fixture = Fixture::new();
        let outside =
            std::env::temp_dir().join(format!("muse-outside-{}", crate::core::token::generate()));
        std::fs::create_dir(&outside).unwrap();
        let _cleanup = scopeguard_remove_dir(outside.clone());

        let target = fixture.target("a/b/file.txt");
        let path = relative(&target).unwrap();
        let parent_path = path.parent().unwrap();
        let directory = ensure_directory(&target, parent_path).unwrap();

        assert!(revalidate_directory(&target, parent_path, &directory).is_ok());

        std::fs::rename(fixture.0.join("a"), outside.join("a")).unwrap();
        assert!(matches!(
            revalidate_directory(&target, parent_path, &directory),
            Err(Error::Changed)
        ));
    }

    fn scopeguard_remove_dir(path: PathBuf) -> impl Drop {
        struct Guard(PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        Guard(path)
    }

    #[test]
    fn write_preserves_existing_mode_0600() {
        let fixture = Fixture::new();
        let target = fixture.target("secret.txt");
        write(&target, b"first").unwrap();
        std::fs::set_permissions(&target.path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(file_mode(&target.path), 0o600);

        write(&target, b"second").unwrap();
        assert_eq!(file_mode(&target.path), 0o600);
        assert_eq!(std::fs::read(&target.path).unwrap(), b"second");
    }

    #[test]
    fn write_preserves_existing_mode_0755() {
        let fixture = Fixture::new();
        let target = fixture.target("tool.sh");
        write(&target, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&target.path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(file_mode(&target.path), 0o755);

        write(&target, b"#!/bin/sh\necho ok\n").unwrap();
        assert_eq!(file_mode(&target.path), 0o755);
    }

    #[test]
    fn write_drops_special_bits_on_replace() {
        let fixture = Fixture::new();
        let target = fixture.target("suid.sh");
        write(&target, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&target.path, std::fs::Permissions::from_mode(0o4755)).unwrap();
        assert_eq!(file_mode(&target.path), 0o4755);

        write(&target, b"#!/bin/sh\necho ok\n").unwrap();
        assert_eq!(file_mode(&target.path), 0o755);
    }

    #[test]
    fn write_new_file_uses_umask_default_not_0666() {
        let fixture = Fixture::new();
        let target = fixture.target("fresh.txt");
        write(&target, b"new").unwrap();
        let mode = file_mode(&target.path);
        assert_ne!(mode, 0o666);
        assert_eq!(mode, u32::from(0o666 & !current_umask()));
    }

    #[test]
    fn read_rejects_fifo_promptly() {
        let fixture = Fixture::new();
        let fifo_path = fixture.0.join("pipe.fifo");
        let c_path = CString::new(fifo_path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let target = fixture.target("pipe.fifo");
        // Pre-fix this hung forever on a FIFO with no writer; must return promptly.
        assert!(read(&target).is_err());
    }

    #[test]
    fn write_rejects_fifo_target() {
        let fixture = Fixture::new();
        let fifo_path = fixture.0.join("pipe.fifo");
        let c_path = CString::new(fifo_path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let target = fixture.target("pipe.fifo");
        assert!(write(&target, b"nope").is_err());
    }
}
