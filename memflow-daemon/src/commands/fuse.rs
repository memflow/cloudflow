use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{state_lock_sync, KernelHandle, STATE};

use bitfield::bitfield;
use futures::Sink;
use std::collections::HashMap;
use std::marker::Unpin;
use std::time::{Duration, Instant, UNIX_EPOCH};

use log::{info, trace};

use memflow_core::*;
use memflow_win32::*;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use std::ffi::OsStr;

const TTL: Duration = Duration::from_secs(1); // 1 second

const HELLO_TXT_CONTENT: &str = "Hello World!\n";

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

struct VirtualMemoryFileSystem {
    conn_id: String,

    uid: u32,
    gid: u32,

    last_refresh: Instant,
    file_system: HashMap<u64, VirtualEntry>,
}

impl VirtualMemoryFileSystem {
    pub fn new(conn_id: String, uid: u32, gid: u32) -> Self {
        Self {
            conn_id,

            uid,
            gid,

            last_refresh: Instant::now(),
            file_system: HashMap::new(),
        }
    }

    fn update_file_system(&mut self) {
        if self.last_refresh.elapsed() <= Duration::from_secs(1) {
            return;
        }

        self.file_system.clear();

        let mut root = VirtualFolder::new(0, &self.conn_id);

        let mut state = state_lock_sync();
        if let Some(conn) = state.connection_mut(&self.conn_id) {
            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    if let Ok(process_info) = kernel.process_info_list() {
                        for pi in process_info.iter() {
                            root.children.push(self.create_process_folder(pi));
                        }
                    }
                }
            }
        }

        self.file_system
            .insert(root.inode, VirtualEntry::Folder(root));

        self.last_refresh = Instant::now();
    }

    fn create_process_folder(&mut self, pi: &Win32ProcessInfo) -> u64 {
        let mut inode = INode(0);
        inode.set_pid(pi.pid as u64);

        let mut prc = VirtualFolder::new(
            inode.0,
            &format!("{}_{}", pi.pid, pi.name.replace(".", "_")),
        );

        // add virtual folder inside of process?
        inode.set_mid(inode.mid() + 1);
        let modules = VirtualFolder::new(inode.0, "modules");
        self.file_system
            .insert(inode.0, VirtualEntry::Folder(modules));
        prc.children.push(inode.0);

        /*
            inode.set_mid(inode.mid() + 1);
            prc.entries
                .push(VirtualEntry::Folder(VirtualFolder::new(INode(inode.0), "modules")));
        */

        let prc_inode = prc.inode;
        self.file_system
            .insert(prc_inode, VirtualEntry::Folder(prc));
        prc_inode
    }
}

impl Filesystem for VirtualMemoryFileSystem {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        println!("---------------");
        println!("lookup: parent={}, name={}", parent, name.to_string_lossy());
        println!("---------------");

        // TODO: incremental updates
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

        // root directory
        /*
        if parent == 1 {
        } else {
            reply.error(ENOENT);
        }
        */

        /*
        if parent == 1 && name.to_str() == Some("hello.txt") {
            reply.entry(&TTL, &self.file_attr, 0);
        } else {
            reply.error(ENOENT);
        }
        */
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("---------------");
        println!("getattr: ino={}", ino);
        println!("---------------");

        // TODO: incremental updates
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

        /*
        match ino {
            1 => reply.attr(&TTL, &self.dir_attr),
            2 => reply.attr(&TTL, &self.file_attr),
            _ => reply.error(ENOENT),
        }
        */
        //reply.error(ENOENT);
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

        if ino == 2 {
            reply.data(&HELLO_TXT_CONTENT.as_bytes()[offset as usize..]);
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
        println!("---------------");
        println!("readdir: ino={}, fh={}, offset={}", ino, _fh, offset);
        println!("---------------");

        // TODO: incremental updates
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

pub async fn mount<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::FuseMount,
) -> Result<()> {
    let mut state = STATE.lock().await;

    // TODO:
    // - mount point should be optional -> also check if dir exists and create the dir
    // - if the dir was created just rm it here again (if its empty + umounted)
    // fallback for the mountpath should be PWD + "./alias or id"

    // check if connection is valid and increase ref count
    if let Some(conn) = state.connection_mut(&msg.id) {
        conn.refcount += 1;

        println!("uid={} gid={}", msg.uid, msg.gid);

        std::thread::spawn(move || {
            let opts = [
                "-o",
                "ro",
                "-o",
                &format!("fsname=hello,allow_other,uid={},gid={}", msg.uid, msg.gid),
            ];
            let mntopts = opts.iter().map(|o| o.as_ref()).collect::<Vec<&OsStr>>();

            // TODO: chmod?
            let vmfs = VirtualMemoryFileSystem::new(msg.id.clone(), msg.uid, msg.gid);

            // TODO: use fuse::spawn_mount to have a convenient umoutn command in memflow?
            // blocks until the fs is umount-ed
            fuse::mount(vmfs, msg.mount_point, &mntopts).unwrap();

            // dec the refcount again after it was unmounted
            let mut state = state_lock_sync();
            if let Some(conn) = state.connection_mut(&msg.id) {
                conn.refcount -= 1;
            }
        });

        // TODO: add a message explaining that the user has to manually umount the fs

        send_ok(frame).await
    } else {
        send_log_error(frame, &format!("no connection with id {} found", msg.id)).await
    }
}
