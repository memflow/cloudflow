use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    /// Message that contains a log message
    Log(Log),
    /// Message that contains a table
    Table(Table),
    /// Message that contains the data read from physical memory
    PhysicalMemoryRead(PhysicalMemoryRead),
    /// Message that contains physical memory metadata
    PhysicalMemoryMetadata(PhysicalMemoryMetadata),
    /// Message that contains a result
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
pub struct PhysicalMemoryReadEntry {
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PhysicalMemoryRead {
    pub reads: Vec<PhysicalMemoryReadEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PhysicalMemoryMetadata {
    pub metadata: memflow::PhysicalMemoryMetadata,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CommandResult {
    pub success: bool,
    pub msg: String,
}
