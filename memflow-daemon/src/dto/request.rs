use serde_derive::*;

use memflow::PhysicalAddress;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Connect(Connect),
    ListConnections,
    CloseConnection(CloseConnection),

    ReadPhysicalMemory(ReadPhysicalMemory),
    WritePhysicalMemory(WritePhysicalMemory),
    PhysicalMemoryMetadata(PhysicalMemoryMetadata),

    // TODO: make os specific
    FuseMount(FuseMount),
    FuseListMounts,

    GdbAttach(GdbAttach),
    GdbList,

    ListProcesses(ListProcesses),
    OpenProcess(OpenProcess),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Connect {
    pub name: String,
    pub args: Option<String>,
    pub alias: Option<String>,
    // TODO: os
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CloseConnection {
    pub conn_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReadPhysicalMemory {
    pub conn_id: String,
    pub addr: PhysicalAddress,
    pub len: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WritePhysicalMemory {
    pub conn_id: String,
    pub addr: PhysicalAddress,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PhysicalMemoryMetadata {
    pub conn_id: String,
}

// TODO: make os specific
#[derive(Serialize, Deserialize, Debug)]
pub struct FuseMount {
    pub conn_id: String,
    pub mount_point: String,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GdbAttach {
    pub conn_id: String,
    pub pid: String,
    pub addr: String,
    // TODO: fetch file permissions for unix sockets
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListProcesses {
    pub conn_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenProcess {
    pub proc_id: String,
}
