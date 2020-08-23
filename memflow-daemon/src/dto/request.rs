use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Connect(Connect),
    ListConnections,
    CloseConnection(CloseConnection),

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
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListProcesses {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenProcess {
    pub id: String,
}
