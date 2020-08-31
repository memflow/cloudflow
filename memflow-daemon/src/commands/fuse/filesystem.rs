mod scopes;
use scopes::ConnectionScope;

use crate::error::{Error, Result};
use crate::state::{state_lock_sync, FileSystemHandle, KernelHandle};

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use log::info;

use fuse_mt::*;
use time::*;

use memflow_core::mem::phys_mem::PhysicalMemory;

pub type ChildrenList = Vec<Arc<Box<dyn FileSystemEntry>>>;

/// Trait describing an entry into the virtual filesystem.
pub trait FileSystemEntry: Send + Sync {
    /// The name of the entry
    fn name(&self) -> &str;

    /// Decides wether or not this entry is a leaf or node (file or folder)
    fn is_leaf(&self) -> bool;

    /// Returns the children of this entry if it is not a leaf
    fn children(&self) -> Option<ChildrenList> {
        None
    }

    /// Returns the size of the leaf in bytes
    fn size(&self) -> usize {
        0
    }

    /// Returns the writable state of this leaf
    fn is_writable(&self) -> bool {
        false
    }

    /// Tries to open the leaf
    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        Err(Error::Other("unable to open file"))
    }
}

/// Extension trait for Boxed `FileSystemEntry` structures
/// that enables recursive iteration over a path.
trait FileSystemTraverse {
    fn traverse_children<'a>(
        &self,
        path: &'a [&'a OsStr],
    ) -> (Option<Arc<Box<dyn FileSystemEntry>>>, &'a [&'a OsStr]);
}

impl<T: FileSystemEntry + ?Sized> FileSystemTraverse for Box<T> {
    fn traverse_children<'a>(
        &self,
        path: &'a [&'a OsStr],
    ) -> (Option<Arc<Box<dyn FileSystemEntry>>>, &'a [&'a OsStr]) {
        if let Some(children) = self.children() {
            for child in children.into_iter() {
                if child.name() == path[0] {
                    if path.len() > 1 {
                        return child.traverse_children(&path[1..]);
                    } else {
                        return (Some(child), path);
                    }
                }
            }
        }
        (None, path)
    }
}

/// Container structure that holds the children of a `FileSystemEntry`.
/// The child list will be invalidated every 5 seconds and
/// the closure is called again to retrieve a new set of elements.
struct FileSystemChildren {
    children: RefCell<Option<ChildrenList>>,
    last_refresh: RefCell<Instant>,
}

// TODO: remove those
unsafe impl Send for FileSystemChildren {}
unsafe impl Sync for FileSystemChildren {}

impl Default for FileSystemChildren {
    fn default() -> Self {
        Self {
            children: RefCell::new(None),
            last_refresh: RefCell::new(Instant::now()),
        }
    }
}

impl FileSystemChildren {
    /// Retrieves the current list of children or executes the closure
    /// to retrieve a new list of children and stores them internally.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::commands::fuse::filesystem::FileSystemChildren;
    ///
    /// let mut children = FileSystemChildren::default();
    ///
    /// let child_list = children.get_or_insert(|| {
    ///     vec![
    ///         Box::new(DriverRootFolder::new(self.kernel.clone())),
    ///         Box::new(ProcessRootFolder::new(self.kernel.clone())),
    ///         Box::new(PhysicalDumpFile::new(self.kernel.clone())),
    ///     ]
    /// });
    /// ```
    pub fn get_or_insert<F>(&self, insert: F) -> ChildrenList
    where
        F: FnOnce() -> Vec<Box<dyn FileSystemEntry>>,
    {
        {
            let mut children = self.children.borrow_mut();
            let mut last_refresh = self.last_refresh.borrow_mut();

            if children.is_none() || last_refresh.elapsed() > Duration::from_secs(5) {
                children.replace(
                    insert()
                        .into_iter()
                        .map(|f| Arc::new(f))
                        .collect::<Vec<_>>(),
                );
                *last_refresh = Instant::now();
            }
        }

        self.children.borrow().as_ref().unwrap().clone()
    }
}

/// Trait implementing basic read/write operations on an opened file.
pub trait FileSystemFileHandler {
    fn read(&mut self, offset: u64, size: u32) -> Result<Vec<u8>>;
    fn write(&mut self, _offset: u64, _data: Vec<u8>) -> Result<usize> {
        Err(Error::Other("unable to write to file"))
    }
}

