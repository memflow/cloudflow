use log::{error,info};
use url::Url;

use futures::prelude::*;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::runtime::Runtime;
use tokio::net::{TcpStream, UnixStream};
use tokio_serde::formats::*;
use tokio_serde::{formats::Json, SymmetricallyFramed};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use memflow_core::*;
use memflow_derive::connector;
use memflow_daemon::{request, response};

type FramedRequestWriter = SymmetricallyFramed<
    FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    request::Message,
    Json<request::Message, request::Message>,
>;
type FramedResponseReader = SymmetricallyFramed<
    FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    response::Message,
    Json<response::Message, response::Message>,
>;

pub struct DaemonConnector {
    addr: String,
    conn_id: String,
    runtime: Runtime,
    writer: FramedRequestWriter,
    reader: FramedResponseReader,
}

async fn connect_tcp(addr: &str) -> Result<(FramedRequestWriter, FramedResponseReader)> {
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

    Ok((serializer, deserializer))
}

impl DaemonConnector {
    pub fn new(addr: &str) -> Result<Self> {
        let mut rt = tokio::runtime::Runtime::new().map_err(|e| {
            error!("{}", e);
            Error::Other("unable to instantiate tokio runtime")
        })?;

        let url = Url::parse(addr).map_err(|_| Error::Other("invalid socket address"))?;
        let result = match url.scheme() {
            "tcp" => {
                if let Some(host_str) = url.host_str() {
                    println!("host_str: {}", host_str);
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
                // TODO:
                return Err(Error::Other("only tcp and unix urls are supported"));
            }
            _ => return Err(Error::Other("only tcp and unix urls are supported")),
        };

        Ok(Self {
            addr: String::new(),
            conn_id: "w10".to_string(),
            runtime: rt,
            writer: result.0,
            reader: result.1,
        })
    }
}

impl Clone for DaemonConnector {
    fn clone(&self) -> Self {
        DaemonConnector::new(&self.addr).unwrap()
    }
}

async fn phys_read_raw_list<'a>(
    writer: &mut FramedRequestWriter,
    reader: &mut FramedResponseReader,
    conn_id: &str,
    data: &mut [PhysicalReadData<'a>],
) -> Result<()> {
    // TODO: keep PhysicalAddress annotations

    for d in data.iter_mut() {
        // send request
        writer
            .send(request::Message::ReadPhysicalMemory(
                request::ReadPhysicalMemory {
                    conn_id: conn_id.to_string(),
                    addr: d.0.as_u64(),
                    len: d.1.len(),
                },
            ))
            .await
            .map_err(|e| {
                error!("{}", e);
                Error::IO("unable to send physical read request")
            })?;

        // wait for reply
        'inner: while let Some(msg) = reader.try_next().await.unwrap() {
            match msg {
                response::Message::BinaryData(msg) => {
                    d.1.clone_from_slice(msg.data.as_slice());
                    break 'inner;
                }
                response::Message::Result(msg) => {
                    // TODO: map to invalid read result
                    if !msg.success {
                        info!("failure received");
                        return Err(Error::Other("failure received"));
                    }
                }
                _ => {
                    info!("invalid message received");
                    return Err(Error::Other("invalid message received"));
                }
            }
        }
    }

    Ok(())
}

impl PhysicalMemory for DaemonConnector {
    fn phys_read_raw_list(&mut self, data: &mut [PhysicalReadData]) -> Result<()> {
        self.runtime.block_on(phys_read_raw_list(
            &mut self.writer,
            &mut self.reader,
            &self.conn_id,
            data,
        ))
    }

    fn phys_write_raw_list(&mut self, data: &[PhysicalWriteData]) -> Result<()> {
        Ok(())
    }

    // TODO:
    fn metadata(&self) -> PhysicalMemoryMetadata {
        PhysicalMemoryMetadata {
            size: 1337,
            readonly: false,
        }
    }
}

/// Creates a new Qemu Procfs Connector instance.
#[connector(name = "daemon")]
pub fn create_connector(args: &ConnectorArgs) -> Result<DaemonConnector> {
    /*
    if let Some(name) = args.get("name").or_else(|| args.get_default()) {
        QemuProcfs::with_guest_name(name)
    } else {
        QemuProcfs::new()
    }
    */
    DaemonConnector::new("tcp://127.0.0.1:8000")
}
