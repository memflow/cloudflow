use abi_stable::{
    abi_stability::check_layout_compatibility, std_types::UTypeId, type_layout::TypeLayout,
    StableAbi,
};
use cglue::result::from_int_result_empty;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use core::mem::MaybeUninit;
use dashmap::mapref::entry;
use dashmap::DashMap;
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;
pub use memflow::types::Address;
use sharded_slab::{Entry, Slab};
use std::collections::{BTreeMap, HashMap};

/*pub struct ManualArc<T> {

}*/

pub enum HandleMap {
    Forward(usize, usize),
    Object(FileOpsObj<c_void>),
}

pub type MappingFunction<T, O> = for<'a> fn(&'a T) -> Option<O>;

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
    entry_list: Slab<DashMap<String, OpaqueMapping>>,
    /// Type layouts identified by slab index.
    layouts: DashMap<usize, &'static TypeLayout>,
    /// Map a specific type to opaque entries in the list.
    type_map: DashMap<UTypeId, usize>,
}

impl PluginStore {
    pub fn entries<T: StableAbi>(&self) -> Entry<DashMap<String, Mapping<T>>> {
        let id = T::ABI_CONSTS.type_id.get();

        let idx = *self.type_map.entry(id).or_insert_with(|| {
            self.layouts
                .iter()
                .find(|p| check_layout_compatibility(T::LAYOUT, p.value()).is_ok())
                .map(|p| *p.key())
                .or_else(|| {
                    self.entry_list.insert(Default::default()).map(|i| {
                        self.layouts.insert(i, T::LAYOUT);
                        i
                    })
                })
                .expect("Slab is full!")
        });

        unsafe { std::mem::transmute(self.entry_list.get(idx).unwrap()) }
    }

