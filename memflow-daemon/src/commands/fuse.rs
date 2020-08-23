use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{KernelHandle, STATE};

use futures::Sink;
use std::marker::Unpin;

use memflow_core::*;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use std::ffi::OsStr;
use std::time::{Duration, UNIX_EPOCH};

const TTL: Duration = Duration::from_secs(1); // 1 second

const HELLO_TXT_CONTENT: &str = "Hello World!\n";

struct VirtualMemoryFileSystem {
    uid: u32,
    gid: u32,

    // TODO: ...
    dir_attr: FileAttr,
    file_attr: FileAttr,
}

impl VirtualMemoryFileSystem {
    pub fn new(uid: u32, gid: u32) -> Self {
        Self {
            uid,
            gid,
            dir_attr: FileAttr {
                ino: 1,
                size: 0,
                blocks: 0,
                atime: UNIX_EPOCH,
                mtime: UNIX_EPOCH,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o755, // TODO:
                nlink: 2,
                uid: uid,
                gid: gid,
                rdev: 0,
                flags: 0,
            },
            file_attr: FileAttr {
                ino: 2,
                size: 13,
                blocks: 1,
                atime: UNIX_EPOCH,
                mtime: UNIX_EPOCH,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: FileType::RegularFile,
                perm: 0o644, // TODO
                nlink: 1,
                uid: uid,
                gid: gid,
                rdev: 0,
                flags: 0,
            },
        }
    }
}

impl Filesystem for VirtualMemoryFileSystem {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        println!("lookup: parent={}, name={}", parent, name.to_string_lossy());

        if parent == 1 && name.to_str() == Some("hello.txt") {
            reply.entry(&TTL, &self.file_attr, 0);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("getattr: ino={}", ino);

        match ino {
            1 => reply.attr(&TTL, &self.dir_attr),
            2 => reply.attr(&TTL, &self.file_attr),
            _ => reply.error(ENOENT),
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
        if ino != 1 {
            reply.error(ENOENT);
            return;
        }

        println!("readdir: ino={}, fh={}, offset={}", ino, _fh, offset);

        let entries = vec![
            (1, FileType::Directory, "."),
            (1, FileType::Directory, ".."),
            (2, FileType::RegularFile, "hello.txt"),
        ];

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            // i + 1 means the index of the next entry
            reply.add(entry.0, (i + 1) as i64, entry.1, entry.2);
        }
        reply.ok();
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
            let vmfs = VirtualMemoryFileSystem::new(msg.uid, msg.gid);

            // TODO: use fuse::spawn_mount to have a convenient umoutn command in memflow?
            // blocks until the fs is umount-ed
            fuse::mount(vmfs, msg.mount_point, &mntopts).unwrap();

            // dec the refcount again after it was unmounted
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            let mut state = rt.block_on(STATE.lock());
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
