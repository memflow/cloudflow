use filer::prelude::v1::*;

use fuse_mt::*;
use log::*;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use time::*;

pub struct FilerFs {
    node: CArcSome<Node>,
    mount_point: String,
    uid: u32,
    gid: u32,
    readonly: bool,
}

const TTL: Timespec = Timespec { sec: 1, nsec: 0 };
const FOPEN_DIRECT_IO: u32 = 1 << 0;

fn path_to_str(path: &Path) -> String {
    let ret = path
        .iter()
        .skip(1)
        .map(|s| s.to_string_lossy())
        .inspect(|s| println!("|s| {}", s))
        .collect::<Vec<_>>()
        .join("/");

    ret.strip_prefix("/").unwrap_or(&ret).to_string()
}

impl FilesystemMT for FilerFs {
    /// Called on mount, before any other function.
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        Ok(())
    }

    /// Called on filesystem unmount.
    fn destroy(&self, _req: RequestInfo) {
        // Nothing.
    }

    /// Get the attributes of a filesystem entry.
    ///
    /// * `fh`: a file handle if this is called on an open file.
    fn getattr(&self, _req: RequestInfo, path: &Path, _fh: Option<u64>) -> ResultEntry {
        let ospath = path_to_str(path);
        println!("META {}", ospath);
        match self.node.metadata(&ospath) {
            // TODO: handle readonly flags, directories
            // let masked_flags =
            //    flags & !libc::O_WRONLY as u32 & !libc::O_RDWR as u32;
            Ok(NodeMetadata {
                is_branch,
                has_read,
                has_write,
                size,
                ..
            }) => {
                let perm = if is_branch {
                    if self.readonly {
                        0o555
                    } else {
                        0o755
                    }
                } else {
                    let has_write = has_write && !self.readonly;
                    0o200 * has_write as u16 + 0o444 * has_read as u16
                };

                let now = time::get_time();

                Ok((
                    TTL,
                    FileAttr {
                        size,
                        blocks: 1, // TODO:
                        atime: now,
                        mtime: now,
                        ctime: now,
                        crtime: Timespec::new(0, 0),
                        kind: if is_branch {
                            FileType::Directory
                        } else {
                            FileType::RegularFile
                        },
                        perm,
                        nlink: 0, // TODO: ?
                        uid: self.uid,
                        gid: self.gid,
                        rdev: 0, // TODO:
                        flags: 0,
                    },
                ))
            }
            Err(_) => Err(libc::ENOENT),
        }
    }

    // The following operations in the FUSE C API are all one kernel call: setattr
    // We split them out to match the C API's behavior.

    /// Change the mode of a filesystem entry.
    ///
    /// * `fh`: a file handle if this is called on an open file.
    /// * `mode`: the mode to change the file to.
    fn chmod(&self, _req: RequestInfo, path: &Path, _fh: Option<u64>, _mode: u32) -> ResultEmpty {
        info!("chmod {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Change the owner UID and/or group GID of a filesystem entry.
    ///
    /// * `fh`: a file handle if this is called on an open file.
    /// * `uid`: user ID to change the file's owner to. If `None`, leave the UID unchanged.
    /// * `gid`: group ID to change the file's group to. If `None`, leave the GID unchanged.
    fn chown(
        &self,
        _req: RequestInfo,
        path: &Path,
        _fh: Option<u64>,
        _uid: Option<u32>,
        _gid: Option<u32>,
    ) -> ResultEmpty {
        info!("chown {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Set the length of a file.
    ///
    /// * `fh`: a file handle if this is called on an open file.
    /// * `size`: size in bytes to set as the file's length.
    fn truncate(
        &self,
        _req: RequestInfo,
        path: &Path,
        _fh: Option<u64>,
        _size: u64,
    ) -> ResultEmpty {
        info!("truncate {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Set timestamps of a filesystem entry.
    ///
    /// * `fh`: a file handle if this is called on an open file.
    /// * `atime`: the time of last access.
    /// * `mtime`: the time of last modification.
    fn utimens(
        &self,
        _req: RequestInfo,
        path: &Path,
        _fh: Option<u64>,
        _atime: Option<Timespec>,
        _mtime: Option<Timespec>,
    ) -> ResultEmpty {
        info!("utimens {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Set timestamps of a filesystem entry (with extra options only used on MacOS).
    #[allow(clippy::too_many_arguments)]
    fn utimens_macos(
        &self,
        _req: RequestInfo,
        path: &Path,
        _fh: Option<u64>,
        _crtime: Option<Timespec>,
        _chgtime: Option<Timespec>,
        _bkuptime: Option<Timespec>,
        _flags: Option<u32>,
    ) -> ResultEmpty {
        info!("utimens_macos {:?}", path);
        Err(libc::ENOSYS)
    }

    // END OF SETATTR FUNCTIONS

    /// Read a symbolic link.
    fn readlink(&self, _req: RequestInfo, path: &Path) -> ResultData {
        info!("readlink {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Create a special file.
    ///
    /// * `parent`: path to the directory to make the entry under.
    /// * `name`: name of the entry.
    /// * `mode`: mode for the new entry.
    /// * `rdev`: if mode has the bits `S_IFCHR` or `S_IFBLK` set, this is the major and minor numbers for the device file. Otherwise it should be ignored.
    fn mknod(
        &self,
        _req: RequestInfo,
        parent: &Path,
        name: &OsStr,
        _mode: u32,
        _rdev: u32,
    ) -> ResultEntry {
        info!("mknod {:?} {:?}", parent, name);
        Err(libc::ENOSYS)
    }

    /// Create a directory.
    ///
    /// * `parent`: path to the directory to make the directory under.
    /// * `name`: name of the directory.
    /// * `mode`: permissions for the new directory.
    fn mkdir(&self, _req: RequestInfo, parent: &Path, name: &OsStr, _mode: u32) -> ResultEntry {
        info!("mkdir {:?} {:?}", parent, name);
        Err(libc::ENOSYS)
    }

    /// Remove a file.
    ///
    /// * `parent`: path to the directory containing the file to delete.
    /// * `name`: name of the file to delete.
    fn unlink(&self, _req: RequestInfo, parent: &Path, name: &OsStr) -> ResultEmpty {
        info!("unlink {:?} {:?}", parent, name);
        Err(libc::ENOSYS)
    }

    /// Remove a directory.
    ///
    /// * `parent`: path to the directory containing the directory to delete.
    /// * `name`: name of the directory to delete.
    fn rmdir(&self, _req: RequestInfo, parent: &Path, name: &OsStr) -> ResultEmpty {
        info!("rmdir {:?} {:?}", parent, name);
        Err(libc::ENOSYS)
    }

    /// Create a symbolic link.
    ///
    /// * `parent`: path to the directory to make the link in.
    /// * `name`: name of the symbolic link.
    /// * `target`: path (may be relative or absolute) to the target of the link.
    fn symlink(
        &self,
        _req: RequestInfo,
        parent: &Path,
        name: &OsStr,
        target: &Path,
    ) -> ResultEntry {
        info!("symlink {:?} {:?} {:?}", parent, name, target);
        Err(libc::ENOSYS)
    }

    /// Rename a filesystem entry.
    ///
    /// * `parent`: path to the directory containing the existing entry.
    /// * `name`: name of the existing entry.
    /// * `newparent`: path to the directory it should be renamed into (may be the same as `parent`).
    /// * `newname`: name of the new entry.
    fn rename(
        &self,
        _req: RequestInfo,
        parent: &Path,
        name: &OsStr,
        newparent: &Path,
        newname: &OsStr,
    ) -> ResultEmpty {
        info!(
            "rename {:?} {:?} {:?} {:?}",
            parent, name, newparent, newname
        );
        Err(libc::ENOSYS)
    }

    /// Create a hard link.
    ///
    /// * `path`: path to an existing file.
    /// * `newparent`: path to the directory for the new link.
    /// * `newname`: name for the new link.
    fn link(
        &self,
        _req: RequestInfo,
        path: &Path,
        newparent: &Path,
        newname: &OsStr,
    ) -> ResultEntry {
        info!("link {:?} {:?} {:?}", path, newparent, newname);
        Err(libc::ENOSYS)
    }

    /// Open a file.
    ///
    /// * `path`: path to the file.
    /// * `flags`: one of `O_RDONLY`, `O_WRONLY`, or `O_RDWR`, plus maybe additional flags.
    ///
    /// Return a tuple of (file handle, flags). The file handle will be passed to any subsequent
    /// calls that operate on the file, and can be any value you choose, though it should allow
    /// your filesystem to identify the file opened even without any path info.
    fn open(&self, _req: RequestInfo, path: &Path, flags: u32) -> ResultOpen {
        let ospath = path_to_str(path);
        match self.node.open(&ospath) {
            // TODO: handle readonly flags, directories
            // let masked_flags =
            //    flags & !libc::O_WRONLY as u32 & !libc::O_RDWR as u32;
            Ok(handle) => Ok((handle as u64, flags | FOPEN_DIRECT_IO)),
            Err(_) => Err(libc::ENOENT),
        }
    }

    /// Read from a file.
    ///
    /// Note that it is not an error for this call to request to read past the end of the file, and
    /// you should only return data up to the end of the file (i.e. the number of bytes returned
    /// will be fewer than requested; possibly even zero). Do not extend the file in this case.
    ///
    /// * `path`: path to the file.
    /// * `fh`: file handle returned from the `open` call.
    /// * `offset`: offset into the file to start reading.
    /// * `size`: number of bytes to read.
    /// * `callback`: a callback that must be invoked to return the result of the operation: either
    ///    the result data as a slice, or an error code.
    ///
    /// Return the return value from the `callback` function.
    fn read(
        &self,
        _req: RequestInfo,
        _path: &Path,
        fh: u64,
        offset: u64,
        size: u32,
        callback: impl FnOnce(ResultSlice<'_>) -> CallbackResult,
    ) -> CallbackResult {
        // TODO: validate chunk size?
        let mut buf = vec![0; size as usize];
        let mut cursor = ObjCursor::from((&self.node, fh as usize));
        // TODO: proper error checking
        match cursor.seek(SeekFrom::Start(offset)) {
            Ok(off) if offset == off => match cursor.read(&mut buf) {
                Ok(read) => callback(Ok(&buf[..read])),
                Err(_) => callback(Err(libc::EIO)),
            },
            _ => callback(Err(libc::EIO)),
        }
    }

    /// Write to a file.
    ///
    /// * `path`: path to the file.
    /// * `fh`: file handle returned from the `open` call.
    /// * `offset`: offset into the file to start writing.
    /// * `data`: the data to write
    /// * `flags`:
    ///
    /// Return the number of bytes written.
    fn write(
        &self,
        _req: RequestInfo,
        _path: &Path,
        fh: u64,
        offset: u64,
        data: Vec<u8>,
        _flags: u32,
    ) -> ResultWrite {
        if self.readonly {
            return Err(libc::EIO);
        }

        let mut cursor = ObjCursor::from((&self.node, fh as usize));
        // TODO: proper error checking
        // TODO: rw perms checking?
        match cursor.seek(SeekFrom::Start(offset)) {
            Ok(off) if offset == off => match cursor.write(&data) {
                Ok(written) => Ok(written as u32),
                Err(_) => Err(libc::EIO),
            },
            _ => Err(libc::EIO),
        }
    }

    /// Called each time a program calls `close` on an open file.
    ///
    /// Note that because file descriptors can be duplicated (by `dup`, `dup2`, `fork`) this may be
    /// called multiple times for a given file handle. The main use of this function is if the
    /// filesystem would like to return an error to the `close` call. Note that most programs
    /// ignore the return value of `close`, though.
    ///
    /// * `path`: path to the file.
    /// * `fh`: file handle returned from the `open` call.
    /// * `lock_owner`: if the filesystem supports locking (`setlk`, `getlk`), remove all locks
    ///   belonging to this lock owner.
    fn flush(&self, _req: RequestInfo, path: &Path, _fh: u64, _lock_owner: u64) -> ResultEmpty {
        info!("flush {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Called when an open file is closed.
    ///
    /// There will be one of these for each `open` call. After `release`, no more calls will be
    /// made with the given file handle.
    ///
    /// * `path`: path to the file.
    /// * `fh`: file handle returned from the `open` call.
    /// * `flags`: the flags passed when the file was opened.
    /// * `lock_owner`: if the filesystem supports locking (`setlk`, `getlk`), remove all locks
    ///   belonging to this lock owner.
    /// * `flush`: whether pending data must be flushed or not.
    fn release(
        &self,
        _req: RequestInfo,
        _path: &Path,
        fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> ResultEmpty {
        self.node.close(fh as _).map_err(|_| libc::EIO)
    }

    /// Write out any pending changes of a file.
    ///
    /// When this returns, data should be written to persistent storage.
    ///
    /// * `path`: path to the file.
    /// * `fh`: file handle returned from the `open` call.
    /// * `datasync`: if `false`, also write metadata, otherwise just write file data.
    fn fsync(&self, _req: RequestInfo, path: &Path, _fh: u64, _datasync: bool) -> ResultEmpty {
        info!("fsync {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Open a directory.
    ///
    /// Analogous to the `opend` call.
    ///
    /// * `path`: path to the directory.
    /// * `flags`: file access flags. Will contain `O_DIRECTORY` at least.
    ///
    /// Return a tuple of (file handle, flags). The file handle will be passed to any subsequent
    /// calls that operate on the directory, and can be any value you choose, though it should
    /// allow your filesystem to identify the directory opened even without any path info.
    fn opendir(&self, _req: RequestInfo, _path: &Path, flags: u32) -> ResultOpen {
        Ok((1, flags))
    }

    /// Get the entries of a directory.
    ///
    /// * `path`: path to the directory.
    /// * `fh`: file handle returned from the `opendir` call.
    ///
    /// Return all the entries of the directory.
    fn readdir(&self, _req: RequestInfo, path: &Path, _fh: u64) -> ResultReaddir {
        let ospath = path_to_str(path);
        let mut result = Vec::new();
        let cb = &mut |entry: ListEntry| {
            result.push(DirectoryEntry {
                name: OsString::from(&*entry.name),
                kind: if entry.is_branch {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
            });
            true
        };
        self.node
            .list(&ospath, &mut cb.into())
            .map_err(|_| libc::ENOENT)?;
        Ok(result)
    }

    /// Close an open directory.
    ///
    /// This will be called exactly once for each `opendir` call.
    ///
    /// * `path`: path to the directory.
    /// * `fh`: file handle returned from the `opendir` call.
    /// * `flags`: the file access flags passed to the `opendir` call.
    fn releasedir(&self, _req: RequestInfo, _path: &Path, _fh: u64, _flags: u32) -> ResultEmpty {
        //info!("releasedir {:?}", path);
        //Err(libc::ENOSYS)
        Ok(())
    }

    /// Write out any pending changes to a directory.
    ///
    /// Analogous to the `fsync` call.
    fn fsyncdir(&self, _req: RequestInfo, path: &Path, _fh: u64, _datasync: bool) -> ResultEmpty {
        info!("fsyncdir {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Get filesystem statistics.
    ///
    /// * `path`: path to some folder in the filesystem.
    ///
    /// See the `Statfs` struct for more details.
    fn statfs(&self, _req: RequestInfo, path: &Path) -> ResultStatfs {
        info!("statfs {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Set a file extended attribute.
    ///
    /// * `path`: path to the file.
    /// * `name`: attribute name.
    /// * `value`: the data to set the value to.
    /// * `flags`: can be either `XATTR_CREATE` or `XATTR_REPLACE`.
    /// * `position`: offset into the attribute value to write data.
    fn setxattr(
        &self,
        _req: RequestInfo,
        path: &Path,
        name: &OsStr,
        _value: &[u8],
        _flags: u32,
        _position: u32,
    ) -> ResultEmpty {
        info!("setxattr {:?} {:?}", path, name);
        Err(libc::ENOSYS)
    }

    /// Get a file extended attribute.
    ///
    /// * `path`: path to the file
    /// * `name`: attribute name.
    /// * `size`: the maximum number of bytes to read.
    ///
    /// If `size` is 0, return `Xattr::Size(n)` where `n` is the size of the attribute data.
    /// Otherwise, return `Xattr::Data(data)` with the requested data.
    fn getxattr(&self, _req: RequestInfo, path: &Path, name: &OsStr, _size: u32) -> ResultXattr {
        info!("getxattr {:?} {:?}", path, name);
        Err(libc::ENOSYS)
    }

    /// List extended attributes for a file.
    ///
    /// * `path`: path to the file.
    /// * `size`: maximum number of bytes to return.
    ///
    /// If `size` is 0, return `Xattr::Size(n)` where `n` is the size required for the list of
    /// attribute names.
    /// Otherwise, return `Xattr::Data(data)` where `data` is all the null-terminated attribute
    /// names.
    fn listxattr(&self, _req: RequestInfo, path: &Path, _size: u32) -> ResultXattr {
        info!("listxattr {:?}", path);
        Err(libc::ENOSYS)
    }

    /// Remove an extended attribute for a file.
    ///
    /// * `path`: path to the file.
    /// * `name`: name of the attribute to remove.
    fn removexattr(&self, _req: RequestInfo, path: &Path, name: &OsStr) -> ResultEmpty {
        info!("removexattr {:?} {:?}", path, name);
        Err(libc::ENOSYS)
    }

    /// Check for access to a file.
    ///
    /// * `path`: path to the file.
    /// * `mask`: mode bits to check for access to.
    ///
    /// Return `Ok(())` if all requested permissions are allowed, otherwise return `Err(EACCES)`
    /// or other error code as appropriate (e.g. `ENOENT` if the file doesn't exist).
    fn access(&self, _req: RequestInfo, path: &Path, _mask: u32) -> ResultEmpty {
        info!("access {:?}", path);

        // TODO: build path structure and lazily evaluate it

        Err(libc::ENOSYS)
    }

    /// Create and open a new file.
    ///
    /// * `parent`: path to the directory to create the file in.
    /// * `name`: name of the file to be created.
    /// * `mode`: the mode to set on the new file.
    /// * `flags`: flags like would be passed to `open`.
    ///
    /// Return a `CreatedEntry` (which contains the new file's attributes as well as a file handle
    /// -- see documentation on `open` for more info on that).
    fn create(
        &self,
        _req: RequestInfo,
        parent: &Path,
        name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> ResultCreate {
        info!("create {:?} {:?}", parent, name);
        Err(libc::ENOSYS)
    }

    // getlk

    // setlk

    // bmap

    /// macOS only: Rename the volume.
    ///
    /// * `name`: new name for the volume
    #[cfg(target_os = "macos")]
    fn setvolname(&self, _req: RequestInfo, name: &OsStr) -> ResultEmpty {
        info!("create {:?}", name);
        Err(libc::ENOSYS)
    }

    // exchange (macOS only, undocumented)

    /// macOS only: Query extended times (bkuptime and crtime).
    ///
    /// * `path`: path to the file to get the times for.
    ///
    /// Return an `XTimes` struct with the times, or other error code as appropriate.
    #[cfg(target_os = "macos")]
    fn getxtimes(&self, _req: RequestInfo, path: &Path) -> ResultXTimes {
        info!("getxtimes {:?}", path);
        Err(libc::ENOSYS)
    }
}

/// Drops the filesystem and removes it from the global state.
impl Drop for FilerFs {
    fn drop(&mut self) {
        // TODO: drop all opened handles
    }
}

pub fn mount(node: CArcSome<Node>, mount_point: &str, uid: u32, gid: u32) -> Result<()> {
    if mount_point.is_empty() {
        return Err(ErrorKind::InvalidPath.into());
    }

    let mount_point = mount_point.to_string();

    info!("filesystem mounted at {}", mount_point);
    info!("please use 'umount' or 'fusermount -u' to unmount the filesystem");

    std::thread::spawn(move || {
        let opts = [
            "-o",
            &format!("auto_unmount,allow_other,uid={},gid={}", uid, gid),
        ];
        let mntopts = opts.iter().map(|o| o.as_ref()).collect::<Vec<&OsStr>>();

        // the filesystem will add itself into the global scope
        let vmfs = FilerFs {
            node,
            mount_point: mount_point.clone(),
            uid,
            gid,
            readonly: false,
        };

        // blocks until the fs is umounted
        fuse_mt::mount(fuse_mt::FuseMT::new(vmfs, 8), &mount_point, &mntopts).unwrap();
    });

    Ok(())
}