    pub fn register_mapping<T: StableAbi>(&self, name: &str, mapping: Mapping<T>) -> bool {
        let entries = self.entries::<T>();

        let entry = entries.entry(name.to_string());

        if matches!(entry, entry::Entry::Vacant(_)) {
            entry.or_insert(mapping);
            true
        } else {
            false
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
        let plugins = PluginStore::default();

        plugins.register_mapping(
            "rpc",
            Mapping::Leaf(|os: &OsInstanceArcBox<'static>| Some(Box::new(os.clone()) as _)),
        );

        plugins.register_mapping(
            "rpc",
            Mapping::Leaf(|conn: &CArc<ThreadedConnector>| Some(Box::new(conn.clone()) as _)),
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
    fn read(&self, handle: usize, data: CIterator<ReadData>) -> Result<()> {
        self.backend.read(handle, data)
    }
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<WriteData>) -> Result<()> {
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
    fn list(&self, path: &str) -> Result<Vec<(String, bool)>> {
        self.backend.list(path, &self.plugins)
    }
}

pub struct NodeBackend {
    backends: Slab<Box<dyn Backend>>,
    /// Maps backend name to backend ID in the vec
    backend_map: DashMap<String, usize>,
    /// Maps handle to (backend, handle) pair.
    handles: Slab<HandleMap>,
}

impl Default for NodeBackend {
    fn default() -> Self {
        let mut ret = Self {
            backends: Slab::new(),
            backend_map: Default::default(),
            handles: Slab::new(),
        };

        let connector = LocalBackend::<CArc<ThreadedConnector>>::default();

        let inventory = Inventory::scan();

        let kcore = inventory.create_connector("kcore", None, None).unwrap();
        connector.insert("kcore", ThreadedConnector::from(kcore).into());

        ret.add_backend("connector", connector);

        let os = LocalBackend::<OsInstanceArcBox>::default();

        let native = inventory.create_os("native", None, None).unwrap();
        os.insert("native", native);

        ret.add_backend("os", os);

        ret
    }
}

impl NodeBackend {
    fn add_backend(&self, name: &str, backend: impl Backend + 'static) {
        let name = name.to_string();

        self.backend_map.entry(name.to_string()).or_insert_with(|| {
            self.backends
                .insert(Box::new(backend) as _)
                .expect("Slab is full!")
        });
    }
}

impl Backend for NodeBackend {
    fn read(&self, handle: usize, data: CIterator<ReadData>) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.read(handle, data)
                } else {
                    Err(ErrorKind::InvalidPath.into())
                }
            }
            Some(HandleMap::Object(obj)) => obj.read(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn write(&self, handle: usize, data: CIterator<WriteData>) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.write(handle, data)
                } else {
                    Err(ErrorKind::InvalidPath.into())
                }
            }
            Some(HandleMap::Object(obj)) => obj.write(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handles.get(handle).as_ref().map(|v| &**v) {
            Some(&HandleMap::Forward(backend, handle)) => {
                if let Some(backend) = self.backends.get(backend) {
                    backend.rpc(handle, input, output)
                } else {
                    Err(ErrorKind::InvalidPath.into())
                }
            }
            Some(HandleMap::Object(obj)) => obj.rpc(input, output),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn open(&self, path: &str, plugins: &PluginStore) -> Result<usize> {
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

        Err(ErrorKind::NotFound.into())
    }

    fn list(&self, path: &str, plugins: &PluginStore) -> Result<Vec<(String, bool)>> {
        let path = path.trim_start_matches('/');
        let (backend, path) = path.split_once("/").unwrap_or((path, ""));

        if !backend.is_empty() {
            if let Some(backend) = self
                .backend_map
                .get(backend)
                .and_then(|idx| self.backends.get(*idx))
            {
                return backend.list(path, plugins);
            }
        } else {
            return Ok(self
                .backend_map
                .iter()
                .map(|r| r.key().clone())
                .map(|k| (k, true))
                .collect());
        }

        Err(ErrorKind::NotFound.into())
    }
}

struct LocalBackend<T> {
    entries: DashMap<String, T>,
    handle_objs: Slab<FileOpsObj<c_void>>,
}

impl<T> Default for LocalBackend<T> {
    fn default() -> Self {
        Self {
            entries: Default::default(),
            handle_objs: Slab::new(),
        }
    }
}

//impl<T: Branch> FileOps for LocalBackend<T> {
//}

impl<T> LocalBackend<T> {
    pub fn insert(&self, name: &str, entry: T) -> bool {
        if self.entries.contains_key(name) {
            false
        } else {
            self.entries.insert(name.into(), entry).is_none()
        }
    }

    fn push_obj(&self, obj: FileOpsObj<c_void>) -> usize {
        self.handle_objs.insert(obj).unwrap()
    }
}

impl<T: Branch> Backend for LocalBackend<T> {
    fn read(&self, handle: usize, data: CIterator<ReadData>) -> Result<()> {
        match self.handle_objs.get(handle) {
            Some(f) => f.read(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn write(&self, handle: usize, data: CIterator<WriteData>) -> Result<()> {
        match self.handle_objs.get(handle) {
            Some(f) => f.write(data),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handle_objs.get(handle) {
            Some(f) => f.rpc(input, output),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn open(&self, path: &str, plugins: &PluginStore) -> Result<usize> {
        let (branch, path) = path.split_once("/").unwrap_or((path, ""));
        match self
            .entries
            .get(branch)
            .and_then(|b| Some(b.get_entry(path, plugins)))
        {
            Some(Ok(DirEntry::Leaf(leaf))) => leaf.open().map(|o| self.push_obj(o)),
            Some(Ok(_)) => Err(ErrorKind::InvalidArgument.into()),
            Some(Err(e)) => Err(e),
            _ => Err(ErrorKind::NotFound.into()),
        }
    }

    fn list(&self, path: &str, plugins: &PluginStore) -> Result<Vec<(String, bool)>> {
        if path.is_empty() {
            Ok(self
                .entries
                .iter()
                .map(|r| r.key().clone())
                .map(|n| (n, true))
                .collect())
        } else {
            let (branch, path) = path.split_once("/").unwrap_or((path, ""));
            match self.entries.get(branch) {
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
    fn read(&self, handle: usize, data: CIterator<ReadData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<WriteData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str, plugins: &PluginStore) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&self, path: &str, plugins: &PluginStore) -> Result<Vec<(String, bool)>>;
}

pub trait Frontend {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, data: CIterator<ReadData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<WriteData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(&self, path: &str) -> Result<Vec<(String, bool)>>;
}

fn branch_map_entry<T: Branch>(
    branch: &T,
    entry: Mapping<T>,
    remote: Option<&str>,
    plugins: &PluginStore,
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
    branch: &T,
    path: &str,
    plugins: &PluginStore,
) -> Result<DirEntry> {
    let (local, remote) = path
        .split_once("/")
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((path, None));

    let entries = plugins.entries::<T>();

    let entry = entries.get(local).ok_or(ErrorKind::NotFound)?;

    branch_map_entry(branch, *entry, remote, plugins)
}

fn branch_list<T: Branch + StableAbi>(
    branch: &T,
    plugins: &PluginStore,
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

impl Branch for CArc<ThreadedConnector> {
    fn get_entry(&self, path: &str, plugins: &PluginStore) -> Result<DirEntry> {
        branch_get_entry(self, path, plugins)
    }

    fn list(&self, plugins: &PluginStore) -> Result<HashMap<String, DirEntry>> {
        branch_list(self, plugins)
    }
}

impl Leaf for CArc<ThreadedConnector> {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            self.clone(),
            Some(ThreadedConnector::read),
            Some(ThreadedConnector::write),
            Some(ThreadedConnector::rpc),
        ))
    }
}

#[derive(StableAbi)]
#[repr(C)]
pub struct ThreadCtx<T: 'static> {
    orig: T,
    stack: CBox<'static, c_void>,
    stack_push: for<'a> extern "C" fn(&c_void, COption<T>),
    stack_pop: for<'a> extern "C" fn(&c_void, &mut MaybeUninit<COption<T>>) -> bool,
}

pub struct ThreadCtxHandle<'a, T: 'static> {
    value: MaybeUninit<T>,
    ctx: &'a ThreadCtx<T>,
}

impl<T> Drop for ThreadCtxHandle<'_, T> {
    fn drop(&mut self) {
        self.ctx.push(Some(unsafe { self.value.as_ptr().read() }))
    }
}

impl<T> core::ops::Deref for ThreadCtxHandle<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.value.as_ptr().as_ref().unwrap() }
    }
}

impl<T> core::ops::DerefMut for ThreadCtxHandle<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.value.as_mut_ptr().as_mut().unwrap() }
    }
}

impl<T> ThreadCtx<T> {
    pub fn new(orig: T, size: usize) -> Self {
        // SAFETY: All types in the opaque functions match!!! It is safe, but needs care!!!

        let stack = crossbeam_deque::Worker::<COption<T>>::new_lifo();

        for _ in 0..size {
            stack.push(COption::None);
        }

        let stack = CBox::from(stack).into_opaque();

        extern "C" fn stack_pop<T>(stack: &c_void, out: &mut MaybeUninit<COption<T>>) -> bool {
            match unsafe {
                (*(stack as *const _ as *const crossbeam_deque::Worker<COption<T>>)).pop()
            } {
                Some(t) => {
                    out.write(t);
                    true
                }
                None => false,
            }
        }

        extern "C" fn stack_push<T>(stack: &c_void, val: COption<T>) {
            unsafe {
                (*(stack as *const _ as *const crossbeam_deque::Worker<COption<T>>)).push(val)
            };
        }

        Self {
            orig,
            stack,
            stack_pop: stack_pop::<T>,
            stack_push: stack_push::<T>,
        }
    }

