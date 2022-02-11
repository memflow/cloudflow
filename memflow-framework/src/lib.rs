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

#[derive(StableAbi)]
#[repr(C)]
pub enum HandleMap {
    Forward(usize, usize),
    Object(FileOpsObj<c_void>),
}

pub type MappingFunction<T, O> = extern "C" fn(&T) -> COption<O>;

#[derive(StableAbi)]
// TODO: Why does the func type not work??
#[sabi(unsafe_opaque_fields)]
#[repr(C)]
pub enum Mapping<T: StableAbi> {
    Branch(MappingFunction<T, BranchBox<'static>>), //BranchBox<'static>>),
    Leaf(MappingFunction<T, LeafBox<'static>>),     //LeafBox<'static>>)
}

unsafe impl<T: StableAbi> Opaquable for Mapping<T> {
    type OpaqueTarget = Mapping<c_void>;
}

impl<T: StableAbi> Copy for Mapping<T> {}

impl<T: StableAbi> Clone for Mapping<T> {
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

#[derive(StableAbi)]
#[repr(C)]
pub struct ListEntry {
    pub name: ReprCString,
    pub is_branch: bool,
}

impl ListEntry {
    fn new(name: ReprCString, is_branch: bool) -> Self {
        Self { name, is_branch }
    }
}

#[derive(StableAbi)]
#[repr(C)]
pub struct CTup2<A, B>(pub A, pub B);

#[derive(StableAbi)]
#[repr(C)]
pub struct CPluginStore {
    store: CBox<'static, c_void>,
    lookup_entry: for<'a> unsafe extern "C" fn(
        &'a c_void,
        UTypeId,
        &'static TypeLayout,
        CSliceRef<'a, u8>,
    ) -> COption<OpaqueMapping>,
    entry_list: for<'a> unsafe extern "C" fn(
        &'a c_void,
        UTypeId,
        &'static TypeLayout,
        &mut OpaqueCallback<*const c_void>,
    ),
    register_mapping: for<'a> unsafe extern "C" fn(
        &'a c_void,
        UTypeId,
        &'static TypeLayout,
        CSliceRef<'a, u8>,
        OpaqueMapping,
    ) -> bool,
}

impl Default for CPluginStore {
    fn default() -> Self {
        PluginStore::default().into()
    }
}

impl From<PluginStore> for CPluginStore {
    fn from(store: PluginStore) -> Self {
        unsafe extern "C" fn lookup_entry(
            store: &c_void,
            id: UTypeId,
            layout: &'static TypeLayout,
            name: CSliceRef<u8>,
        ) -> COption<OpaqueMapping> {
            let store = store as *const _ as *const PluginStore;
            let entries = (*store).entries_raw(id, layout);
            entries.get(name.into_str()).map(|e| *e).into()
        }

        unsafe extern "C" fn entry_list<'a>(
            store: &c_void,
            id: UTypeId,
            layout: &'static TypeLayout,
            out: &mut OpaqueCallback<*const c_void>,
        ) {
            let store: &PluginStore = &*(store as *const _ as *const PluginStore);
            let entries = (*store).entries_raw(id, layout);
            entries
                .iter()
                .take_while(|e| {
                    out.call(
                        &CTup2(CSliceRef::from(e.key().as_str()), *e.value()) as *const _
                            as *const c_void,
                    )
                })
                .for_each(|_| {});
        }

        unsafe extern "C" fn register_mapping(
            store: &c_void,
            id: UTypeId,
            layout: &'static TypeLayout,
            name: CSliceRef<u8>,
            mapping: OpaqueMapping,
        ) -> bool {
            let store = store as *const _ as *const PluginStore;
            (*store).register_mapping_raw(id, layout, name.into_str(), mapping)
        }

        Self {
            store: CBox::from(store).into_opaque(),
            lookup_entry,
            entry_list,
            register_mapping,
        }
    }
}

impl CPluginStore {
    pub fn register_mapping<T: StableAbi>(&self, name: &str, mapping: Mapping<T>) -> bool {
        unsafe {
            (self.register_mapping)(
                &*self.store,
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                name.into(),
                mapping.into_opaque(),
            )
        }
    }

