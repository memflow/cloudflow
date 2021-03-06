use crate::error::{Error, Result};
use crate::state::{KernelHandle, STATE};

use log::{error, info};
use memflow::{ConnectorArgs, ConnectorInstance, ConnectorInventory};

use crate::memflow_rpc::{
    CloseConnectionRequest, CloseConnectionResponse, ConnectionDescription, ListConnectionsRequest,
    ListConnectionsResponse, NewConnectionRequest, NewConnectionResponse,
};

fn create_connector(msg: &NewConnectionRequest) -> Result<ConnectorInstance> {
    let args = match &msg.args {
        args if args == "" => ConnectorArgs::default(),
        args => ConnectorArgs::parse(&args)
            .map_err(|_| Error::Connector("unable to parse connector string".into()))?,
    };

    let inventory = unsafe { ConnectorInventory::scan() };
    unsafe { inventory.create_connector(&msg.name, &args) }.map_err(Error::from)
}

pub async fn new<'a>(msg: &NewConnectionRequest) -> Result<NewConnectionResponse> {
    match create_connector(msg) {
        Ok(conn) => {
            // TODO: add os argument
            // TODO: redirect log to client
            // TODO: add cache options

            info!("connector created");

            // initialize kernel
            let kernel = memflow_win32::Kernel::builder(conn)
                .build_default_caches()
                .build()?;

            info!("found win32 kernel");

            let mut state = STATE.lock().await;

            match state.connection_add(
                &msg.name,
                if msg.args == "" {
                    None
                } else {
                    Some(msg.args.clone())
                },
                if msg.alias == "" {
                    None
                } else {
                    Some(msg.alias.clone())
                },
                KernelHandle::Win32(kernel),
            ) {
                Ok(id) => {
                    info!("connection created: {} | {} | {:?}", id, msg.name, msg.args);
                    Ok(NewConnectionResponse { conn_id: id })
                }
                Err(err) => {
                    let err_msg = format!(
                        "could not create connector: {} | {:?} ({})",
                        msg.name, msg.args, err
                    );
                    error!("{}", err_msg);
                    Err(Error::Connector(err_msg))
                }
            }
        }
        Err(err) => {
            let err_msg = format!(
                "could not create connector: {} | {:?} ({})",
                msg.name, msg.args, err
            );
            error!("{}", err_msg);
            Err(Error::Connector(err_msg))
        }
    }
}

pub async fn ls(_msg: &ListConnectionsRequest) -> Result<ListConnectionsResponse> {
    let state = STATE.lock().await;

    info!(
        "listing open connections: {} connections",
        state.connections.len()
    );

    let mut connections = vec![];

    if !state.connections.is_empty() {
        for c in state.connections.iter() {
            let con = ConnectionDescription {
                conn_id: c.1.id.clone(),
                name: c.1.name.clone(),
                args: c.1.args.as_ref().map(|a| a.to_string()).unwrap_or_default(),
                alias: c
                    .1
                    .alias
                    .as_ref()
                    .map(|a| a.to_string())
                    .unwrap_or_default(),
                refcount: c.1.refcount as u64,
            };
            connections.push(con);
        }
    }

    Ok(ListConnectionsResponse {
        connections: connections,
    })
}

pub async fn rm(msg: &CloseConnectionRequest) -> Result<CloseConnectionResponse> {
    let mut state = STATE.lock().await;

    match state.connection_remove(&msg.conn_id) {
        Ok(_) => {
            info!("connection {} removed", msg.conn_id);
            Ok(CloseConnectionResponse {})
        }
        Err(err) => {
            let err_msg = format!("unable to remove connection {}: {}", msg.conn_id, err);
            error!("{}", err_msg);
            Err(Error::Connector(err_msg))
        }
    }
}
