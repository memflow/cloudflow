mod stub;

use crate::error::{Error, Result};
use crate::state::{new_uuid, STATE};
use log::info;

use crate::memflow_rpc::{
    GdbAttachRequest, GdbAttachResponse, GdbListRequest, GdbListResponse, GdbStub,
};

pub async fn attach(msg: &GdbAttachRequest) -> Result<GdbAttachResponse> {
    let mut state = STATE.lock().await;

    // find connection and spawn gdb thread
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        let kernel = conn.kernel.clone();
        let id = new_uuid();

        info!("gdb stub with id {} spawned at address {}", id, &msg.addr);
        info!("the gdb stub will automatically be closed on disconnect");

        let msg_clone = msg.clone();
        let id_clone = id.clone();
        std::thread::spawn(move || {
            stub::spawn_gdb_stub(
                &id_clone,
                &msg_clone.conn_id,
                msg_clone.pid,
                &msg_clone.addr,
                kernel,
            )
            .unwrap();
        });

        Ok(GdbAttachResponse { id: id })
    } else {
        Err(Error::Connector(format!(
            "no connection with id {} found",
            msg.conn_id
        )))
    }
}

pub async fn ls(_msg: &GdbListRequest) -> Result<GdbListResponse> {
    let state = STATE.lock().await;

    info!("listing open gdb stubs: {} stubs", state.gdb_stubs.len());

    let mut gdb_stubs = vec![];

    for gdb_stub in state.gdb_stubs.iter() {
        let stub = GdbStub {
            id: gdb_stub.1.id.clone(),
            connection: gdb_stub.1.conn_id.clone(),
            addr: gdb_stub.1.addr.clone(),
        };
        gdb_stubs.push(stub);
    }

    Ok(GdbListResponse { stubs: gdb_stubs })
}
