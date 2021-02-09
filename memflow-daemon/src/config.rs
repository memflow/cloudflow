use serde_derive::Deserialize;

#[cfg(not(target_os = "windows"))]
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub verbosity: Option<String>,
    pub pid_file: Option<String>,
    pub log_file: Option<String>,
    pub socket_addr: String,
}
