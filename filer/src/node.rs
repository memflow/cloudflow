use crate::backend::*;
use crate::error::*;
use crate::fs::*;
use crate::plugin_store::*;
use crate::types::*;
use abi_stable::StableAbi;
use cglue::prelude::v1::*;

pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;

use dashmap::DashMap;
use sharded_slab::Slab;

#[derive(StableAbi)]
#[repr(C)]
pub enum HandleMap {
    Forward(usize, usize),
    Object(FileOpsObj<c_void>),
}

impl ListEntry {
    pub fn new(name: ReprCString, is_branch: bool) -> Self {
        Self { name, is_branch }
    }
}

#[derive(Default)]
pub struct Node {
    pub backend: NodeBackend,
    pub frontends: Vec<FrontendArcBox<'static>>,
    pub plugins: CPluginStore,
}

impl Frontend for Node {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, data: CIterator<RWData>) -> Result<()> {
        self.backend.read(handle, data)
    }
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<ROData>) -> Result<()> {
        self.backend.write(handle, data)
    }
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        self.backend.rpc(handle, input, output)
    }
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str) -> Result<usize> {
        self.backend.open(path, &self.plugins)
    }
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()> {
        self.backend.list(path, &self.plugins, out)
    }
}

#[derive(Default)]
pub struct NodeBackend {
    backends: Slab<BackendBox<'static>>,
    /// Maps backend name to backend ID in the vec
    backend_map: DashMap<String, usize>,
    /// Maps handle to (backend, handle) pair.
    handles: Slab<HandleMap>,
}

impl NodeBackend {
    pub fn add_backend(&self, name: &str, backend: impl Backend + 'static) {
        let name = name.to_string();

        self.backend_map.entry(name.to_string()).or_insert_with(|| {
            self.backends
                .insert(trait_obj!(backend as Backend))
                .expect("Slab is full!")
        });
    }
}

impl Backend for NodeBackend {
    fn read(&self, handle: usize, data: CIterator<RWData>) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.read(handle, data)
                } else {
                    Err(Error(ErrorOrigin::Node, ErrorKind::InvalidPath))
                }
            }
            Some(HandleMap::Object(obj)) => obj.read(data),
            _ => Err(Error(ErrorOrigin::Node, ErrorKind::NotFound)),
        }
    }

    fn write(&self, handle: usize, data: CIterator<ROData>) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.write(handle, data)
                } else {
                    Err(Error(ErrorOrigin::Node, ErrorKind::InvalidPath))
                }
            }
            Some(HandleMap::Object(obj)) => obj.write(data),
            _ => Err(Error(ErrorOrigin::Node, ErrorKind::NotFound)),
        }
    }

    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.rpc(handle, input, output)
                } else {
                    Err(Error(ErrorOrigin::Node, ErrorKind::InvalidPath))
                }
            }
            Some(HandleMap::Object(obj)) => obj.rpc(input, output),
            _ => Err(Error(ErrorOrigin::Node, ErrorKind::NotFound)),
        }
    }

    fn open(&self, path: &str, plugins: &CPluginStore) -> Result<usize> {
        if let Some((backend, path)) = path.split_once("/") {
            if let Some((bid, backend)) = self
                .backend_map
                .get(backend)
                .and_then(|idx| self.backends.get(*idx).map(|b| (*idx, b)))
            {
                let ret = backend.open(path, plugins)?;
                self.handles.insert(HandleMap::Forward(bid, ret));
                return Ok(ret);
            }
        }

        Err(Error(ErrorOrigin::Node, ErrorKind::NotFound))
    }

    fn list(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()> {
        let path = path.trim_start_matches('/');
        let (backend, path) = path.split_once("/").unwrap_or((path, ""));

        if !backend.is_empty() {
            if let Some(backend) = self
                .backend_map
                .get(backend)
                .and_then(|idx| self.backends.get(*idx))
            {
                return backend.list(path, plugins, out);
            }
        } else {
            self.backend_map
                .iter()
                .map(|r| r.key().clone())
                .map(|k| ListEntry::new(k.into(), true))
                .feed_into_mut(out);
            return Ok(());
        }

        Err(Error(ErrorOrigin::Node, ErrorKind::NotFound))
    }
}

#[cglue_trait]
#[int_result]
pub trait Frontend {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, data: CIterator<RWData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<ROData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()>;
}
