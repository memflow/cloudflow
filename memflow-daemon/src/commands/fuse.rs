mod filesystem;
use filesystem::VirtualMemoryFileSystem;
use log::info;

use crate::error::{Error, Result};
use crate::state::{new_uuid, STATE};

use crate::memflow_rpc::{
    FuseListRequest, FuseListResponse, FuseMount, FuseMountRequest, FuseMountResponse,
};

use std::ffi::OsStr;
use std::path::Path;

pub async fn mount(msg: &FuseMountRequest) -> Result<FuseMountResponse> {
    let mut state = STATE.lock().await;

    let is_empty = Path::new(&msg.mount_point)
        .read_dir()
        .map_err(|_| Error::Other("mount point not found".to_string()))?
        .next()
        .is_none();
    if is_empty {
        // find connection and spawn filesystem thread
        if let Some(conn) = state.connection_mut(&msg.conn_id) {
            let kernel = conn.kernel.clone();
            let id = new_uuid();

            info!("filesystem with id {} mounted at {}", id, &msg.mount_point);
            info!("please use 'umount' or 'fusermount -u' to unmount the filesystem");

            let msg_clone = msg.clone();
            std::thread::spawn(move || {
                let opts = [
                    "-o",
                    &format!(
                        "auto_unmount,allow_other,uid={},gid={}",
                        msg_clone.uid, msg_clone.gid
                    ),
                ];
                let mntopts = opts.iter().map(|o| o.as_ref()).collect::<Vec<&OsStr>>();

                // the filesystem will add itself into the global scope
                let vmfs = VirtualMemoryFileSystem::new(
                    &id,
                    &msg_clone.conn_id,
                    &msg_clone.mount_point,
                    kernel,
                    msg_clone.uid,
                    msg_clone.gid,
                );

                // blocks until the fs is umounted
                fuse_mt::mount(
                    fuse_mt::FuseMT::new(vmfs, 8),
                    &msg_clone.mount_point,
                    &mntopts,
                )
                .unwrap();
            });

            Ok(FuseMountResponse {})
        } else {
            Err(Error::Connector(format!(
                "no connection with id {} found",
                msg.conn_id
            )))
        }
    } else {
        Err(Error::Other(format!(
            "mount point {} is not empty",
            msg.mount_point
        )))
    }
}

pub async fn ls(_msg: &FuseListRequest) -> Result<FuseListResponse> {
    let state = STATE.lock().await;

    info!(
        "listing mounted file systems: {} file systems",
        state.file_systems.len()
    );

    let mut file_systems = vec![];

    for file_system in state.file_systems.iter() {
        let fuse_mount = FuseMount {
            id: file_system.1.id.clone(),
            conn_id: file_system.1.conn_id.clone(),
            mount_point: file_system.1.mount_point.clone(),
        };
        file_systems.push(fuse_mount);
    }

    Ok(FuseListResponse {
        mounts: file_systems,
    })
}
