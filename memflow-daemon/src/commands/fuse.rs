mod filesystem;
use filesystem::VirtualMemoryFileSystem;

use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::{new_uuid, state_lock_sync, FileSystemHandle, STATE};

use futures::Sink;
use std::marker::Unpin;

use std::ffi::OsStr;

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
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        conn.refcount += 1;

        // spawn a thread to move this out of the async runtime
        std::thread::spawn(move || {
            println!("uid={} gid={}", msg.uid, msg.gid);

            let opts = [
                "-o",
                "ro",
                "-o",
                &format!("fsname=hello,allow_other,uid={},gid={}", msg.uid, msg.gid),
            ];
            let mntopts = opts.iter().map(|o| o.as_ref()).collect::<Vec<&OsStr>>();

            // TODO: chmod?
            let fuse_id = new_uuid();
            let vmfs = VirtualMemoryFileSystem::new(&fuse_id, &msg.conn_id, msg.uid, msg.gid);

            // TODO: use fuse::spawn_mount to have a convenient umoutn command in memflow?
            // blocks until the fs is umount-ed
            let file_system =
                unsafe { fuse::spawn_mount(vmfs, msg.mount_point.clone(), &mntopts) }.unwrap();

            // grab state and add the new file_system
            let mut state = state_lock_sync();
            state.file_systems.insert(
                fuse_id.clone(),
                FileSystemHandle::new(&fuse_id, &msg.conn_id, &msg.mount_point, file_system),
            );
        });

        send_ok(frame).await
    } else {
        send_log_error(
            frame,
            &format!("no connection with id {} found", msg.conn_id),
        )
        .await
    }
}

pub async fn ls<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    let state = STATE.lock().await;

    send_log_info(
        frame,
        &format!(
            "listing mounted file systems: {} file systems",
            state.file_systems.len()
        ),
    )
    .await?;

    if !state.file_systems.is_empty() {
        let mut table = response::Table::default();
        table.headers = vec![
            "id".to_string(),
            "connection".to_string(),
            "mount point".to_string(),
        ];

        for c in state.file_systems.iter() {
            let entry = vec![c.1.id.clone(), c.1.conn_id.clone(), c.1.mount_point.clone()];
            table.entries.push(entry);
        }

        send_table(frame, table).await?;
    }

    send_ok(frame).await
}

pub async fn umount<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::FuseUmount,
) -> Result<()> {
    let mut state = STATE.lock().await;

    if state.file_systems.contains_key(&msg.fuse_id) {
        let conn_id = state
            .file_systems
            .get(&msg.fuse_id)
            .unwrap()
            .conn_id
            .clone();
        if let Some(conn) = state.connection_mut(&conn_id) {
            conn.refcount -= 1;
        }
        state.file_systems.remove(&msg.fuse_id);
        send_ok(frame).await
    } else {
        send_err(
            frame,
            &format!(
                "unable to remove file system {}: file system not found",
                msg.fuse_id
            ),
        )
        .await
    }
}