    pub fn lookup_entry<T: StableAbi>(&self, name: &str) -> Option<Mapping<T>> {
        let mapping: Option<OpaqueMapping> = unsafe {
            (self.lookup_entry)(
                &*self.store,
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                name.into(),
            )
        }
        .into();
        unsafe { core::mem::transmute(mapping) }
    }

    pub fn entry_list<'a, T: StableAbi>(
        &'a self,
        mut callback: OpaqueCallback<(&'a str, &'a Mapping<T>)>,
    ) {
        let cb = &mut move |data: *const c_void| {
            let CTup2(a, b): &CTup2<CSliceRef<'a, u8>, OpaqueMapping> =
                unsafe { &*(data as *const c_void as *const _) };
            callback.call((unsafe { a.into_str() }, unsafe {
                &*(b as *const OpaqueMapping as *const Mapping<T>)
            }))
        };

        unsafe {
            (self.entry_list)(
                &*self.store,
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                &mut cb.into(),
            )
        };
    }
}

impl PluginStore {
    pub unsafe fn entries_raw(
        &self,
        id: UTypeId,
        layout: &'static TypeLayout,
    ) -> Entry<DashMap<String, OpaqueMapping>> {
        let idx = *self.type_map.entry(id).or_insert_with(|| {
            self.layouts
                .iter()
                .find(|p| check_layout_compatibility(layout, p.value()).is_ok())
                .map(|p| *p.key())
                .or_else(|| {
                    self.entry_list.insert(Default::default()).map(|i| {
                        self.layouts.insert(i, layout);
                        i
                    })
                })
                .expect("Slab is full!")
        });

        self.entry_list.get(idx).unwrap()
    }

    pub fn entries<T: StableAbi>(&self) -> Entry<DashMap<String, Mapping<T>>> {
        let id = T::ABI_CONSTS.type_id.get();
        unsafe { std::mem::transmute(self.entries_raw(id, T::LAYOUT)) }
    }

    pub unsafe fn register_mapping_raw(
        &self,
        id: UTypeId,
        layout: &'static TypeLayout,
        name: &str,
        mapping: OpaqueMapping,
    ) -> bool {
        let entries = self.entries_raw(id, layout);

        let entry = entries.entry(name.to_string());

        if matches!(entry, entry::Entry::Vacant(_)) {
            entry.or_insert(mapping);
            true
        } else {
            false
        }
    }

    pub fn register_mapping<T: StableAbi>(&self, name: &str, mapping: Mapping<T>) -> bool {
        unsafe {
            self.register_mapping_raw(
                T::ABI_CONSTS.type_id.get(),
                T::LAYOUT,
                name,
                mapping.into_opaque(),
            )
        }
    }
}

pub struct Node {
    backend: NodeBackend,
    frontends: Vec<FrontendArcBox<'static>>,
    plugins: CPluginStore,
}

impl Default for Node {
    fn default() -> Self {
        let backend = NodeBackend::default();
        let plugins = CPluginStore::default();

        extern "C" fn self_as_leaf<T: Leaf + Into<LeafBaseBox<'static, T>> + Clone + 'static>(
            obj: &T,
        ) -> COption<LeafBox<'static>> {
            COption::Some(trait_obj!(obj.clone() as Leaf))
        }

