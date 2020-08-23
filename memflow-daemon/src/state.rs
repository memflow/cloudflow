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
    pub connection_aliases: HashMap<String, String>,

    pub processes: HashMap<String, OpenedProcess>,
}

impl State {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            connection_aliases: HashMap::new(),

            processes: HashMap::new(),
        }
    }

    pub fn connection_add(
        &mut self,
        name: &str,
        args: Option<String>,
        alias: Option<String>,
        kernel: KernelHandle,
    ) -> Result<String> {
        if alias.is_some()
            && self
                .connection_aliases
                .contains_key(alias.as_ref().unwrap())
        {
            return Err(Error::Connector(
                "a connection with this alias already exists",
            ));
        }

        let id = new_uuid();
        let conn = OpenedConnection::new(&id, alias.clone(), name, args, kernel);

        self.connections.insert(id.clone(), conn);
        if let Some(a) = alias {
            self.connection_aliases.insert(a, id.clone());
        }

        Ok(id)
    }

    pub fn connection(&self, id: &str) -> Option<&OpenedConnection> {
        // first try to get by id
        if self.connections.contains_key(id) {
            self.connections.get(id)
        } else if let Some(real_id) = self.connection_aliases.get(id) {
            self.connections.get(real_id)
        } else {
            None
        }
    }

    pub fn connection_mut(&mut self, id: &str) -> Option<&mut OpenedConnection> {
        // first try to get by id
        if self.connections.contains_key(id) {
            self.connections.get_mut(id)
        } else if let Some(real_id) = self.connection_aliases.get(id) {
            self.connections.get_mut(real_id)
        } else {
            None
        }
    }

    pub fn connection_remove(&mut self, id: &str) -> Result<()> {
        let (id, alias) = if let Some(conn) = self.connection(id) {
            if conn.refcount == 0 {
                (conn.id.clone(), conn.alias.clone())
            } else {
                return Err(Error::Connector("connection still has open references"));
            }
        } else {
            return Err(Error::Connector("connection not found"));
        };

        if let Some(alias) = &alias {
            self.connection_aliases.remove(alias);
        }
        self.connections.remove(&id);

        Ok(())
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
