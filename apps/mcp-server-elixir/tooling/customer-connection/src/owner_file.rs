use std::{
    ffi::{CString, OsStr},
    fs::{File, Metadata},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path},
};

pub const MAX_BYTES: u64 = 8192;

pub fn read(path: &Path) -> Result<Vec<u8>, ()> {
    let (directory, name, uid) = private_parent(path, false)?;
    let mut file = open_at(&directory, &name, false)?;
    let before = file.metadata().map_err(|_| ())?;
    validate(&before, uid)?;
    if !no_acl(&file)? {
        return Err(());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    let after = file.metadata().map_err(|_| ())?;
    validate(&after, uid)?;
    if bytes.is_empty()
        || bytes.len() as u64 > MAX_BYTES
        || bytes.len() as u64 != after.len()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || !no_acl(&file)?
    {
        return Err(());
    }
    Ok(bytes)
}

pub fn install(path: &Path, bytes: &[u8]) -> Result<(), ()> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_BYTES {
        return Err(());
    }
    let (directory, name, uid) = private_parent(path, true)?;
    let name = CString::new(name.as_bytes()).map_err(|_| ())?;
    // SAFETY: descriptor/name are live. O_EXCL refuses any existing file/link,
    // and the kernel creates the file with at most owner read/write permissions.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(());
    }
    // SAFETY: openat returned a new, exclusively owned descriptor.
    let mut file = unsafe { File::from_raw_fd(fd) };
    let result = (|| {
        validate(&file.metadata().map_err(|_| ())?, uid)?;
        if !no_acl(&file)? {
            return Err(());
        }
        file.write_all(bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        directory.sync_all().map_err(|_| ())
    })();
    if result.is_err() {
        // SAFETY: removes only the newly created basename in the owned parent;
        // no existing file was overwritten and no directory is removed.
        unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    }
    result
}

fn private_parent(path: &Path, create: bool) -> Result<(File, std::ffi::OsString, u32), ()> {
    // SAFETY: these functions take no pointers and read the current OS identity.
    let (uid, euid) = unsafe { (libc::getuid(), libc::geteuid()) };
    if uid != euid || path.as_os_str().len() > 4096 || !path.is_absolute() {
        return Err(());
    }
    let mut parts = path.components();
    if parts.next() != Some(Component::RootDir) {
        return Err(());
    }
    let mut names = Vec::new();
    for part in parts {
        match part {
            Component::Normal(name) => names.push(name),
            _ => return Err(()),
        }
    }
    let (name, directories) = names.split_last().ok_or(())?;
    let mut directory = File::open("/").map_err(|_| ())?;
    for (index, name) in directories.iter().enumerate() {
        if create && index + 1 == directories.len() {
            let name = CString::new(name.as_bytes()).map_err(|_| ())?;
            // SAFETY: creates only the immediate parent under a checked directory
            // descriptor. Existing paths are still opened without following links.
            let status = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
            if status != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
                return Err(());
            }
        }
        directory = open_at(&directory, name, true)?;
        let metadata = directory.metadata().map_err(|_| ())?;
        // Shared sticky root-owned ancestors (e.g. /private/tmp) are safe only
        // before the mandatory private, current-user-owned immediate parent.
        let sticky_root = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
        if !metadata.is_dir()
            || ![0, uid].contains(&metadata.uid())
            || (metadata.mode() & 0o022 != 0 && !sticky_root)
        {
            return Err(());
        }
    }
    let parent = directory.metadata().map_err(|_| ())?;
    if parent.uid() != uid || parent.mode() & 0o7777 != 0o700 || !no_acl(&directory)? {
        return Err(());
    }
    Ok((directory, name.to_os_string(), uid))
}

fn validate(metadata: &Metadata, uid: u32) -> Result<(), ()> {
    if metadata.is_file()
        && metadata.uid() == uid
        && metadata.nlink() == 1
        && [0o400, 0o600].contains(&(metadata.mode() & 0o7777))
        && metadata.len() <= MAX_BYTES
    {
        Ok(())
    } else {
        Err(())
    }
}

fn open_at(parent: &File, name: &OsStr, directory: bool) -> Result<File, ()> {
    let name = CString::new(name.as_bytes()).map_err(|_| ())?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: parent is a live directory descriptor, name is NUL-terminated,
    // and no create flag is used. Descriptor-relative traversal never follows links.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(());
    }
    // SAFETY: openat returned a new, exclusively owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(target_os = "linux")]
fn no_acl(file: &File) -> Result<bool, ()> {
    // SAFETY: descriptor is live, attribute name is NUL-terminated, and a null
    // buffer with length zero requests only the attribute size.
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            std::ptr::null_mut(),
            0,
        )
    };
    if size >= 0 {
        return Ok(size == 0);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ENODATA | libc::ENOTSUP) => Ok(true),
        _ => Err(()),
    }
}

#[cfg(target_os = "macos")]
fn no_acl(file: &File) -> Result<bool, ()> {
    use std::ffi::c_void;
    unsafe extern "C" {
        fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> *mut c_void;
        fn acl_get_entry(acl: *mut c_void, id: libc::c_int, entry: *mut *mut c_void)
        -> libc::c_int;
        fn acl_free(acl: *mut c_void) -> libc::c_int;
    }
    // SAFETY: descriptor is live. Constants are ACL_TYPE_EXTENDED and
    // ACL_FIRST_ENTRY from macOS sys/acl.h. Every allocated ACL is freed.
    unsafe {
        let acl = acl_get_fd_np(file.as_raw_fd(), 0x100);
        if acl.is_null() {
            // Darwin's FILESEC_ACL property is absent (ENOENT) on a live
            // descriptor with no ACL; all other retrieval failures deny.
            return match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::ENOENT) => Ok(true),
                _ => Err(()),
            };
        }
        let mut entry = std::ptr::null_mut();
        let status = acl_get_entry(acl, 0, &mut entry);
        let error = std::io::Error::last_os_error().raw_os_error();
        acl_free(acl);
        // Darwin returns -1/EINVAL when an ACL has no first entry.
        match (status, error) {
            (-1, Some(libc::EINVAL)) => Ok(true),
            (0, _) => Ok(false),
            _ => Err(()),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("Customer connections support Linux and macOS only.");

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    #[test]
    fn current_os_owner_is_required_independently_of_private_mode() {
        let directory = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("owner-metadata-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join("owner"))
            .unwrap();
        let metadata = file.metadata().unwrap();
        assert_eq!(validate(&metadata, metadata.uid()), Ok(()));
        assert_eq!(validate(&metadata, metadata.uid() + 1), Err(()));
        let acl = no_acl(&file);
        std::fs::remove_dir_all(directory).unwrap();
        assert_eq!(acl, Ok(true));
    }
}
