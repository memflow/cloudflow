use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Log(Log),
    Table(Table),
    Result(CommandResult),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Log {
    pub level: i32, // log::Level
    pub msg: String,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub entries: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CommandResult {
    pub success: bool,
    pub msg: String,
}
