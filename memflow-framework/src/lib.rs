use abi_stable::{type_layout::TypeLayout, StableAbi};
use cglue::trait_group::c_void;
use memflow::prelude::v1::*;
use std::collections::{BTreeMap, HashMap};

/*pub struct ManualArc<T> {

}*/

pub enum HandleMap {
    Forward(usize, usize),
    Object(Box<dyn FileOps>),
}

type MappingFunction =
    for<'a> unsafe extern "C" fn(&'a mut c_void) -> Option<CBox<'static, c_void>>;

struct BranchEntry {
    mapper: MappingFunction,
    is_branch: bool,
}

pub struct PluginStore {
    // plugins: Vec<Box<dyn Plugin>>
    /// A never shrinking list of lists of mapping functions meant for specific types of plugins.
    ///
    /// Calling the mapping functions manually is inherently unsafe, because the types are meant to
    /// be opaque, and unchecked downcasting is being performed.
    entry_list: Vec<BTreeMap<String, BranchEntry>>,
    /// Map a specific type to opaque entries in the list.
    type_map: HashMap<&'static TypeLayout, usize>,
}

pub struct Node {
    backend: NodeBackend,
    frontends: Vec<Box<dyn Frontend>>,
    plugins: PluginStore,
}

pub struct NodeBackend {
    backends: Vec<Box<dyn Backend>>,
    /// Maps backend name to backend ID in the vec
    backend_map: HashMap<String, usize>,
    /// Maps handle to (backend, handle) pair.
    handles: Vec<HandleMap>,
}

