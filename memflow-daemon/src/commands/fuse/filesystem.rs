use crate::state::{state_lock_sync, KernelHandle};

use bitfield::bitfield;
use std::collections::HashMap;
use std::time::{Duration, Instant, UNIX_EPOCH};

use log::{info, trace};

use memflow_win32::*;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use std::ffi::OsStr;

// 1 second file system ttl
const TTL: Duration = Duration::from_secs(1);

bitfield! {
    pub struct INode(u64);
    impl Debug;
    pub pid, set_pid: 31, 0;
    pub mid, set_mid: 63, 32;
}

enum VirtualEntry {
    Folder(VirtualFolder),
    File(VirtualFile),
}

impl VirtualEntry {
    pub fn inode(&self) -> u64 {
        match self {
            VirtualEntry::Folder(folder) => folder.inode,
            VirtualEntry::File(file) => file.inode,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            VirtualEntry::Folder(folder) => &folder.name,
            VirtualEntry::File(file) => &file.name,
        }
    }
}

struct VirtualFolder {
    pub inode: u64,
    pub name: String,
    pub children: Vec<u64>,
}

impl VirtualFolder {
    pub fn new(inode: u64, name: &str) -> Self {
        Self {
            inode,
            name: name.to_string(),
            children: Vec::new(),
        }
    }
}

struct VirtualFile {
    pub inode: u64,
    pub name: String,

    // TODO: callbacks for size/contents/etc
    pub get_size: Box<dyn Fn() -> u64>,
    pub get_contents: Box<dyn Fn() -> Vec<u8>>,
}

pub struct VirtualMemoryFileSystem {
    id: String,
    conn_id: String,

    uid: u32,
    gid: u32,

    last_refresh: Instant,
    file_system: HashMap<u64, VirtualEntry>,
}

unsafe impl Send for VirtualMemoryFileSystem {}

impl VirtualMemoryFileSystem {
    pub fn new(id: &str, conn_id: &str, uid: u32, gid: u32) -> Self {
        let file_system = Self::create_root_folder(&conn_id);
        Self {
            id: id.to_string(),
            conn_id: conn_id.to_string(),

            uid,
            gid,

            last_refresh: Instant::now(),
            file_system,
        }
    }

    fn update_file_system(&mut self) {
        if self.last_refresh.elapsed() > Duration::from_secs(10) {
            self.file_system = Self::create_root_folder(&self.conn_id);
            self.last_refresh = Instant::now();
        }
    }

    fn create_root_folder(conn_id: &str) -> HashMap<u64, VirtualEntry> {
        // TODO: incremental updates for changed pids
        let mut fs = HashMap::new();

        let mut root = VirtualFolder::new(0, conn_id);

        let mut state = state_lock_sync();
        if let Some(conn) = state.connection_mut(conn_id) {
            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    if let Ok(process_info) = kernel.process_info_list() {
                        for pi in process_info.iter() {
                            root.children.push(Self::create_process_folder(pi, &mut fs));
                        }
                    }
                }
            }
        }

        fs.insert(root.inode, VirtualEntry::Folder(root));

        fs
    }

    fn create_process_folder(pi: &Win32ProcessInfo, fs: &mut HashMap<u64, VirtualEntry>) -> u64 {
        let mut inode = INode(0);
        inode.set_pid(pi.pid as u64);

        let mut prc = VirtualFolder::new(
            inode.0,
            &format!("{}_{}", pi.pid, pi.name.replace(".", "_")),
        );

        // add virtual folder inside of process?
        inode.set_mid(inode.mid() + 1);
        let modules = VirtualFolder::new(inode.0, "modules");
        fs.insert(inode.0, VirtualEntry::Folder(modules));
        prc.children.push(inode.0);

        inode.set_mid(inode.mid() + 1);
        let info = VirtualFile {
            inode: inode.0,
            name: "info".to_string(),
            get_size: Box::new(|| -> u64 { "this is a test\nthis is another test\n".len() as u64 }),
            get_contents: Box::new(|| -> Vec<u8> {
                "this is a test\nthis is another test\n".as_bytes().to_vec()
            }),
        };
        fs.insert(inode.0, VirtualEntry::File(info));
        prc.children.push(inode.0);

        let prc_inode = prc.inode;
        fs.insert(prc_inode, VirtualEntry::Folder(prc));
        prc_inode
    }
}

impl Filesystem for VirtualMemoryFileSystem {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        self.update_file_system();

