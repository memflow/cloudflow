use log::{error, info};
use url::Url;

use futures::prelude::*;
use tokio::net::{tcp, unix, TcpStream, UnixStream};
use tokio::runtime::Runtime;
use tokio_serde::formats::*;
use tokio_serde::{formats::Json, SymmetricallyFramed};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use memflow::*;
use memflow_daemon::{request, response};
use memflow_derive::connector;

// framed udp read/write pairs
type FramedUdsRequestWriter = SymmetricallyFramed<
    FramedWrite<unix::OwnedWriteHalf, LengthDelimitedCodec>,
    request::Message,
    Json<request::Message, request::Message>,
>;
type FramedUdsResponseReader = SymmetricallyFramed<
    FramedRead<unix::OwnedReadHalf, LengthDelimitedCodec>,
    response::Message,
    Json<response::Message, response::Message>,
>;

// framed tcp read/write pairs
type FramedTcpRequestWriter = SymmetricallyFramed<
    FramedWrite<tcp::OwnedWriteHalf, LengthDelimitedCodec>,
    request::Message,
    Json<request::Message, request::Message>,
>;
type FramedTcpResponseReader = SymmetricallyFramed<
    FramedRead<tcp::OwnedReadHalf, LengthDelimitedCodec>,
    response::Message,
    Json<response::Message, response::Message>,
>;

/// A read/write framed pair for a stream.
enum FramedStream {
    Uds((FramedUdsRequestWriter, FramedUdsResponseReader)),
    Tcp((FramedTcpRequestWriter, FramedTcpResponseReader)),
}

impl FramedStream {
    pub async fn send(&mut self, item: request::Message) -> Result<()> {
        match self {
            FramedStream::Uds((writer, _)) => writer.send(item).await.map_err(|e| {
                error!("{}", e);
                Error::IO("unable to send message")
            }),
            FramedStream::Tcp((writer, _)) => writer.send(item).await.map_err(|e| {
                error!("{}", e);
                Error::IO("unable to send message")
            }),
        }
    }

    pub async fn try_next(&mut self) -> Result<response::Message> {
        match self {
            FramedStream::Uds((_, reader)) => reader
                .try_next()
                .await
                .map_err(|e| {
                    error!("{}", e);
                    Error::IO("unable to read read message")
                })?
                .ok_or_else(|| Error::IO("no more messages")),
            FramedStream::Tcp((_, reader)) => reader
                .try_next()
                .await
                .map_err(|e| {
                    error!("{}", e);
                    Error::IO("unable to read read message")
                })?
                .ok_or_else(|| Error::IO("no more messages")),
        }
    }
}

pub struct DaemonConnector {
    addr: String,
    conn_id: String,

    runtime: Runtime,
    stream: FramedStream,

    metadata: PhysicalMemoryMetadata,
}

async fn connect_tcp(addr: &str) -> Result<FramedStream> {
    info!("trying to open connection to {}", addr);
    let socket = TcpStream::connect(addr)
        .await
        .map_err(|_| Error::Other("unable to connect to tcp socket"))?;
    let (reader, writer) = socket.into_split();

    let framed_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());
    let serializer = tokio_serde::SymmetricallyFramed::new(
        framed_writer,
        SymmetricalJson::<request::Message>::default(),
    );

    let framed_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
    let deserializer = tokio_serde::SymmetricallyFramed::new(
        framed_reader,
        SymmetricalJson::<response::Message>::default(),
    );

    Ok(FramedStream::Tcp((serializer, deserializer)))
}

async fn connect_uds(addr: &str) -> Result<FramedStream> {
    println!("trying to open connection to {}", addr);
    let socket = UnixStream::connect(addr)
        .await
        .map_err(|_| Error::Other("unable to connect to udp socket"))?;
    let (reader, writer) = socket.into_split();

    let framed_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());
    let serializer = tokio_serde::SymmetricallyFramed::new(
        framed_writer,
        SymmetricalJson::<request::Message>::default(),
    );

    let framed_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
    let deserializer = tokio_serde::SymmetricallyFramed::new(
        framed_reader,
        SymmetricalJson::<response::Message>::default(),
    );

    Ok(FramedStream::Uds((serializer, deserializer)))
}

