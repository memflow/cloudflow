use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Connect(Connect),
    ListConnections(ListConnections),
    CloseConnection(CloseConnection),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Connect {
    pub name: String,
    pub args: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListConnections {}

#[derive(Serialize, Deserialize, Debug)]
pub struct CloseConnection {
    pub id: String,
}
