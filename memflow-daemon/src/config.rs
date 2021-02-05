use serde_derive::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub verbosity: Option<String>,
    pub pid_file: Option<String>,
    pub log_file: Option<String>,
    pub socket_addr: String,
}

// set max to 1 GiB frames
pub const MAX_FRAME_LENGTH: usize = 1 * 1024 * 1024 * 1024;
