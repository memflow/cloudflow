use async_trait::async_trait;
use log::error;
use memflow_daemon::error::Result;

use memflow_daemon::memflow_rpc::memflow_client::MemflowClient;
use memflow_daemon::memflow_rpc::{
    CloseConnectionRequest, CloseConnectionResponse, ListConnectionsRequest,
    ListConnectionsResponse, ListProcessesRequest, ListProcessesResponse, NewConnectionRequest,
    NewConnectionResponse, PhysicalMemoryMetadataRequest, PhysicalMemoryMetadataResponse,
    ProcessInfoRequest, ProcessInfoResponse, ReadPhysicalMemoryEntryRequest,
    ReadPhysicalMemoryRequest, ReadPhysicalMemoryResponse, ReadVirtualMemoryEntryRequest,
    ReadVirtualMemoryRequest, ReadVirtualMemoryResponse, WritePhysicalMemoryRequest,
    WritePhysicalMemoryResponse, WriteVirtualMemoryRequest, WriteVirtualMemoryResponse,
};

pub type Client = MemflowClient<tonic::transport::Channel>;

pub struct Config {
    pub host: String,
}

pub fn create_client(conf: &Config) -> Client {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(create_client_async(conf))
}

pub async fn create_client_async(conf: &Config) -> Client {
    MemflowClient::connect(conf.host.clone())
        .await
        .expect("Could not create connection client")
}

/// Takes config and a request to send over the wire.
/// For a request the appropriate response is returned. E.g. for NewConnectionRequest -> NewConnectionResponse
pub fn dispatch_request<R, S>(conf: &Config, req: R) -> Result<S>
where
    tonic::Request<R>: DispatchMessage<tonic::Response<S>>,
{
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(dispatch_request_async(conf, req))
}

/// Takes config and a request to send over the wire.
/// For a request the appropriate response is returned. E.g. for NewConnectionRequest -> NewConnectionResponse
pub fn dispatch_request_client<R, S>(conf: &Config, req: R, client: &mut Client) -> Result<S>
where
    tonic::Request<R>: DispatchMessage<tonic::Response<S>>,
{
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(dispatch_request_async_client(conf, req, client))
}

/// Takes config and a request to send over the wire.
/// For a request the appropriate response is returned. E.g. for NewConnectionRequest -> NewConnectionResponse
pub async fn dispatch_request_async<R, S>(conf: &Config, req: R) -> Result<S>
where
    tonic::Request<R>: DispatchMessage<tonic::Response<S>>,
{
    let mut client = create_client_async(conf).await;
    dispatch_request_async_client(conf, req, &mut client).await
}

/// Takes config and a request to send over the wire via client.
/// For a request the appropriate response is returned. E.g. for NewConnectionRequest -> NewConnectionResponse
pub async fn dispatch_request_async_client<R, S>(
    conf: &Config,
    req: R,
    client: &mut Client,
) -> Result<S>
where
    tonic::Request<R>: DispatchMessage<tonic::Response<S>>,
{
    let request = tonic::Request::new(req);
    request
        .dispatch_message(conf, client)
        .await
        .map(|x| x.into_inner())
}

#[async_trait]
pub trait DispatchMessage<S> {
    async fn dispatch_message(self, conf: &Config, client: &mut Client) -> Result<S>
    where
        Self: Sized;
}

