mod filesystem;
use filesystem::VirtualMemoryFileSystem;

use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::STATE;

use futures::Sink;
use std::ffi::OsStr;
use std::marker::Unpin;
use std::path::Path;

pub async fn mount<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::FuseMount,
) -> Result<()> {
    let mut state = STATE.lock().await;

    let is_empty = Path::new(&msg.mount_point)
        .read_dir()
        .map_err(|_| Error::Other("mount point not found"))?
        .next()
        .is_none();
    if is_empty {
        // find connection and spawn filesystem thread
        if let Some(conn) = state.connection_mut(&msg.conn_id) {
            let kernel = conn.kernel.clone();
            std::thread::spawn(move || {
                let opts = [
                    "-o",
                    &format!("auto_unmount,allow_other,uid={},gid={}", msg.uid, msg.gid),
                ];
                let mntopts = opts.iter().map(|o| o.as_ref()).collect::<Vec<&OsStr>>();

                // the filesystem will add itself into the global scope
                let vmfs = VirtualMemoryFileSystem::new(
                    &msg.conn_id,
                    &msg.mount_point,
                    kernel,
                    msg.uid,
                    msg.gid,
                );

                // blocks until the fs is umounted
                fuse_mt::mount(fuse_mt::FuseMT::new(vmfs, 8), &msg.mount_point, &mntopts).unwrap();
            });

            send_ok(frame).await
        } else {
            send_err(
                frame,
                &format!("no connection with id {} found", msg.conn_id),
            )
            .await
        }
    } else {
        send_err(
            frame,
            &format!("mount point {} is not empty", msg.mount_point),
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