/// This reader provides a basic implementation of a `FileSystemFileHandler`.
/// It will just return the contents being put into it when reading.
struct StaticFileReader {
    contents: String,
}

impl StaticFileReader {
    pub fn new(contents: &str) -> Self {
        Self {
            contents: contents.to_string(),
        }
    }

    pub fn from_string(contents: String) -> Self {
        Self { contents }
    }
}

impl FileSystemFileHandler for StaticFileReader {
    fn read(&mut self, offset: u64, size: u32) -> Result<Vec<u8>> {
        let contents = self.contents.as_bytes();

        let start = std::cmp::min((offset + 1) as usize, contents.len())
            .checked_sub(1)
            .ok_or(Error::Other("Reading from empty buffer"))?;
        let end = std::cmp::min((offset + size as u64) as usize, contents.len());

        Ok(contents[start..end].to_vec())
    }
}

/// Helper struct that contains all current file handles
struct FileHandles {
    file_handle: u64,
    handles: Vec<(u64, Arc<Mutex<Box<dyn FileSystemFileHandler>>>)>,
}

// TODO: remove those
unsafe impl Send for FileHandles {}
unsafe impl Sync for FileHandles {}

impl Default for FileHandles {
    fn default() -> Self {
        Self {
            file_handle: 0,
            handles: Vec::new(),
        }
    }
}

impl FileHandles {
    pub fn insert(&mut self, entry: Box<dyn FileSystemFileHandler>) -> u64 {
        self.file_handle += 1;
        self.handles
            .push((self.file_handle, Arc::new(Mutex::new(entry))));
        self.file_handle
    }

    pub fn get(&self, handle: u64) -> Option<Arc<Mutex<Box<dyn FileSystemFileHandler>>>> {
        self.handles
            .iter()
            .find(|h| h.0 == handle)
            .map(|h| h.1.clone())
    }

    pub fn remove(&mut self, handle: u64) {
        self.handles.retain(|h| h.0 != handle);
    }
}

/// The Virtual Memory File System
/// The VMFS will add and remove itself from the global state.
pub struct VirtualMemoryFileSystem {
    id: String,
    conn_id: String,
    mount_point: String,

    uid: u32,
    gid: u32,
    readonly: bool,

    root: Arc<Box<dyn FileSystemEntry>>,

    opened_files: RwLock<FileHandles>,
}

impl VirtualMemoryFileSystem {
    pub fn new(
        id: &str,
        conn_id: &str,
        mount_point: &str,
        kernel: KernelHandle,
        uid: u32,
        gid: u32,
    ) -> Self {
        let readonly = match &kernel {
            KernelHandle::Win32(kernel) => kernel.phys_mem.metadata().readonly,
        };

        Self {
            id: id.to_string(),
            conn_id: conn_id.to_string(),
            mount_point: mount_point.to_string(),

            uid,
            gid,
            readonly,

            root: Arc::new(Box::new(ConnectionScope::new(kernel))),

            opened_files: RwLock::new(FileHandles::default()),
        }
    }

    pub fn find_node(&self, path: &[&OsStr]) -> Option<Arc<Box<dyn FileSystemEntry>>> {
        if path.len() > 1 {
            // skip over root '/'
            let child = self.root.traverse_children(&path[1..]);
            child.0
        } else {
            Some(self.root.clone())
        }
    }
}

const TTL: Timespec = Timespec { sec: 1, nsec: 0 };

