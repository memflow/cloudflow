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

#[repr(C)]
#[derive(StableAbi)]
pub struct Node {
    pub backend: BackendBox<'static>,
    pub plugins: CPluginStore,
}

impl Node {
    pub fn new<T: Backend + Into<BackendBaseBox<'static, T>> + 'static>(backend: T) -> Self {
        let backend = trait_obj!(backend as Backend);
        Self {
            backend,
            plugins: Default::default(),
        }
    }
}

impl Frontend for CArcSome<Node> {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, data: CIterator<RWData>) -> Result<()> {
        self.backend.read(self.into(), handle, data)
    }
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<ROData>) -> Result<()> {
        self.backend.write(self.into(), handle, data)
    }
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        self.backend.rpc(self.into(), handle, input, output)
    }
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str) -> Result<usize> {
        self.backend.open(self.into(), path, &self.plugins)
    }
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()> {
        self.backend.list(self.into(), path, &self.plugins, out)
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

impl<T: Backend> Backend for std::sync::Arc<T> {
    fn read(&self, stack: BackendStack, handle: usize, data: CIterator<RWData>) -> Result<()> {
        (**self).read(stack, handle, data)
    }

    fn write(&self, stack: BackendStack, handle: usize, data: CIterator<ROData>) -> Result<()> {
        (**self).write(stack, handle, data)
    }

    fn rpc(
        &self,
        stack: BackendStack,
        handle: usize,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<()> {
        (**self).rpc(stack, handle, input, output)
    }

    fn open(&self, stack: BackendStack, path: &str, plugins: &CPluginStore) -> Result<usize> {
        (**self).open(stack, path, plugins)
    }

    fn list(
        &self,
        stack: BackendStack,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()> {
        (**self).list(stack, path, plugins, out)
    }
}

impl<T: Backend> Backend for CArcSome<T> {
    fn read(&self, stack: BackendStack, handle: usize, data: CIterator<RWData>) -> Result<()> {
        (**self).read(stack, handle, data)
    }

    fn write(&self, stack: BackendStack, handle: usize, data: CIterator<ROData>) -> Result<()> {
        (**self).write(stack, handle, data)
    }

    fn rpc(
        &self,
        stack: BackendStack,
        handle: usize,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<()> {
        (**self).rpc(stack, handle, input, output)
    }

    fn open(&self, stack: BackendStack, path: &str, plugins: &CPluginStore) -> Result<usize> {
        (**self).open(stack, path, plugins)
    }

    fn list(
        &self,
        stack: BackendStack,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()> {
        (**self).list(stack, path, plugins, out)
    }
}

impl Backend for NodeBackend {
    fn read(&self, stack: BackendStack, handle: usize, data: CIterator<RWData>) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.read((&stack, self).into(), handle, data)
                } else {
                    Err(Error(ErrorOrigin::Node, ErrorKind::InvalidPath))
                }
            }
            Some(HandleMap::Object(obj)) => obj.read(data),
            _ => Err(Error(ErrorOrigin::Node, ErrorKind::NotFound)),
        }
    }

    fn write(&self, stack: BackendStack, handle: usize, data: CIterator<ROData>) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.write((&stack, self).into(), handle, data)
                } else {
                    Err(Error(ErrorOrigin::Node, ErrorKind::InvalidPath))
                }
            }
            Some(HandleMap::Object(obj)) => obj.write(data),
            _ => Err(Error(ErrorOrigin::Node, ErrorKind::NotFound)),
        }
    }

    fn rpc(
        &self,
        stack: BackendStack,
        handle: usize,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.rpc((&stack, self).into(), handle, input, output)
                } else {
                    Err(Error(ErrorOrigin::Node, ErrorKind::InvalidPath))
                }
            }
            Some(HandleMap::Object(obj)) => obj.rpc(input, output),
            _ => Err(Error(ErrorOrigin::Node, ErrorKind::NotFound)),
        }
    }

    fn open(&self, stack: BackendStack, path: &str, plugins: &CPluginStore) -> Result<usize> {
        if let Some((backend, path)) = path.split_once("/") {
            if let Some((bid, backend)) = self
                .backend_map
                .get(backend)
                .and_then(|idx| self.backends.get(*idx).map(|b| (*idx, b)))
            {
                let ret = backend.open((&stack, self).into(), path, plugins)?;
                let ret = self.handles.insert(HandleMap::Forward(bid, ret)).unwrap();
                return Ok(ret);
            }
        }

        Err(Error(ErrorOrigin::Node, ErrorKind::NotFound))
    }

    fn list(
        &self,
        stack: BackendStack,
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
                return backend.list((&stack, self).into(), path, plugins, out);
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

    #[skip_func]
    fn open_handle<'a>(&'a self, path: &str) -> Result<ObjHandle<'a, Self>>
    where
        Self: Sized,
    {
        Ok((self, self.open(path)?).into())
    }

    #[skip_func]
    fn open_cursor<'a>(&'a self, path: &str) -> Result<ObjCursor<'a, Self>>
    where
        Self: Sized,
    {
        Ok((self, self.open(path)?).into())
    }
}

pub struct ObjHandle<'a, T>(&'a T, usize);

impl<'a, T> From<(&'a T, usize)> for ObjHandle<'a, T> {
    fn from((a, b): (&'a T, usize)) -> Self {
        Self(a, b)
    }
}

impl<'a, T: Frontend> ObjHandle<'a, T> {
    /// Perform read operation on the given handle
    pub fn read(&self, data: CIterator<RWData>) -> Result<()> {
        self.0.read(self.1, data)
    }
    /// Perform write operation on the given handle.
    pub fn write(&self, data: CIterator<ROData>) -> Result<()> {
        self.0.write(self.1, data)
    }
    /// Perform remote procedure call on the given handle.
    pub fn rpc(&self, input: &[u8], output: &mut [u8]) -> Result<()> {
        self.0.rpc(self.1, input, output)
    }
}

pub struct ObjCursor<'a, T>(ObjHandle<'a, T>, Size);

impl<'a, T> From<(&'a T, usize)> for ObjCursor<'a, T> {
    fn from((a, b): (&'a T, usize)) -> Self {
        Self(ObjHandle(a, b), 0)
    }
}

impl<'a, T> From<(&'a T, usize, Size)> for ObjCursor<'a, T> {
    fn from((a, b, c): (&'a T, usize, Size)) -> Self {
        Self(ObjHandle(a, b), c)
    }
}

impl<'a, T: Frontend> std::io::Read for ObjCursor<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .read((&mut core::iter::once(CTup2(self.1, buf.into()))).into())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.as_str()))?;
        self.1 += buf.len() as Size;
        Ok(buf.len())
    }
}

impl<'a, T: Frontend> std::io::Write for ObjCursor<'a, T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .write((&mut core::iter::once(CTup2(self.1, buf.into()))).into())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.as_str()))?;
        self.1 += buf.len() as Size;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

use std::io::SeekFrom;

impl<'a, T: Frontend> std::io::Seek for ObjCursor<'a, T> {
    fn seek(&mut self, s: SeekFrom) -> std::io::Result<u64> {
        match s {
            SeekFrom::Start(v) => {
                self.1 = v as u64;
                Ok(v)
            }
            SeekFrom::Current(v) => {
                if v >= 0 {
                    self.1 += v as Size;
                } else {
                    self.1 -= (-v) as Size;
                }
                Ok(self.1 as u64)
            }
            SeekFrom::End(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "not supprted",
            )),
        }
    }
}
