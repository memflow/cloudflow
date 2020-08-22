use std::collections::HashMap;
use std::sync::Mutex;

use lazy_static::lazy_static;
use uuid::Uuid;

use memflow_core::ConnectorInstance;

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
    // loaded connectors
    pub connectors: HashMap<String, ConnectorState>,
    // opened process list

    // etc
}

impl State {
    pub fn new() -> Self {
        Self {
            connectors: HashMap::new(),
        }
    }
}

pub struct ConnectorState {
    pub id: String,
    pub name: String,
    pub args: Option<String>,
    pub instance: ConnectorInstance,
}

impl ConnectorState {
    pub fn new(id: &str, name: &str, args: Option<String>, instance: ConnectorInstance) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            args: args.map(|a| a.to_string()),
            instance,
        }
    }
}