    fn push(&self, val: Option<T>) {
        (self.stack_push)(&*self.stack, val.into())
    }

    fn pop(&self) -> Option<Option<T>> {
        let mut out = MaybeUninit::uninit();
        if (self.stack_pop)(&*self.stack, &mut out) {
            Some(unsafe { out.assume_init() }.into())
        } else {
            None
        }
    }
}

impl<T: Clone> ThreadCtx<T> {
    pub fn get(&self) -> ThreadCtxHandle<T> {
        let v = loop {
            match self.pop() {
                Some(Some(v)) => break v,
                Some(None) => break self.orig.clone(),
                None => {}
            }
        };

        ThreadCtxHandle {
            value: MaybeUninit::new(v),
            ctx: self,
        }
    }
}

#[derive(StableAbi)]
#[repr(C)]
pub struct ThreadedConnector(ThreadCtx<ConnectorInstanceArcBox<'static>>);

impl ThreadedConnector {
    extern "C" fn read(&self, data: CIterator<ReadData>) -> i32 {
        self.0
            .get()
            .phys_view()
            .read_raw_iter(data, &mut (&mut |_: ReadData| true).into())
            .into_int_result()
    }

    extern "C" fn write(&self, data: CIterator<WriteData>) -> i32 {
        self.0
            .get()
            .phys_view()
            .write_raw_iter(data, &mut (&mut |_: WriteData| true).into())
            .into_int_result()
    }

    extern "C" fn rpc(&self, input: CSliceRef<u8>, output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}

impl From<ConnectorInstanceArcBox<'static>> for ThreadedConnector {
    fn from(conn: ConnectorInstanceArcBox<'static>) -> Self {
        Self(ThreadCtx::new(conn, 32))
    }
}

impl Branch for OsInstanceArcBox<'static> {
    fn get_entry(&self, path: &str, plugins: &PluginStore) -> Result<DirEntry> {
        branch_get_entry(self, path, plugins)
    }

    fn list(&self, plugins: &PluginStore) -> Result<HashMap<String, DirEntry>> {
        branch_list(self, plugins)
    }
}

impl Leaf for OsInstanceArcBox<'static> {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(CArc::from(self.clone()), None, None, None))
    }
}

