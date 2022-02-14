#[macro_use]
extern crate filer;

pub use cglue::slice::CSliceMut;
use connector::ThreadedConnectorArc;
use filer::prelude::v1::*;
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;
use os::ThreadedOsArc;
use std::sync::Arc;

pub mod connector;
pub mod os;
pub mod process;
pub mod util;

const BUILTIN_PLUGINS: &[extern "C" fn(&Node)] =
    &[os::on_node, process::on_node, connector::on_node];

pub fn create_node() -> CArcSome<Node> {
    let backend = NodeBackend::default();

    MemflowBackend::to_node(&backend);

    let node = Node::new(backend);

    for plugin in BUILTIN_PLUGINS {
        plugin(&node);
    }

    node.into()
}

pub struct MemflowBackend {
    connector: Arc<LocalBackend<ThreadedConnectorArc, Arc<Self>>>,
    os: Arc<LocalBackend<ThreadedOsArc, Arc<Self>>>,
    inventory: Inventory,
}

impl Default for MemflowBackend {
    fn default() -> Self {
        Self {
            connector: LocalBackend::default().with_new().into(),
            os: LocalBackend::default().with_new().into(),
            inventory: Inventory::scan(),
        }
    }
}

impl MemflowBackend {
    fn new_arc() -> CArcSome<Self> {
        let ret = Arc::from(Self::default());

        // SAFETY: we are not reading the underlying object from anywhere else.
        unsafe {
            unsafe fn ptr_mut<T>(ptr: *const T) -> *mut T {
                ptr as *mut T
            }

            (*ptr_mut(&*ret.connector)).set_context(ret.clone());
            (*ptr_mut(&*ret.os)).set_context(ret.clone());
        }

        ret.into()
    }

    fn add_to_node(&self, backend: &NodeBackend) {
        backend.add_backend("connector", self.connector.clone());
        backend.add_backend("os", self.os.clone());
    }

    pub fn to_node(backend: &NodeBackend) {
        Self::new_arc().add_to_node(backend)
    }
}