        plugins.register_mapping(
            "os",
            Mapping::Leaf(self_as_leaf::<OsInstanceArcBox<'static>>),
        );

        plugins.register_mapping(
            "mem",
            Mapping::Leaf(self_as_leaf::<CArc<ThreadedConnector>>),
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
    fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()> {
        self.backend.list(path, &self.plugins, out)
    }
}

pub struct NodeBackend {
    backends: Slab<BackendBox<'static>>,
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
                .insert(trait_obj!(backend as Backend))
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

        Err(ErrorKind::NotFound.into())
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

    fn open(&self, path: &str, plugins: &CPluginStore) -> Result<usize> {
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

    fn list(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()> {
        if path.is_empty() {
            self.entries
                .iter()
                .map(|r| r.key().clone())
                .map(|n| ListEntry::new(n.into(), true))
                .feed_into_mut(out);

            Ok(())
        } else {
            let (branch, path) = path.split_once("/").unwrap_or((path, ""));
            match self.entries.get(branch) {
                Some(branch) => {
                    let cb = &mut |entry: BranchListEntry| {
                        out.call(ListEntry::new(
                            entry.name.into(),
                            matches!(entry.obj, DirEntry::Branch(_)),
                        ))
                    };

                    branch.list_recurse(path, plugins, &mut cb.into())
                }
                _ => Err(ErrorKind::NotFound.into()),
            }
        }
    }
}

#[cglue_trait]
#[int_result]
pub trait Backend {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, data: CIterator<ReadData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<WriteData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str, plugins: &CPluginStore) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()>;
}

#[cglue_trait]
#[int_result]
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
    fn list(&self, path: &str, out: &mut OpaqueCallback<ListEntry>) -> Result<()>;
}

fn branch_map_entry<T: Branch + StableAbi>(
    branch: &T,
    entry: Mapping<T>,
    remote: Option<&str>,
    plugins: &CPluginStore,
) -> Result<DirEntry> {
    match (remote, entry) {
        (Some(path), Mapping::Branch(map)) => map(branch)
            .as_ref()
            .ok_or::<ErrorKind>(ErrorKind::NotFound)?
            .get_entry(path, plugins),
        (None, Mapping::Branch(map)) => Ok(DirEntry::Branch(
            Option::from(map(branch)).ok_or(ErrorKind::NotFound)?,
        )),
        (None, Mapping::Leaf(map)) => Ok(DirEntry::Leaf(
            Option::from(map(branch)).ok_or(ErrorKind::NotFound)?,
        )),
        _ => Err(ErrorKind::NotFound.into()),
    }
}

fn branch_get_entry<T: Branch + StableAbi>(
    branch: &T,
    path: &str,
    plugins: &CPluginStore,
) -> Result<DirEntry> {
    let (local, remote) = path
        .split_once("/")
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((path, None));

    let entry = plugins
        .lookup_entry::<T>(local)
        .ok_or(ErrorKind::NotFound)?;

    branch_map_entry(branch, entry, remote, plugins)
}

fn branch_list<T: Branch + StableAbi>(
    branch: &T,
    plugins: &CPluginStore,
) -> Result<HashMap<String, DirEntry>> {
    let mut ret = vec![];

    plugins.entry_list::<T>(
        (&mut |(name, entry): (&str, &Mapping<T>)| {
            ret.push(
                branch_map_entry(branch, *entry, None, plugins)
                    .map(|entry| (name.to_string(), entry)),
            );
            true
        })
            .into(),
    );

    ret.into_iter().collect()
}

impl Branch for CArc<ThreadedConnector> {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        branch_get_entry(self, path, plugins)
    }

    fn list(
        &self,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        branch_list(self, plugins)?
            .into_iter()
            .map(|(name, entry)| BranchListEntry::new(name.into(), entry))
            .feed_into_mut(out);
        Ok(())
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
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        branch_get_entry(self, path, plugins)
    }

    fn list(
        &self,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        branch_list(self, plugins)?
            .into_iter()
            .map(|(name, entry)| BranchListEntry::new(name.into(), entry))
            .feed_into_mut(out);
        Ok(())
    }
}

impl Leaf for OsInstanceArcBox<'static> {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(CArc::from(self.clone()), None, None, None))
    }
}

// Internal FS representation. Does not cross backends.

#[repr(C)]
#[derive(StableAbi)]
pub enum DirEntry {
    Branch(BranchBox<'static>),
    Leaf(LeafBox<'static>),
}

#[repr(C)]
#[derive(StableAbi)]
pub struct BranchListEntry {
    pub name: ReprCString,
    pub obj: DirEntry,
}

impl BranchListEntry {
    fn new(name: ReprCString, obj: DirEntry) -> Self {
        Self { name, obj }
    }
}

#[cglue_trait]
#[int_result]
pub trait Branch {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry>;
    fn list(&self, plugins: &CPluginStore, out: &mut OpaqueCallback<BranchListEntry>)
        -> Result<()>;

    fn list_recurse(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        if path.is_empty() {
            self.list(plugins, out)
        } else {
            match self.get_entry(path, plugins) {
                Ok(DirEntry::Branch(mut branch)) => branch.list(plugins, out),
                Ok(_) => Err(ErrorKind::InvalidPath.into()),
                Err(e) => Err(e),
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, StableAbi)]
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

#[cglue_trait]
#[int_result]
pub trait Leaf {
    fn open(&self) -> Result<FileOpsObj<c_void>>;
}
