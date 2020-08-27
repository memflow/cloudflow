mod connection_memory;
mod module_dump;
mod module_pe_header;
mod process_info;
mod process_memory;
mod static_ds;

use crate::error::Result;
use crate::state::{state_lock_sync, CachedWin32Process, KernelHandle};

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

/// Default file TTL
const TTL: Duration = Duration::from_secs(1);

bitfield! {
    /// Describes an INode in the VMFS
    /// This bitfield struct is used to ensure that processes with the same PID end up with the same inodes.
    pub struct INode(u64);
    impl Debug;
    pub pid, set_pid: 31, 0;
    pub mid, set_mid: 47, 32;
    pub id, set_id: 63, 48;
}

impl INode {
    pub fn new(pid: u64, mid: u64, id: u64) -> Self {
        let mut inode = INode(0);
        inode.set_pid(pid);
        inode.set_mid(mid);
        inode.set_id(id);
        inode
    }
}

/// Entries of the vmfs can either be Files or Folders.
/// This enum is used when building a tree structure for the vmfs.
pub enum VirtualEntry {
    Folder(VirtualFolder),
    File(VirtualFile),
}

impl VirtualEntry {
    pub fn name(&self) -> &str {
        match self {
            VirtualEntry::Folder(folder) => &folder.name,
            VirtualEntry::File(file) => &file.name,
        }
    }
}

/// A folder in the vmfs.
pub struct VirtualFolder {
    pub name: String,
    pub children: Vec<u64>,
}

impl VirtualFolder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            children: Vec::new(),
        }
    }
}

/// A file in the vmfs.
pub struct VirtualFile {
    pub name: String,
    pub data_source: Box<dyn VirtualFileDataSource>,
}

/// A trait for providing data for a VirtualFile
pub trait VirtualFileDataSource {
    fn content_length(&mut self) -> Result<u64>;
    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>>;
}

/// Trait describing a module of the vmfs.
/// This is used to extend functionality of the vmfs and add files/folders in a modular way.

pub trait VMFSConnectionExt {
    fn entry(
        &self,
        conn_id: &str,
        add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry>;
}

pub trait VMFSProcessExt {
    fn entry(
        &self,
        conn_id: &str,
        process: &mut CachedWin32Process,
        add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry>;
}

pub trait VMFSModuleExt {
    fn entry(
        &self,
        conn_id: &str,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
        add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry>;
}

/// The Virtual Memory File System
/// ...
pub struct VirtualMemoryFileSystem {
    id: String,
    conn_id: String,

    uid: u32,
    gid: u32,

    last_refresh: Instant,
    file_system: HashMap<u64, VirtualEntry>,

    ext_connections: Vec<Box<dyn VMFSConnectionExt>>,
    ext_processes: Vec<Box<dyn VMFSProcessExt>>,
    ext_modules: Vec<Box<dyn VMFSModuleExt>>,
}

unsafe impl Send for VirtualMemoryFileSystem {}

impl VirtualMemoryFileSystem {
    pub fn new(id: &str, conn_id: &str, uid: u32, gid: u32) -> Self {
        let mut fs = Self {
            id: id.to_string(),
            conn_id: conn_id.to_string(),

            uid,
            gid,

            last_refresh: Instant::now(),
            file_system: HashMap::new(),

            ext_connections: vec![Box::new(connection_memory::VMFSConnectionMemory)],
            ext_processes: vec![
                Box::new(process_info::VMFSProcessInfo),
                Box::new(process_memory::VMFSProcessMemory),
            ],
            ext_modules: vec![
                Box::new(module_pe_header::VMFSModulePEHeader),
                Box::new(module_dump::VMFSModuleMemory),
            ],
        };

        // initialize file_system
        fs.file_system = fs.create_root_folder();

        fs
    }

    fn update_file_system(&mut self) {
        if self.last_refresh.elapsed() > Duration::from_secs(10) {
            self.file_system = self.create_root_folder();
            self.last_refresh = Instant::now();
        }
    }

