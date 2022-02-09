use abi_stable::{
    abi_stability::check_layout_compatibility, std_types::UTypeId, type_layout::TypeLayout,
    StableAbi,
};
use cglue::trait_group::c_void;
use memflow::prelude::v1::*;
use std::collections::{BTreeMap, HashMap};

/*pub struct ManualArc<T> {

}*/

pub enum HandleMap {
    Forward(usize, usize),
    Object(Box<dyn FileOps>),
}

pub type MappingFunction<T, O> = for<'a> fn(&'a mut T) -> Option<O>;

pub enum Mapping<T> {
    Branch(MappingFunction<T, Box<dyn Branch>>), //BranchArcBox<'static>>),
    Leaf(MappingFunction<T, Box<dyn Leaf>>),     //LeafArcBox<'static>>)
}

impl<T> Copy for Mapping<T> {}

impl<T> Clone for Mapping<T> {
    fn clone(&self) -> Self {
        *self
    }
}

type OpaqueMapping = Mapping<c_void>;

#[derive(Default)]
pub struct PluginStore {
    // plugins: Vec<Box<dyn Plugin>>
    /// A never shrinking list of lists of mapping functions meant for specific types of plugins.
    ///
    /// Calling the mapping functions manually is inherently unsafe, because the types are meant to
    /// be opaque, and unchecked downcasting is being performed.
    entry_list: Vec<(&'static TypeLayout, BTreeMap<String, OpaqueMapping>)>,
    /// Map a specific type to opaque entries in the list.
    type_map: HashMap<UTypeId, usize>,
}

impl PluginStore {
    pub fn entries<T: StableAbi>(&mut self) -> &mut BTreeMap<String, Mapping<T>> {
        let id = T::ABI_CONSTS.type_id.get();

        let idx = *self.type_map.entry(id).or_insert_with(|| {
            self.entry_list
                .iter()
                .enumerate()
                .find(|(_, (layout, _))| check_layout_compatibility(T::LAYOUT, layout).is_ok())
                .map(|(i, _)| i)
                .unwrap_or(self.entry_list.len())
        });

        if idx == self.entry_list.len() {
            self.entry_list.push((T::LAYOUT, Default::default()));
        }

        unsafe {
            (&mut self.entry_list[idx].1 as *mut _ as *mut BTreeMap<String, Mapping<T>>)
                .as_mut()
                .unwrap()
        }
    }

    pub fn register_mapping<T: StableAbi>(&mut self, name: &str, mapping: Mapping<T>) -> bool {
        let entries = self.entries::<T>();

        if entries.contains_key(name) {
            false
        } else {
            entries.insert(name.to_string(), mapping);
            true
        }
    }
}

pub struct Node {
    backend: NodeBackend,
    frontends: Vec<Box<dyn Frontend>>,
    plugins: PluginStore,
}

impl Default for Node {
    fn default() -> Self {
        let backend = NodeBackend::default();
        let mut plugins = PluginStore::default();

        plugins.register_mapping(
            "rpc",
            Mapping::Leaf(|os: &mut OsInstanceArcBox<'static>| Some(Box::new(os.clone()) as _)),
        );

        Self {
            backend,
            frontends: vec![],
            plugins,
        }
    }
}

impl Frontend for Node {
    /// Perform read operation on the given handle
    fn read(&mut self, handle: usize, data: CIterator<ReadData>) -> Result<()> {
        self.backend.read(handle, data)
    }
    /// Perform write operation on the given handle.
    fn write(&mut self, handle: usize, data: CIterator<WriteData>) -> Result<()> {
        self.backend.write(handle, data)
    }
    /// Perform remote procedure call on the given handle.
    fn rpc(&mut self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        self.backend.rpc(handle, input, output)
    }
    /// Open a leaf at the given path. The result is a handle.
    fn open(&mut self, path: &str) -> Result<usize> {
        self.backend.open(path, &mut self.plugins)
    }
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&mut self, path: &str) -> Result<Vec<(String, bool)>> {
        self.backend.list(path, &mut self.plugins)
    }
}

pub struct NodeBackend {
    backends: Vec<Box<dyn Backend>>,
    /// Maps backend name to backend ID in the vec
    backend_map: HashMap<String, usize>,
    /// Maps handle to (backend, handle) pair.
    handles: Vec<HandleMap>,
}

impl Default for NodeBackend {
    fn default() -> Self {
        let mut ret = Self {
            backends: vec![],
            backend_map: Default::default(),
            handles: vec![],
        };

        ret.add_backend(
            "connector",
            LocalBackend::<ConnectorInstanceArcBox>::default(),
        );

        let mut os = LocalBackend::<OsInstanceArcBox>::default();

        let inventory = Inventory::scan();
        let native = inventory.create_os("native", None, None).unwrap();

        os.insert("native", native);

        ret.add_backend("os", os);

        ret
    }
}

impl NodeBackend {
    fn add_backend(&mut self, name: &str, backend: impl Backend + 'static) {
        let name = name.to_string();
        if self.backend_map.get(&name).is_none()
            && self.backend_map.insert(name, self.backends.len()).is_none()
        {
            self.backends.push(Box::new(backend) as _);
        }
    }
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
        let path = path.trim_start_matches('/');
        let (backend, path) = path.split_once("/").unwrap_or((path, ""));

