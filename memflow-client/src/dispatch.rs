use async_trait::async_trait;
use memflow_daemon::error::Result;

use memflow_daemon::memflow_rpc::memflow_client::MemflowClient;
use memflow_daemon::memflow_rpc::{
    CloseConnectionRequest, CloseConnectionResponse, ListConnectionsRequest,
    ListConnectionsResponse, ListProcessesRequest, ListProcessesResponse, NewConnectionRequest,
    NewConnectionResponse, PhysicalMemoryMetadataRequest, PhysicalMemoryMetadataResponse,
    ProcessInfoRequest, ProcessInfoResponse, ReadPhysicalMemoryRequest, ReadPhysicalMemoryResponse,
    ReadVirtualMemoryRequest, ReadVirtualMemoryResponse, WritePhysicalMemoryRequest,
    WritePhysicalMemoryResponse, WriteVirtualMemoryRequest, WriteVirtualMemoryResponse,
};
use tokio::runtime::Runtime;

pub type Client = MemflowClient<tonic::transport::Channel>;

pub struct Config {
    pub host: String,
}

/// This returns a Client and a Runtime. The client can only be used within the provided runtime
pub fn create_client(conf: &Config) -> (Client, Runtime) {
    let rt = Runtime::new().unwrap();
    (rt.block_on(create_client_async(conf)), rt)
}

/// The client can only be used with the same runtime it has been created with
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
    let rt = Runtime::new().unwrap();
    rt.block_on(dispatch_request_async(conf, req))
}

/// Takes config and a request to send over the wire.
/// For a request the appropriate response is returned. E.g. for NewConnectionRequest -> NewConnectionResponse
pub fn dispatch_request_client<R, S>(
    conf: &Config,
    req: R,
    client: &mut Client,
    rt: &Runtime,
) -> Result<S>
where
    tonic::Request<R>: DispatchMessage<tonic::Response<S>>,
{
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