// Internal FS representation. Does not cross backends.

pub enum DirEntry {
    Branch(Box<dyn Branch>),
    Leaf(Box<dyn Leaf>),
}

//#[cglue_trait]
pub trait Branch {
    fn get_entry(&self, path: &str, plugins: &PluginStore) -> Result<DirEntry>;
    fn list(&self, plugins: &PluginStore) -> Result<HashMap<String, DirEntry>>;

    fn list_recurse(&self, path: &str, plugins: &PluginStore) -> Result<HashMap<String, DirEntry>> {
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

pub struct FileOpsObj<T: 'static> {
    obj: CArc<T>,
    read: Option<for<'a> extern "C" fn(&'a T, data: CIterator<ReadData>) -> i32>,
    write: Option<for<'a> extern "C" fn(&'a T, data: CIterator<WriteData>) -> i32>,
    rpc: Option<
        for<'a> extern "C" fn(&'a T, input: CSliceRef<'a, u8>, output: CSliceMut<'a, u8>) -> i32,
    >,
}

impl<T> FileOpsObj<T> {
    pub fn new(
        obj: CArc<T>,
        read: Option<for<'a> extern "C" fn(&'a T, data: CIterator<ReadData>) -> i32>,
        write: Option<for<'a> extern "C" fn(&'a T, data: CIterator<WriteData>) -> i32>,
        rpc: Option<
            for<'a> extern "C" fn(
                &'a T,
                input: CSliceRef<'a, u8>,
                output: CSliceMut<'a, u8>,
            ) -> i32,
        >,
    ) -> FileOpsObj<c_void> {
        Self {
            obj,
            read,
            write,
            rpc,
        }
        .into_opaque()
    }

    pub fn read(&self, data: CIterator<ReadData>) -> Result<()> {
        from_int_result_empty((self.read.ok_or(ErrorKind::NotImplemented)?)(
            self.obj.as_ref().unwrap(),
            data,
        ))
    }

    pub fn write(&self, data: CIterator<WriteData>) -> Result<()> {
        from_int_result_empty((self.write.ok_or(ErrorKind::NotImplemented)?)(
            self.obj.as_ref().unwrap(),
            data,
        ))
    }

    pub fn rpc(&self, input: &[u8], output: &mut [u8]) -> Result<()> {
        from_int_result_empty((self.rpc.ok_or(ErrorKind::NotImplemented)?)(
            self.obj.as_ref().unwrap(),
            input.into(),
            output.into(),
        ))
    }
}

unsafe impl<T> cglue::trait_group::Opaquable for FileOpsObj<T> {
    type OpaqueTarget = FileOpsObj<c_void>;
}

//#[cglue_trait]
pub trait Leaf {
    fn open(&self) -> Result<FileOpsObj<c_void>>;
}