    fn create_root_folder(&self) -> HashMap<u64, VirtualEntry> {
        // TODO: incremental updates for changed pids
        let mut file_system = HashMap::new();

        let root_inode = INode::new(0, 0, 0);
        let mut root_folder = VirtualFolder::new(&self.conn_id);

        // add all vmfs modules for the connection scope (inode 0-0-x)
        let mut ext_inode = INode::new(0, 0, 0);
        for ext in self.ext_connections.iter() {
            if let Ok(fse) = ext.entry(&self.conn_id, &mut |fse| {
                ext_inode.set_id(ext_inode.id() + 1);
                file_system.insert(ext_inode.0, fse);
                ext_inode.0
            }) {
                ext_inode.set_id(ext_inode.id() + 1);
                file_system.insert(ext_inode.0, fse);
                root_folder.children.push(ext_inode.0);
            }
        }

        // add 'process' folder (inode 0-1-0)
        let process_inode = INode::new(0, 1, 0);
        let mut process_folder = VirtualFolder::new("process");

        // inode for process entries is: pid-x-x
        let mut state = state_lock_sync();
        if let Some(conn) = state.connection_mut(&self.conn_id) {
            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    if let Ok(process_info) = kernel.process_info_list() {
                        for pi in process_info.iter() {
                            let proc_inode = INode::new(1 + pi.pid as u64, 0, 0);
                            let process = Win32Process::with_kernel_ref(kernel, pi.clone());
                            process_folder.children.push(self.create_process_folder(
                                INode(proc_inode.0),
                                process,
                                &mut file_system,
                            ));
                        }
                    }
                }
            }
        }

        // insert process node
        root_folder.children.push(process_inode.0);
        file_system.insert(process_inode.0, VirtualEntry::Folder(process_folder));

        // add 'driver' folder (inode 1-0-0)
        let driver_inode = INode::new(1, 0, 0);
        let mut driver_folder = VirtualFolder::new("driver");

        if let Some(conn) = state.connection_mut(&self.conn_id) {
            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    if let Ok(pi) = kernel.kernel_process_info() {
                        let mut process = Win32Process::with_kernel_ref(kernel, pi);
                        if let Ok(module_info) = process.module_info_list() {
                            let mut mod_inode = INode(driver_inode.0);
                            for mi in module_info.iter() {
                                mod_inode.set_mid(mod_inode.mid() + 1);
                                driver_folder.children.push(self.create_module_folder(
                                    INode(mod_inode.0),
                                    &mut process,
                                    mi,
                                    &mut file_system,
                                ));
                            }
                        }
                    }
                }
            }
        }

        // insert driver node
        root_folder.children.push(driver_inode.0);
        file_system.insert(driver_inode.0, VirtualEntry::Folder(driver_folder));

        // insert root node
        file_system.insert(root_inode.0, VirtualEntry::Folder(root_folder));
        file_system
    }

    fn create_process_folder(
        &self,
        mut inode: INode,
        mut process: CachedWin32Process,
        file_system: &mut HashMap<u64, VirtualEntry>,
    ) -> u64 {
        let proc_inode = INode(inode.0);
        let mut proc_folder = VirtualFolder::new(&format!(
            "{}_{}",
            process.proc_info.pid,
            process.proc_info.name.replace(".", "_")
        ));

        // add all vmfs modules for the process scope
        // inode for process module entries is: pid-0-x
        inode.set_mid(0);
        for ext in self.ext_processes.iter() {
            // instantiate entry with a new id
            if let Ok(fse) = ext.entry(&self.conn_id, &mut process, &mut |fse| {
                inode.set_id(inode.id() + 1);
                file_system.insert(inode.0, fse);
                inode.0
            }) {
                inode.set_id(inode.id() + 1);
                file_system.insert(inode.0, fse);
                proc_folder.children.push(inode.0);
            }
        }

        // add all modules for this process
        // inode for module entries is: pid-x-y
        if let Ok(modules) = process.module_info_list() {
            for mi in modules.iter() {
                inode.set_mid(inode.mid() + 1); // increase mid for each module
                proc_folder.children.push(self.create_module_folder(
                    INode(inode.0),
                    &mut process,
                    mi,
                    file_system,
                ));
            }
        }

        file_system.insert(proc_inode.0, VirtualEntry::Folder(proc_folder));
        proc_inode.0
    }

    fn create_module_folder(
        &self,
        mut inode: INode,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
        file_system: &mut HashMap<u64, VirtualEntry>,
    ) -> u64 {
        inode.set_id(0);

        let module_inode = INode(inode.0);
        let mut module_folder = VirtualFolder::new(&format!(
            "{}_{}",
            mod_info.base,
            mod_info.name.replace(".", "_")
        ));

        // add all vmfs modules for the module scope
        for ext in self.ext_modules.iter() {
            // instantiate entry with a new id
            if let Ok(fse) = ext.entry(&self.conn_id, process, &mod_info, &mut |fse| {
                inode.set_id(inode.id() + 1);
                file_system.insert(inode.0, fse);
                inode.0
            }) {
                inode.set_id(inode.id() + 1);
                file_system.insert(inode.0, fse);
                module_folder.children.push(inode.0);
            }
        }

        file_system.insert(module_inode.0, VirtualEntry::Folder(module_folder));
        module_inode.0
    }
}

