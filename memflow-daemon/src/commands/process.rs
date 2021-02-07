use log::info;

use crate::error::{Error, Result};

use crate::state::KernelHandle;
use crate::state::STATE;

use crate::memflow_rpc::{
    ListProcessesRequest, ListProcessesResponse, ProcessInfoRequest, ProcessInfoResponse,
    Win32ModuleInfo, Win32ProcessInfo,
};

fn conv_win32_module(module: &memflow_win32::win32::Win32ModuleInfo) -> Win32ModuleInfo {
    Win32ModuleInfo {
        peb_entry: module.peb_entry.as_u64(),
        parent_eprocess: module.parent_eprocess.as_u64(),

        base: module.base.as_u64(),
        size: module.size as u64,
        path: module.path.clone(),
        name: module.name.clone(),
    }
}

fn conv_win32_process(proc_info: &memflow_win32::win32::Win32ProcessInfo) -> Win32ProcessInfo {
    Win32ProcessInfo {
        address: proc_info.address.as_u64(),

        pid: proc_info.pid,
        name: proc_info.name.clone(),
        dtb: proc_info.dtb.as_u64(),
        section_base: proc_info.section_base.as_u64(),
        exit_status: proc_info.exit_status,
        ethread: proc_info.ethread.as_u64(),
        wow64: proc_info.wow64.as_u64(),

        teb: proc_info.teb.unwrap_or_default().as_u64(),
        teb_wow64: proc_info.teb_wow64.unwrap_or_default().as_u64(),

        peb_native: proc_info.peb_native.as_u64(),
        peb_wow64: proc_info.peb_wow64.unwrap_or_default().as_u64(),

        proc_pointer_bits: proc_info.proc_arch.bits() as u32,
        arch_pointer_bits: proc_info.sys_arch.bits() as u32,
        is_wow64: !proc_info.wow64.is_null(),
    }
}

pub async fn process_info(msg: &ProcessInfoRequest) -> Result<ProcessInfoResponse> {
    let mut state = STATE.lock().await;

    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut proc = kernel.process_pid(msg.pid)?;
                let module_list = proc.module_list()?;

                let modules = module_list
                    .into_iter()
                    .map(|x| conv_win32_module(&x))
                    .collect();

                let response = ProcessInfoResponse {
                    process: Some(conv_win32_process(&proc.proc_info)),
                    modules: modules,
                };
                Ok(response)
            }
        }
    } else {
        Err(Error::Connector(format!(
            "no connection with id {} found",
            msg.conn_id
        )))
    }
}

pub async fn ls(msg: &ListProcessesRequest) -> Result<ListProcessesResponse> {
    let mut state = STATE.lock().await;

    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                if let Ok(processes) = kernel.process_info_list() {
                    info!(
                        "listing processes for connection {}: {} processes\n",
                        msg.conn_id,
                        processes.len(),
                    );

                    let response = ListProcessesResponse {
                        processes: processes
                            .into_iter()
                            .map(|x| conv_win32_process(&x))
                            .collect(),
                    };
                    Ok(response)
                } else {
                    Err(Error::Connector(format!(
                        "could not get processes on connection {}",
                        msg.conn_id
                    )))
                }
            }
        }
    } else {
        Err(Error::Connector(format!(
            "no connection with id {} found",
            msg.conn_id
        )))
    }
}

/*
pub async fn open<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::Connect,
) -> Result<()> {
    let mut state = STATE.lock().await;

    if let Some(conn) = state.connections.get_mut(&msg.id) {
        match &mut conn.kernel {
            Kernel::Win32(kernel) => {

                kernel.process_info(name)

            }
        }
    } else {
        send_log_error(frame, &format!("no connection with id {} found", msg.id)).await?;
    }

    send_eof(frame).await




    match create_connector(&msg) {
        Ok(conn) => {
            // TODO: add os argument
            // TODO: redirect log to client
            // TODO: add cache options

            send_log_info(frame, "connector created").await?;

            // initialize kernel
            let kernel = memflow_win32::Kernel::builder(conn)
                .build_default_caches()
                .build()
                .map_err(|_| Error::Connector("unable to find kernel"))?;

            send_log_info(frame, "found win32 kernel").await?;

            let mut state = STATE.lock().await;

            let uuid = new_uuid();

            let opened_connection =
                OpenedConnection::new(&uuid, &msg.name, msg.args.clone(), Kernel::Win32(kernel));

            state.connections.insert(uuid.clone(), opened_connection);

            send_log_info(
                frame,
                &format!(
                    "connection created: {} | {} | {:?}",
                    uuid, msg.name, msg.args
                ),
            )
            .await?;
        }
        Err(err) => {
            send_log_error(
                frame,
                &format!(
                    "could not create connector: {} | {:?} ({})",
                    msg.name, msg.args, err
                ),
            )
            .await?;
        }
    };

    send_eof(frame).await
}
*/
