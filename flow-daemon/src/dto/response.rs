use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Log(Log),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Log {
    pub level: i32, // log::Level
    pub msg: String,
}
