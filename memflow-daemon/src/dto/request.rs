use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Connect(Connect),
    ListConnections,
    CloseConnection(CloseConnection),

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