impl DaemonConnector {
    pub fn new(addr: &str, conn_id: &str) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            error!("{}", e);
            Error::Other("unable to instantiate tokio runtime")
        })?;

        let url = Url::parse(addr).map_err(|_| Error::Other("invalid socket address"))?;
        let mut stream = match url.scheme() {
            "tcp" => {
                if let Some(host_str) = url.host_str() {
                    rt.block_on(connect_tcp(&format!(
                        "{}:{}",
                        host_str,
                        url.port().unwrap_or(8000)
                    )))?
                } else {
                    return Err(Error::Other("invalid tcp host address"));
                }
            }
            "unix" => {
                if let Ok(path) = url.to_file_path() {
                    rt.block_on(connect_uds(&format!("{}", path.to_string_lossy())))?
                } else {
                    return Err(Error::Other("invalid unix domain socket path"));
                }
            }
            _ => return Err(Error::Other("only unix and tcp urls are supported")),
        };

        // read metadata
        let metadata = rt
            .block_on(phys_metadata(&mut stream, conn_id))
            .map_err(|_| Error::Other("unable to get phys_mem metadata from daemon"))?;

        Ok(Self {
            addr: String::new(),
            conn_id: conn_id.to_string(),

            runtime: rt,
            stream,

            metadata,
        })
    }
}

impl Clone for DaemonConnector {
    fn clone(&self) -> Self {
        DaemonConnector::new(&self.addr, &self.conn_id).unwrap()
    }
}

async fn phys_read_raw_list(
    stream: &mut FramedStream,
    conn_id: &str,
    data: &mut [PhysicalReadData<'_>],
) -> Result<()> {
    let mut reads = Vec::new();
    for d in data.iter() {
        reads.push(request::ReadPhysicalMemoryEntry {
            addr: d.0,
            len: d.1.len(),
        });
    }

    // send request
    stream
        .send(request::Message::ReadPhysicalMemory(
            request::ReadPhysicalMemory {
                conn_id: conn_id.to_string(),
                reads,
            },
        ))
        .await
        .map_err(|e| {
            error!("{}", e);
            Error::IO("unable to send physical read request")
        })?;

    // wait for reply
    if let Ok(msg) = stream.try_next().await {
        match msg {
            response::Message::PhysicalMemoryRead(msg) => {
                //d.1.clone_from_slice(msg.data.as_slice());
                for read in msg.reads.iter().zip(data.iter_mut()) {
                    (read.1).1.clone_from_slice(read.0.data.as_slice());
                }
            }
            response::Message::Result(msg) => {
                if !msg.success {
                    // TODO: continue batch on error
                    info!("failure received: {}", msg.msg);
                    return Err(Error::Other("failure received"));
                }
            }
            _ => {
                info!("invalid message received");
                return Err(Error::Other("invalid message received"));
            }
        }
    }

    Ok(())
}

async fn phys_write_raw_list(
    stream: &mut FramedStream,
    conn_id: &str,
    data: &[PhysicalWriteData<'_>],
) -> Result<()> {
    let mut writes = Vec::new();
    for d in data.iter() {
        writes.push(request::WritePhysicalMemoryEntry {
            addr: d.0,
            data: d.1.to_vec(),
        });
    }

    // send request
    stream
        .send(request::Message::WritePhysicalMemory(
            request::WritePhysicalMemory {
                conn_id: conn_id.to_string(),
                writes,
            },
        ))
        .await
        .map_err(|e| {
            error!("{}", e);
            Error::IO("unable to send physical write request")
        })?;

    // wait for reply
    if let Ok(msg) = stream.try_next().await {
        match msg {
            response::Message::Result(msg) => {
                if !msg.success {
                    // TODO: continue batch on error
                    info!("failure received: {}", msg.msg);
                    return Err(Error::Other("failure received"));
                }
            }
            _ => {
                info!("invalid message received");
                return Err(Error::Other("invalid message received"));
            }
        }
    }

    Ok(())
}

async fn phys_metadata(stream: &mut FramedStream, conn_id: &str) -> Result<PhysicalMemoryMetadata> {
    // send request
    stream
        .send(request::Message::PhysicalMemoryMetadata(
            request::PhysicalMemoryMetadata {
                conn_id: conn_id.to_string(),
            },
        ))
        .await
        .map_err(|e| {
            error!("{}", e);
            Error::IO("unable to send metadata request")
        })?;

    // wait for reply
    if let Ok(msg) = stream.try_next().await {
        match msg {
            response::Message::PhysicalMemoryMetadata(msg) => {
                return Ok(msg.metadata);
            }
            response::Message::Result(msg) => {
                if !msg.success {
                    info!("failure received: {}", msg.msg);
                    return Err(Error::Other("failure received"));
                }
            }
            _ => {
                info!("invalid message received");
                return Err(Error::Other("invalid message received"));
            }
        }
    }

    Err(Error::Other("unable to receive metadata"))
}

impl PhysicalMemory for DaemonConnector {
    fn phys_read_raw_list(&mut self, data: &mut [PhysicalReadData]) -> Result<()> {
        self.runtime
            .block_on(phys_read_raw_list(&mut self.stream, &self.conn_id, data))
    }

    fn phys_write_raw_list(&mut self, data: &[PhysicalWriteData]) -> Result<()> {
        self.runtime
            .block_on(phys_write_raw_list(&mut self.stream, &self.conn_id, data))
    }

    fn metadata(&self) -> PhysicalMemoryMetadata {
        self.metadata
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