impl Filesystem for VirtualMemoryFileSystem {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        self.update_file_system();

        if let Some(entry) = self.file_system.get(&(parent - 1)) {
            info!(
                "lookup(): found file system entry: {} {}",
                parent - 1,
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(folder) => {
                    // match child entries by name
                    // TODO: maybe add a map?
                    for child_inode in folder.children.clone().iter() {
                        if let Some(child_entry) = self.file_system.get_mut(&child_inode) {
                            // TODO: improve this check
                            if child_entry.name() == name.to_string_lossy() {
                                trace!(
                                    "lookup(): found child entry: {} {}",
                                    child_inode,
                                    child_entry.name()
                                );
                                match child_entry {
                                    VirtualEntry::Folder(_child_folder) => {
                                        reply.entry(
                                            &TTL,
                                            &FileAttr {
                                                ino: 1 + child_inode,
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
                                                ino: 1 + child_inode,
                                                size: child_file
                                                    .data_source
                                                    .content_length()
                                                    .unwrap_or_default(),
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

        if let Some(entry) = self.file_system.get_mut(&(ino - 1)) {
            info!(
                "getattr(): found file system entry: {} {}",
                ino - 1,
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(_folder) => {
                    reply.attr(
                        &TTL,
                        &FileAttr {
                            ino,
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
                            ino,
                            size: file.data_source.content_length().unwrap_or_default(),
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
        size: u32,
        reply: ReplyData,
    ) {
        if let Some(entry) = self.file_system.get_mut(&(ino - 1)) {
            info!(
                "getattr(): found file system entry: {} {}",
                ino - 1,
                entry.name()
            );

            match entry {
                VirtualEntry::File(file) => {
                    if let Ok(contents) = file.data_source.contents(offset, size) {
                        reply.data(contents.as_slice());
                    } else {
                        reply.data(&[]);
                    }
                }
                VirtualEntry::Folder(_folder) => {
                    // should never happen
                    reply.error(ENOENT);
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
                ino - 1,
                entry.name()
            );

            match entry {
                VirtualEntry::Folder(folder) => {
                    let mut entries = vec![
                        (1, FileType::Directory, ".".to_string()),
                        (1, FileType::Directory, "..".to_string()),
                    ];

                    // find each child entry and add them to the list
                    for child_inode in folder.children.iter() {
                        if let Some(child_entry) = self.file_system.get(&child_inode) {
                            match child_entry {
                                VirtualEntry::Folder(child_folder) => {
                                    trace!(
                                        "readdir(): adding child folder: {} {}",
                                        child_inode,
                                        child_folder.name
                                    );
                                    entries.push((
                                        1 + child_inode,
                                        FileType::Directory,
                                        child_folder.name.clone(),
                                    ));
                                }
                                VirtualEntry::File(child_file) => {
                                    trace!(
                                        "readdir(): adding child file: {} {}",
                                        child_inode,
                                        child_file.name
                                    );
                                    entries.push((
                                        1 + child_inode,
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
