use memflow_client;

use log::error;

use memflow::{
    ConnectorArgs, Error, PhysicalMemory, PhysicalMemoryMetadata, PhysicalReadData,
    PhysicalWriteData, Result,
};
use memflow_derive::connector;
use tokio::runtime::Runtime;

pub struct DaemonConnector {
    addr: String,
    conn_id: String,

    runtime: Runtime,
    client: memflow_client::dispatch::Client,
    conf: memflow_client::dispatch::Config,

    metadata: memflow_daemon::memflow_rpc::PhysicalMemoryMetadata,
}

impl DaemonConnector {
    pub fn new(addr: &str, conn_id: &str) -> Result<Self> {
        let conf = memflow_client::dispatch::Config { host: addr.into() };

        let rt = Runtime::new().map_err(|e| {
            error!("{}", e);
            Error::Other("unable to instantiate tokio runtime")
        })?;

        let mut client: memflow_client::dispatch::Client =
            rt.block_on(memflow_client::dispatch::create_client_async(&conf));

        let metadata = rt
            .block_on(memflow_client::dispatch::dispatch_request_async_client(
                &conf,
                memflow_daemon::memflow_rpc::PhysicalMemoryMetadataRequest {
                    conn_id: conn_id.into(),
                },
                &mut client,
            ))
            .expect("Failed to get memory metadata")
            .metadata
            .expect("Received no metadata");

        Ok(Self {
            addr: addr.to_string(),
            conn_id: conn_id.to_string(),

            runtime: rt,
            client,
            conf,

            metadata,
        })
    }
}

impl Clone for DaemonConnector {
    fn clone(&self) -> Self {
        DaemonConnector::new(&self.addr, &self.conn_id).unwrap()
    }
}

impl PhysicalMemory for DaemonConnector {
    fn phys_read_raw_list(&mut self, data: &mut [PhysicalReadData]) -> Result<()> {
        let mut reads = vec![];
        for read in data.iter() {
            reads.push(
                memflow_daemon::memflow_rpc::ReadPhysicalMemoryEntryRequest {
                    addr: read.0.as_u64(),
                    len: read.1.len() as u64,
                },
            );
        }
        let request = memflow_daemon::memflow_rpc::ReadPhysicalMemoryRequest {
            conn_id: self.conn_id.clone(),
            reads: reads,
        };
        let response = self
            .runtime
            .block_on(memflow_client::dispatch::dispatch_request_async_client(
                &self.conf,
                request,
                &mut self.client,
            ))
            .map_err(|_| Error::Other("Transfer error"))?;

        for (data_out, read_in) in data.iter_mut().zip(response.reads.iter()) {
            data_out.1.copy_from_slice(&read_in.data[..]);
        }
        Ok(())
    }

    fn phys_write_raw_list(&mut self, data: &[PhysicalWriteData]) -> Result<()> {
        let mut writes = vec![];
        for write in data {
            writes.push(
                memflow_daemon::memflow_rpc::WritePhysicalMemoryEntryRequest {
                    addr: write.0.as_u64(),
                    data: write.1.into(),
                },
            );
        }
        let request = memflow_daemon::memflow_rpc::WritePhysicalMemoryRequest {
            conn_id: self.conn_id.clone(),
            writes: writes,
        };

        self.runtime
            .block_on(memflow_client::dispatch::dispatch_request_async_client(
                &self.conf,
                request,
                &mut self.client,
            ))
            .map_err(|_| Error::Other("Transfer error"))?;

        Ok(())
    }

    fn metadata(&self) -> PhysicalMemoryMetadata {
        PhysicalMemoryMetadata {
            size: self.metadata.size as usize,
            readonly: self.metadata.readonly,
        }
    }
}

/// Creates a new Qemu Procfs Connector instance.
#[connector(name = "daemon")]
pub fn create_connector(args: &ConnectorArgs) -> Result<DaemonConnector> {
    let addr = args
        .get("host")
        .or_else(|| args.get_default())
        .ok_or_else(|| Error::Connector("host argument is missing"))?;
    let conn_id = args
        .get("id")
        .ok_or_else(|| Error::Connector("id argument is missing"))?;
    DaemonConnector::new(addr, conn_id)
}