        if !backend.is_empty() {
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

struct LocalBackend<T> {
    entries: HashMap<String, T>,
    handle_objs: Vec<Box<dyn FileOps>>,
}

impl<T> Default for LocalBackend<T> {
    fn default() -> Self {
        Self {
            entries: Default::default(),
            handle_objs: vec![],
        }
    }
}

//impl<T: Branch> FileOps for LocalBackend<T> {
//}

impl<T> LocalBackend<T> {
    pub fn insert(&mut self, name: &str, entry: T) -> bool {
        if self.entries.contains_key(name) {
            false
        } else {
            self.entries.insert(name.into(), entry).is_none()
        }
    }
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

pub trait Frontend {
    /// Perform read operation on the given handle
    fn read(&mut self, handle: usize, data: CIterator<ReadData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&mut self, handle: usize, data: CIterator<WriteData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&mut self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&mut self, path: &str) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&mut self, path: &str) -> Result<Vec<(String, bool)>>;
}

fn branch_map_entry<T: Branch>(
    branch: &mut T,
    entry: Mapping<T>,
    remote: Option<&str>,
    plugins: &mut PluginStore,
) -> Result<DirEntry> {
    match (remote, entry) {
        (Some(path), Mapping::Branch(map)) => map(branch)
            .ok_or(ErrorKind::NotFound)?
            .get_entry(path, plugins),
        (None, Mapping::Branch(map)) => {
            Ok(DirEntry::Branch(map(branch).ok_or(ErrorKind::NotFound)?))
        }
        (None, Mapping::Leaf(map)) => Ok(DirEntry::Leaf(map(branch).ok_or(ErrorKind::NotFound)?)),
        _ => Err(ErrorKind::NotFound.into()),
    }
}

fn branch_get_entry<T: Branch + StableAbi>(
    branch: &mut T,
    path: &str,
    plugins: &mut PluginStore,
) -> Result<DirEntry> {
    let (local, remote) = path
        .split_once("/")
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((path, None));

    let entries = plugins.entries::<T>();

    let entry = entries.get_mut(local).ok_or(ErrorKind::NotFound)?;

    branch_map_entry(branch, *entry, remote, plugins)
}

fn branch_list<T: Branch + StableAbi>(
    branch: &mut T,
    plugins: &mut PluginStore,
) -> Result<HashMap<String, DirEntry>> {
    // TODO: do without clone
    let entries = plugins.entries::<T>().clone();

    entries
        .into_iter()
        .map(|(name, entry)| {
            branch_map_entry(branch, entry, None, plugins).map(|entry| (name, entry))
        })
        .collect()
}

impl Branch for ConnectorInstanceArcBox<'static> {
    fn get_entry(&mut self, path: &str, plugins: &mut PluginStore) -> Result<DirEntry> {
        branch_get_entry(self, path, plugins)
    }

    fn list(&mut self, plugins: &mut PluginStore) -> Result<HashMap<String, DirEntry>> {
        branch_list(self, plugins)
    }
}

impl Leaf for ConnectorInstanceArcBox<'static> {
    fn open(&mut self) -> Result<Box<dyn FileOps>> {
        Ok(Box::new(self.clone()) as _)
    }
}

impl FileOps for ConnectorInstanceArcBox<'static> {
    fn read(&mut self, data: CIterator<ReadData>) -> Result<()> {
        self.phys_view()
            .read_raw_iter(data, &mut (&mut |_: ReadData| true).into())
    }

    fn write(&mut self, data: CIterator<WriteData>) -> Result<()> {
        self.phys_view()
            .write_raw_iter(data, &mut (&mut |_: WriteData| true).into())
    }

    fn rpc(&mut self, input: &[u8], output: &mut [u8]) -> Result<()> {
        Ok(())
    }
}

impl Branch for OsInstanceArcBox<'static> {
    fn get_entry(&mut self, path: &str, plugins: &mut PluginStore) -> Result<DirEntry> {
        branch_get_entry(self, path, plugins)
    }

    fn list(&mut self, plugins: &mut PluginStore) -> Result<HashMap<String, DirEntry>> {
        branch_list(self, plugins)
    }
}

impl Leaf for OsInstanceArcBox<'static> {
    fn open(&mut self) -> Result<Box<dyn FileOps>> {
        Ok(Box::new(self.clone()) as _)
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

//#[cglue_trait]
pub trait Branch {
    fn get_entry(&mut self, path: &str, plugins: &mut PluginStore) -> Result<DirEntry>;
    fn list(&mut self, plugins: &mut PluginStore) -> Result<HashMap<String, DirEntry>>;

    fn list_recurse(
        &mut self,
        path: &str,
        plugins: &mut PluginStore,
    ) -> Result<HashMap<String, DirEntry>> {
        if path.is_empty() {
            self.list(plugins)
        } else {
            match self.get_entry(path, plugins) {
                Ok(DirEntry::Branch(mut branch)) => branch.list(plugins),
                Ok(_) => Err(ErrorKind::InvalidPath.into()),
                Err(e) => Err(e),
            }
        }
    }
}

//#[cglue_trait]
pub trait Leaf {
    fn open(&mut self) -> Result<Box<dyn FileOps>>;
}
