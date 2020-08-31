use serde_derive::*;

use memflow_core::Address;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Connect(Connect),
    ListConnections,
    CloseConnection(CloseConnection),

    ReadPhysicalMemory(ReadPhysicalMemory),
    WritePhysicalMemory(WritePhysicalMemory),

    // TODO: make os specific
    FuseMount(FuseMount),
    FuseListMounts,

    GdbAttach(GDBAttach),
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
    pub addr: u64, // TODO: use Address here
    pub len: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WritePhysicalMemory {
    pub conn_id: String,
    pub address: u64,
    pub data: Vec<u8>, // TODO: encode as base64?
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
pub struct GDBAttach {
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