        if let Some(entry) = self.file_system.get(&(parent - 1)) {
            info!(
                "lookup(): found file system entry: {} {}",
                entry.inode(),
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(folder) => {
                    // match child entries by name
                    // TODO: maybe add a map?
                    for child in folder.children.iter() {
                        if let Some(child_entry) = self.file_system.get(&child) {
                            // TODO: improve this check
                            if child_entry.name() == name.to_string_lossy() {
                                trace!(
                                    "lookup(): found child entry: {} {}",
                                    child_entry.inode(),
                                    child_entry.name()
                                );
                                match child_entry {
                                    VirtualEntry::Folder(child_folder) => {
                                        reply.entry(
                                            &TTL,
                                            &FileAttr {
                                                ino: 1 + child_folder.inode,
                                                size: 0,
                                                blocks: 0,
                                                atime: UNIX_EPOCH,
                                                mtime: UNIX_EPOCH,
                                                ctime: UNIX_EPOCH,
                                                crtime: UNIX_EPOCH,
                                                kind: FileType::Directory,
                                                perm: 0o755,
                                                nlink: 2,
                                                uid: self.uid,
                                                gid: self.gid,
                                                rdev: 0,
                                                flags: 0,
                                            },
                                            0,
                                        );
                                    }
                                    VirtualEntry::File(child_file) => {
                                        reply.entry(
                                            &TTL,
                                            &FileAttr {
                                                ino: 1 + child_file.inode,
                                                size: (child_file.get_size)(),
                                                blocks: 1, // TODO:
                                                atime: UNIX_EPOCH,
                                                mtime: UNIX_EPOCH,
                                                ctime: UNIX_EPOCH,
                                                crtime: UNIX_EPOCH,
                                                kind: FileType::RegularFile,
                                                perm: 0o644,
                                                nlink: 1,
                                                uid: self.uid,
                                                gid: self.gid,
                                                rdev: 0,
                                                flags: 0,
                                            },
                                            0,
                                        );
                                    }
                                }

                                // early return, we found our entry
                                return;
                            }
                        }
                    }
                }
                VirtualEntry::File(_) => {
                    // TODO: should not happen in readdir - print warn
                    reply.error(ENOENT);
                }
            }
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        self.update_file_system();

        if let Some(entry) = self.file_system.get(&(ino - 1)) {
            info!(
                "getattr(): found file system entry: {} {}",
                entry.inode(),
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(folder) => {
                    reply.attr(
                        &TTL,
                        &FileAttr {
                            ino: 1 + folder.inode,
                            size: 0,
                            blocks: 0,
                            atime: UNIX_EPOCH,
                            mtime: UNIX_EPOCH,
                            ctime: UNIX_EPOCH,
                            crtime: UNIX_EPOCH,
                            kind: FileType::Directory,
                            perm: 0o755,
                            nlink: 2,
                            uid: self.uid,
                            gid: self.gid,
                            rdev: 0,
                            flags: 0,
                        },
                    );
                }
                VirtualEntry::File(file) => {
                    reply.attr(
                        &TTL,
                        &FileAttr {
                            ino: 1 + file.inode,
                            size: 13,  // TODO:
                            blocks: 1, // TODO:
                            atime: UNIX_EPOCH,
                            mtime: UNIX_EPOCH,
                            ctime: UNIX_EPOCH,
                            crtime: UNIX_EPOCH,
                            kind: FileType::RegularFile,
                            perm: 0o644,
                            nlink: 1,
                            uid: self.uid,
                            gid: self.gid,
                            rdev: 0,
                            flags: 0,
                        },
                    );
                }
            }
        } else {
            reply.error(ENOENT);
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        _size: u32,
        reply: ReplyData,
    ) {
        println!(
            "read: ino={}, fh={}, offset={} size={}",
            ino, _fh, offset, _size
        );

        if let Some(entry) = self.file_system.get(&(ino - 1)) {
            info!(
                "getattr(): found file system entry: {} {}",
                entry.inode(),
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(_folder) => {
                    // should not happen
                    reply.error(ENOENT);
                }
                VirtualEntry::File(file) => {
                    // get file contents :)
                    let contents = (file.get_contents)();
                    reply.data(&contents.as_slice()[offset as usize..]);
                }
            }
        } else {
            reply.error(ENOENT);
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        self.update_file_system();

        if let Some(entry) = self.file_system.get(&(ino - 1)) {
            info!(
                "readdir(): found file system entry: {} {}",
                entry.inode(),
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(folder) => {
                    let mut entries = vec![
                        (1, FileType::Directory, ".".to_string()),
                        (1, FileType::Directory, "..".to_string()),
                    ];

                    // find each child entry and add them to the list
                    for child in folder.children.iter() {
                        if let Some(child_entry) = self.file_system.get(&child) {
                            match child_entry {
                                VirtualEntry::Folder(child_folder) => {
                                    trace!(
                                        "readdir(): adding child folder: {} {}",
                                        child_folder.inode,
                                        child_folder.name
                                    );
                                    entries.push((
                                        1 + child_folder.inode,
                                        FileType::Directory,
                                        child_folder.name.clone(),
                                    ));
                                }
                                VirtualEntry::File(child_file) => {
                                    trace!(
                                        "readdir(): adding child file: {} {}",
                                        child_file.inode,
                                        child_file.name
                                    );
                                    entries.push((
                                        1 + child_file.inode,
                                        FileType::RegularFile,
                                        child_file.name.clone(),
                                    ));
                                }
                            }
                        }
                    }

                    // send entries to fuse
                    for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
                        // i + 1 means the index of the next entry
                        reply.add(entry.0, (i + 1) as i64, entry.1, entry.2);
                    }

                    reply.ok();
                }
                VirtualEntry::File(_) => {
                    // TODO: should not happen in readdir - print warn
                    reply.error(ENOENT);
                }
            }
        } else {
            reply.error(ENOENT);
        }
    }
}

/// Spawns a new thread which will remove all information
/// about the filesystem from the global STATE.
///
/// This drop is really just used in the case
/// where the user umounted the filesystem manually.
impl Drop for VirtualMemoryFileSystem {
    fn drop(&mut self) {
        let id = self.id.clone();
        let conn_id = self.conn_id.clone();

        std::thread::spawn(move || {
            let mut state = state_lock_sync();
            if state.file_systems.contains_key(&id) {
                info!(
                    "closing virtual filesystem and removing reference from connection {}",
                    conn_id
                );

                if let Some(conn) = state.connection_mut(&conn_id) {
                    conn.refcount -= 1;
                }

                state.file_systems.remove(&id);
            }
        });
    }
}