impl Backend for NodeBackend {
    fn read(&mut self, handle: usize, data: CIterator<ReadData>) -> Result<()> {
        match self.handles.get_mut(handle) {
            Some(&mut HandleMap::Forward(backend, handle)) if backend < self.backends.len() => {
                self.backends[backend].read(handle, data)
            }
            Some(HandleMap::Object(obj)) => obj.read(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn write(&mut self, handle: usize, data: CIterator<WriteData>) -> Result<()> {
        match self.handles.get_mut(handle) {
            Some(&mut HandleMap::Forward(backend, handle)) if backend < self.backends.len() => {
                self.backends[backend].write(handle, data)
            }
            Some(HandleMap::Object(obj)) => obj.write(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn rpc(&mut self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handles.get_mut(handle) {
            Some(&mut HandleMap::Forward(backend, handle)) if backend < self.backends.len() => {
                self.backends[backend].rpc(handle, input, output)
            }
            Some(HandleMap::Object(obj)) => obj.rpc(input, output),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn open(&mut self, path: &str, plugins: &mut PluginStore) -> Result<usize> {
        if let Some((backend, path)) = path.split_once("/") {
            if let Some(&idx) = self.backend_map.get(backend) {
                return self.backends[idx].open(path, plugins);
            }
        }

        Err(ErrorKind::NotFound.into())
    }

    fn list(&mut self, path: &str, plugins: &mut PluginStore) -> Result<Vec<(String, bool)>> {
        if let Some((backend, path)) = path.split_once("/") {
            if let Some(&idx) = self.backend_map.get(backend) {
                return self.backends[idx].list(path, plugins);
            }
        } else {
            return Ok(self
                .backend_map
                .keys()
                .cloned()
                .map(|k| (k, true))
                .collect());
        }

        Err(ErrorKind::NotFound.into())
    }
}

/*

// Local backend is not needed, since we can just create 2 backends for OSs and Connectors
// separately

pub struct LocalBackend {
    connector: Vec<ConnectorInstanceArcBox<'static>>,
    os: Vec<OsInstanceArcBox<'static>>,
    handles: Vec<Option<(bool, usize)>>,
}

impl Backend for LocalBackend {
    fn read(&mut self, handle: usize, data: &mut [u8]) -> Result<()> {
        match self.handles.get(handle) {
            Some(&Some((false, handle))) => self.connector.read(handle, data),
            Some(&Some((true, handle))) => self.os.read(handle, data),
            _ => Err(ErrorKind::NotFound.into())
        }
    }

    fn write(&mut self, handle: usize, data: &[u8]) -> Result<()> {
        match self.handles.get(handle) {
            Some(&Some((false, handle))) => self.connector.write(handle, data),
            Some(&Some((true, handle))) => self.os.write(handle, data),
            _ => Err(ErrorKind::NotFound.into())
        }
    }

    fn rpc(&mut self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handles.get(handle) {
            Some(&Some((false, handle))) => self.connector.rpc(handle, input, output),
            Some(&Some((true, handle))) => self.os.rpc(handle, input, output),
            _ => Err(ErrorKind::NotFound.into())
        }
    }
}*/

struct LocalBackend<T> {
    entries: HashMap<String, T>,
    handle_objs: Vec<Box<dyn FileOps>>,
}

impl<T: Branch> Backend for LocalBackend<T> {
    fn read(&mut self, handle: usize, data: CIterator<ReadData>) -> Result<()> {
        match self.handle_objs.get_mut(handle) {
            Some(f) => f.read(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn write(&mut self, handle: usize, data: CIterator<WriteData>) -> Result<()> {
        match self.handle_objs.get_mut(handle) {
            Some(f) => f.write(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn rpc(&mut self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handle_objs.get_mut(handle) {
            Some(f) => f.rpc(input, output),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn open(&mut self, path: &str, plugins: &mut PluginStore) -> Result<usize> {
        let (branch, path) = path.split_once("/").unwrap_or((path, ""));
        match self
            .entries
            .get_mut(branch)
            .and_then(|b| Some(b.get_entry(path, plugins)))
        {
            Some(Ok(DirEntry::Leaf(mut leaf))) => {
                self.handle_objs.push(leaf.open()?);
                Ok(self.handle_objs.len() - 1)
            }
            Some(Ok(_)) => Err(ErrorKind::InvalidArgument.into()),
            Some(Err(e)) => Err(e),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn list(&mut self, path: &str, plugins: &mut PluginStore) -> Result<Vec<(String, bool)>> {
        if path.is_empty() {
            Ok(self.entries.keys().cloned().map(|n| (n, true)).collect())
        } else {
            let (branch, path) = path.split_once("/").unwrap_or((path, ""));
            match self.entries.get_mut(branch) {
                Some(branch) => Ok(branch
                    .list_recurse(path, plugins)?
                    .into_iter()
                    .map(|(name, obj)| (name, matches!(obj, DirEntry::Branch(_))))
                    .collect()),
                _ => Err(ErrorKind::NotFound.into()),
            }
        }
    }
}

pub trait Backend {
    /// Perform read operation on the given handle
    fn read(&mut self, handle: usize, data: CIterator<ReadData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&mut self, handle: usize, data: CIterator<WriteData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&mut self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&mut self, path: &str, plugins: &mut PluginStore) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&mut self, path: &str, plugins: &mut PluginStore) -> Result<Vec<(String, bool)>>;
}

pub trait Frontend {}

impl FileOps for ConnectorInstanceArcBox<'static> {
    fn read(&mut self, data: CIterator<ReadData>) -> Result<()> {
        Ok(())
    }

    fn write(&mut self, data: CIterator<WriteData>) -> Result<()> {
        Ok(())
    }

    fn rpc(&mut self, input: &[u8], output: &mut [u8]) -> Result<()> {
        Ok(())
    }
}

impl Branch for ConnectorInstanceArcBox<'static> {
    fn get_entry(&mut self, path: &str, plugins: &mut PluginStore) -> Result<DirEntry> {
        match path.split_once("/") {
            Some((branch, path)) => {}
            None => {}
        }
        todo!()
    }

    fn list(&mut self, plugins: &mut PluginStore) -> Result<Vec<(String, DirEntry)>> {
        todo!()
    }
}

impl FileOps for OsInstanceArcBox<'static> {
    fn read(&mut self, data: CIterator<ReadData>) -> Result<()> {
        Ok(())
    }

    fn write(&mut self, data: CIterator<WriteData>) -> Result<()> {
        Ok(())
    }

    fn rpc(&mut self, input: &[u8], output: &mut [u8]) -> Result<()> {
        Ok(())
    }
}

// Internal FS representation. Does not cross backends.

pub trait FileOps {
    fn read(&mut self, data: CIterator<ReadData>) -> Result<()>;
    fn write(&mut self, data: CIterator<WriteData>) -> Result<()>;
    fn rpc(&mut self, input: &[u8], output: &mut [u8]) -> Result<()>;
}

pub enum DirEntry {
    Branch(Box<dyn Branch>),
    Leaf(Box<dyn Leaf>),
}

pub trait Branch {
    fn get_entry(&mut self, path: &str, plugins: &mut PluginStore) -> Result<DirEntry>;
    fn list(&mut self, plugins: &mut PluginStore) -> Result<Vec<(String, DirEntry)>>;

    fn list_recurse(
        &mut self,
        path: &str,
        plugins: &mut PluginStore,
    ) -> Result<Vec<(String, DirEntry)>> {
        match self.get_entry(path, plugins) {
            Ok(DirEntry::Branch(mut branch)) => branch.list(plugins),
            Ok(_) => Err(ErrorKind::InvalidPath.into()),
            Err(e) => Err(e),
        }
    }
}

pub trait Leaf {
    fn open(&mut self) -> Result<Box<dyn FileOps>>;
}
