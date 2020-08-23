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

    pub fn connection_add(
        &mut self,
        name: &str,
        args: Option<String>,
        alias: Option<String>,
        kernel: KernelHandle,
    ) -> String {
        let id = new_uuid();
        let conn = OpenedConnection::new(&id, alias, name, args, kernel);
        self.connections.insert(id.clone(), conn);
        id
    }

    pub fn connection(&self, id: &str) -> Option<&OpenedConnection> {
        // first try to get by id
        if self.connections.contains_key(id) {
            self.connections.get(id)
        } else {
            // try using the alias
            for c in self.connections.iter() {
                if let Some(alias) = &c.1.alias {
                    if alias == id {
                        return Some(c.1);
                    }
                }
            }
            None
        }
    }

    pub fn connection_mut(&mut self, id: &str) -> Option<&mut OpenedConnection> {
        // first try to get by id
        if self.connections.contains_key(id) {
            self.connections.get_mut(id)
        } else {
            // try using the alias
            for c in self.connections.iter_mut() {
                if let Some(alias) = &c.1.alias {
                    if alias == id {
                        return Some(c.1);
                    }
                }
            }
            None
        }
    }
}

pub type CachedWin32Kernel = memflow_win32::Kernel<
    CachedMemoryAccess<'static, ConnectorInstance, TimedCacheValidator>,
    CachedVirtualTranslate<DirectTranslate, TimedCacheValidator>,
>;

pub enum KernelHandle {
    Win32(CachedWin32Kernel),
}

pub struct OpenedConnection {
    pub id: String,
    pub alias: Option<String>,
    pub refcount: usize,
    pub name: String,
    pub args: Option<String>,
    pub kernel: KernelHandle,
}

impl OpenedConnection {
    pub fn new(
        id: &str,
        alias: Option<String>,
        name: &str,
        args: Option<String>,
        kernel: KernelHandle,
    ) -> Self {
        Self {
            id: id.to_string(),
            alias,
            refcount: 0,
            name: name.to_string(),
            args,
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