impl FilesystemMT for VirtualMemoryFileSystem {
    /// Called on mount, before any other function.
    fn init(&self, _req: RequestInfo) -> ResultEmpty {
        // grab state and insert the reference
        let mut state = state_lock_sync();
        if let Some(conn) = state.connection_mut(&self.conn_id) {
            conn.refcount += 1;
            state.file_systems.insert(
                self.id.clone(),
                FileSystemHandle::new(&self.id, &self.conn_id, &self.mount_point),
            );
        }
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
        let ospath = path.iter().collect::<Vec<_>>();
        if let Some(node) = self.find_node(ospath.as_slice()) {
            let now = time::get_time();
            if node.is_leaf() {
                let perm = if self.readonly || !node.is_writable() {
                    0o555
                } else {
                    0o755
                };
                Ok((
                    TTL,
                    FileAttr {
                        size: node.size() as u64,
                        blocks: 1, // TODO:
                        atime: now,
                        mtime: now,
                        ctime: now,
                        crtime: Timespec::new(0, 0),
                        kind: FileType::RegularFile,
                        perm,
                        nlink: 0, // TODO: ?
                        uid: self.uid,
                        gid: self.gid,
                        rdev: 0, // TODO:
                        flags: 0,
                    },
                ))
            } else {
                Ok((
                    TTL,
                    FileAttr {
                        size: 0,
                        blocks: 0,
                        atime: now,
                        mtime: now,
                        ctime: now,
                        crtime: Timespec::new(0, 0),
                        kind: FileType::Directory,
                        perm: 0o555,
                        nlink: 0, // TODO: ?
                        uid: self.uid,
                        gid: self.gid,
                        rdev: 0, // TODO:
                        flags: 0,
                    },
                ))
            }
        } else {
            //Err(libc::ENOSYS)
            Err(libc::ENOENT)
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
        let ospath = path.iter().collect::<Vec<_>>();
        if let Some(node) = self.find_node(ospath.as_slice()) {
            if node.is_leaf() {
                if let Ok(reader) = node.open() {
                    if let Ok(mut opened_files) = self.opened_files.write() {
                        let handle = opened_files.insert(reader);
                        if !self.readonly && node.is_writable() {
                            // return flags with requested flags
                            Ok((handle, flags))
                        } else {
                            // remove write options from flags
                            let masked_flags =
                                flags & !libc::O_WRONLY as u32 & !libc::O_RDWR as u32;
                            Ok((handle, masked_flags))
                        }
                    } else {
                        Err(libc::EIO)
                    }
                } else {
                    Err(libc::EIO)
                }
            } else {
                // open called on a folder?
                Err(libc::ENOENT)
            }
        } else {
            Err(libc::ENOENT)
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
        if let Ok(opened_files) = self.opened_files.read() {
            if let Some(file) = opened_files.get(fh) {
                if let Ok(buf) = file.lock().unwrap().read(offset, size) {
                    callback(Ok(buf.as_slice()))
                } else {
                    callback(Err(libc::EIO))
                }
            } else {
                callback(Err(libc::ENOENT))
            }
        } else {
            callback(Err(libc::EIO))
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
        // TODO: double check writability?
        if !self.readonly {
            if let Ok(opened_files) = self.opened_files.read() {
                if let Some(file) = opened_files.get(fh) {
                    if let Ok(bytes) = file.lock().unwrap().write(offset, data) {
                        Ok(bytes as u32)
                    } else {
                        Err(libc::EIO)
                    }
                } else {
                    // opened file
                    Err(libc::ENOENT)
                }
            } else {
                // lock guard
                Err(libc::EIO)
            }
        } else {
            // readonly
            Err(libc::EIO)
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
        if let Ok(mut opened_files) = self.opened_files.write() {
            opened_files.remove(fh);
            Ok(())
        } else {
            Err(libc::EIO)
        }
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
        let ospath = path.iter().collect::<Vec<_>>();
        if let Some(node) = self.find_node(ospath.as_slice()) {
            let mut result = Vec::new();
            if let Some(children) = node.children() {
                for child in (*children).iter() {
                    result.push(DirectoryEntry {
                        name: OsString::from(child.name()),
                        kind: if child.is_leaf() {
                            FileType::RegularFile
                        } else {
                            FileType::Directory
                        },
                    });
                }
            }
            Ok(result)
        } else {
            Err(libc::ENOENT)
        }
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
impl Drop for VirtualMemoryFileSystem {
    fn drop(&mut self) {
        // grab state and remove the reference
        let mut state = state_lock_sync();
        if state.file_systems.contains_key(&self.id) {
            info!(
                "closing virtual filesystem and removing reference from connection {}",
                self.conn_id
            );

            if let Some(conn) = state.connection_mut(&self.conn_id) {
                conn.refcount -= 1;
            }

            state.file_systems.remove(&self.id);
        }
    }
}
