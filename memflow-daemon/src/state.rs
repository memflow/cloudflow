use std::collections::HashMap;
use tokio::sync::Mutex;

use lazy_static::lazy_static;
use uuid::Uuid;

use memflow_core::*;

lazy_static! {
    pub static ref STATE: Mutex<State> = Mutex::new(State::new());
}

pub fn new_uuid() -> String {
    let uuid = Uuid::new_v4();
    uuid.to_simple()
        .encode_lower(&mut Uuid::encode_buffer())
        .chars()
        .take(10)
        .collect::<String>()
}

/// Contains the entire global state of the daemon.
pub struct State {
    pub connections: HashMap<String, OpenedConnection>,
    pub processes: HashMap<String, OpenedProcess>,
}

impl State {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            processes: HashMap::new(),
        }
    }
}

pub type CachedWin32Kernel = memflow_win32::Kernel<
    CachedMemoryAccess<'static, ConnectorInstance, TimedCacheValidator>,
    CachedVirtualTranslate<DirectTranslate, TimedCacheValidator>,
>;

pub enum Kernel {
    Win32(CachedWin32Kernel),
}

pub struct OpenedConnection {
    pub id: String,
    pub name: String,
    pub args: Option<String>,
    pub kernel: Kernel,
}

impl OpenedConnection {
    pub fn new(id: &str, name: &str, args: Option<String>, kernel: Kernel) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            args: args.map(|a| a.to_string()),
            kernel,
        }
    }
}

pub struct OpenedProcess {
    pub conn_id: String,
}

impl OpenedProcess {
    pub fn new(conn_id: &str) -> Self {
        Self {
            conn_id: conn_id.to_string(),
        }
    }
}