#[async_trait]
impl DispatchMessage<tonic::Response<NewConnectionResponse>>
    for tonic::Request<NewConnectionRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<NewConnectionResponse>> {
        client.new_connection(self).await.map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<ListConnectionsResponse>>
    for tonic::Request<ListConnectionsRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<ListConnectionsResponse>> {
        client.list_connections(self).await.map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<CloseConnectionResponse>>
    for tonic::Request<CloseConnectionRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<CloseConnectionResponse>> {
        client.close_connection(self).await.map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<ReadPhysicalMemoryResponse>>
    for tonic::Request<ReadPhysicalMemoryRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<ReadPhysicalMemoryResponse>> {
        client
            .read_physical_memory(self)
            .await
            .map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<WritePhysicalMemoryResponse>>
    for tonic::Request<WritePhysicalMemoryRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<WritePhysicalMemoryResponse>> {
        client
            .write_physical_memory(self)
            .await
            .map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<PhysicalMemoryMetadataResponse>>
    for tonic::Request<PhysicalMemoryMetadataRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<PhysicalMemoryMetadataResponse>> {
        client
            .physical_memory_metadata(self)
            .await
            .map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<ReadVirtualMemoryResponse>>
    for tonic::Request<ReadVirtualMemoryRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<ReadVirtualMemoryResponse>> {
        client.read_virtual_memory(self).await.map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<WriteVirtualMemoryResponse>>
    for tonic::Request<WriteVirtualMemoryRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<WriteVirtualMemoryResponse>> {
        client
            .write_virtual_memory(self)
            .await
            .map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<ListProcessesResponse>>
    for tonic::Request<ListProcessesRequest>
{
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<ListProcessesResponse>> {
        client.list_processes(self).await.map_err(|x| x.into())
    }
}

#[async_trait]
impl DispatchMessage<tonic::Response<ProcessInfoResponse>> for tonic::Request<ProcessInfoRequest> {
    async fn dispatch_message(
        self,
        _conf: &Config,
        client: &mut Client,
    ) -> Result<tonic::Response<ProcessInfoResponse>> {
        client.process_info(self).await.map_err(|x| x.into())
    }
}

pub fn benchmark(
    conf: &Config,
    physical_mode: bool,
    conn_id: &str,
    read_size: u64,
    async_mode: bool,
) {
    if async_mode {
        benchmark_async(conf, physical_mode, conn_id, read_size);
    } else {
        benchmark_sync(conf, physical_mode, conn_id, read_size);
    }
}

fn benchmark_sync(conf: &Config, physical_mode: bool, conn_id: &str, read_size: u64) {
    let pid = 0;
    let address = dispatch_request(
        conf,
        ProcessInfoRequest {
            conn_id: conn_id.to_string(),
            pid: pid,
        },
    )
    .expect("could not access process info")
    .process
    .unwrap()
    .address;

    let entry = ReadVirtualMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let req = ReadVirtualMemoryRequest {
        conn_id: conn_id.to_string(),
        pid: pid,
        base_offsets: false,
        reads: vec![entry],
    };
    let phys_entry = ReadPhysicalMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let phys_req = ReadPhysicalMemoryRequest {
        conn_id: conn_id.to_string(),
        reads: vec![phys_entry],
    };

    let mut client = create_client(conf);

    let start_time = std::time::Instant::now();
    let mut total_runs = 0;
    loop {
        total_runs += 1;

        let response = if !physical_mode {
            dispatch_request_client(conf, req.clone(), &mut client).map(|_| ())
        } else {
            dispatch_request_client(conf, phys_req.clone(), &mut client).map(|_| ())
        };
        match response {
            Err(e) => error!("{:#?}", e),
            Ok(_) => (),
        }

        if (std::time::Instant::now() - start_time).as_secs() > 10 {
            break;
        }
    }
    let end_time = std::time::Instant::now();

    let total_sec = (end_time - start_time).as_secs_f64();
    println!(
        "Total: {} s, Total: {}, Each: {} ms",
        total_sec,
        total_runs,
        total_sec * 1000.0 / total_runs as f64
    );
}

fn benchmark_async(conf: &Config, physical_mode: bool, conn_id: &str, read_size: u64) {
    let pid = 0;
    let address = dispatch_request(
        conf,
        ProcessInfoRequest {
            conn_id: conn_id.to_string(),
            pid: pid,
        },
    )
    .expect("could not access process info")
    .process
    .unwrap()
    .address;

    let entry = ReadVirtualMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let req = ReadVirtualMemoryRequest {
        conn_id: conn_id.to_string(),
        pid: pid,
        base_offsets: false,
        reads: vec![entry],
    };
    let phys_entry = ReadPhysicalMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let phys_req = ReadPhysicalMemoryRequest {
        conn_id: conn_id.to_string(),
        reads: vec![phys_entry],
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = create_client(conf);

    let start_time = std::time::Instant::now();
    let mut total_runs = 0;

    let bench = async {
        let mut responses = vec![];
        loop {
            total_runs += 1;

            let response = async {
                if !physical_mode {
                    let mut client_cp = client.clone();
                    dispatch_request_async_client(conf, req.clone(), &mut client_cp)
                        .await
                        .map(|_| ())
                } else {
                    let mut client_cp = client.clone();
                    dispatch_request_async_client(conf, phys_req.clone(), &mut client_cp)
                        .await
                        .map(|_| ())
                }
            };
            responses.push(response);

            if (std::time::Instant::now() - start_time).as_secs() > 10 || total_runs >= 20000 {
                break;
            }
        }
        let results = futures::future::join_all(responses).await;
        for res in results {
            match res {
                Err(e) => error!("{:#?}", e),
                Ok(_) => (),
            }
        }
    };
    rt.block_on(bench);
    let end_time = std::time::Instant::now();

    let total_sec = (end_time - start_time).as_secs_f64();
    println!(
        "Total: {} s, Total: {}, Each: {} ms",
        total_sec,
        total_runs,
        total_sec * 1000.0 / total_runs as f64
    );
}
