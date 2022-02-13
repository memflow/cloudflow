use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;

pub fn create_node() -> CArcSome<Node> {
    let backend = NodeBackend::default();

    MemflowBackend::to_node(&backend);

    extern "C" fn self_as_leaf<T: Leaf + Into<LeafBaseBox<'static, T>> + Clone + 'static>(
        obj: &T,
    ) -> COption<LeafBox<'static>> {
        COption::Some(trait_obj!(obj.clone() as Leaf))
    }

    let node = Node::new(backend);

    node.plugins
        .register_mapping("os", Mapping::Leaf(self_as_leaf::<ThreadedOsArc>));

    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<ThreadedConnectorArc>));

    node.into()
}

impl Branch for ThreadedConnectorArc {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        branch::get_entry(self, path, plugins)
    }

    fn list(
        &self,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        branch::list(self, plugins)?
            .into_iter()
            .map(|(name, entry)| BranchListEntry::new(name.into(), entry))
            .feed_into_mut(out);
        Ok(())
    }
}

impl Leaf for ThreadedConnectorArc {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            self.0.clone(),
            Some(ThreadedConnector::read),
            Some(ThreadedConnector::write),
            Some(ThreadedConnector::rpc),
        ))
    }
}

use std::sync::{Arc, Weak};

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
        println!("ADDED!!");
    }

    pub fn to_node(backend: &NodeBackend) {
        Self::new_arc().add_to_node(backend)
    }
}

#[derive(StableAbi)]
#[repr(transparent)]
pub struct ThreadedConnector(ThreadCtx<ConnectorInstanceArcBox<'static>>);

#[derive(Clone, StableAbi)]
#[repr(transparent)]
pub struct ThreadedConnectorArc(CArcSome<ThreadedConnector>);

impl StrBuild<CArc<Arc<MemflowBackend>>> for ThreadedConnectorArc {
    fn build(input: &str, ctx: &CArc<Arc<MemflowBackend>>) -> Result<ThreadedConnectorArc> {
        let (name, args) = input.split_once(":").unwrap_or((input, ""));
        ctx.as_ref()
            .ok_or(ErrorKind::NotFound)?
            .inventory
            .create_connector(
                name,
                None,
                Some(&str::parse(args).map_err(|_| ErrorKind::InvalidArgument)?),
            )
            .map(|c| ThreadedConnector::from(c).self_arc_up())
            .map_err(|e| {
                println!("{}", input);
                e
            })
            .map_err(|_| ErrorKind::Uninitialized.into())
    }
}

impl From<ThreadedConnectorArc> for CArcSome<ThreadedConnector> {
    fn from(ThreadedConnectorArc(arc): ThreadedConnectorArc) -> Self {
        arc
    }
}

impl From<CArcSome<ThreadedConnector>> for ThreadedConnectorArc {
    fn from(arc: CArcSome<ThreadedConnector>) -> Self {
        ThreadedConnectorArc(arc)
    }
}

fn memdata_map<A: Into<memflow::types::Address>, B>(
    iter: impl Iterator<Item = CTup2<A, B>>,
) -> impl Iterator<Item = MemData<memflow::types::Address, B>> {
    iter.map(|CTup2(a, b)| MemData(a.into(), b))
}

impl ThreadedConnector {
    fn read2(&self, data: CIterator<RWData>) -> Result<()> {
        self.0
            .get()
            .phys_view()
            .read_raw_iter(
                (&mut memdata_map(data)).into(),
                &mut (&mut |_: ReadData| true).into(),
            )
            .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
    }

    extern "C" fn read(&self, data: CIterator<RWData>) -> i32 {
        self.read2(data).into_int_result()
    }

    fn write2(&self, data: CIterator<ROData>) -> Result<()> {
        self.0
            .get()
            .phys_view()
            .write_raw_iter(
                (&mut memdata_map(data)).into(),
                &mut (&mut |_: WriteData| true).into(),
            )
            .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
    }

    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        self.write2(data).into_int_result()
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}

impl From<ConnectorInstanceArcBox<'static>> for ThreadedConnector {
    fn from(conn: ConnectorInstanceArcBox<'static>) -> Self {
        Self(ThreadCtx::new(conn, 32))
    }
}

impl Branch for ThreadedOsArc {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        branch::get_entry(self, path, plugins)
    }

    fn list(
        &self,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        branch::list(self, plugins)?
            .into_iter()
            .map(|(name, entry)| BranchListEntry::new(name.into(), entry))
            .feed_into_mut(out);
        Ok(())
    }
}

impl Leaf for ThreadedOsArc {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            self.0.clone(),
            Some(ThreadedOs::read),
            Some(ThreadedOs::write),
            Some(ThreadedOs::rpc),
        ))
    }
}

#[derive(StableAbi)]
#[repr(transparent)]
pub struct ThreadedOs(ThreadCtx<OsInstanceArcBox<'static>>);

impl From<OsInstanceArcBox<'static>> for ThreadedOs {
    fn from(os: OsInstanceArcBox<'static>) -> Self {
        Self(ThreadCtx::new(os, 32))
    }
}

#[derive(Clone, StableAbi)]
#[repr(transparent)]
pub struct ThreadedOsArc(CArcSome<ThreadedOs>);

impl StrBuild<CArc<Arc<MemflowBackend>>> for ThreadedOsArc {
    fn build(input: &str, ctx: &CArc<Arc<MemflowBackend>>) -> Result<ThreadedOsArc> {
        let (name, args) = input.split_once(":").unwrap_or((input, ""));
        ctx.as_ref()
            .ok_or(ErrorKind::NotFound)?
            .inventory
            .create_os(
                name,
                None,
                Some(&str::parse(args).map_err(|_| ErrorKind::InvalidArgument)?),
            )
            .map(|c| ThreadedOs::from(c).self_arc_up())
            .map_err(|_| ErrorKind::Uninitialized.into())
    }
}

impl ArcType for ThreadedOs {
    type ArcSelf = ThreadedOsArc;
}

impl ArcType for ThreadedConnector {
    type ArcSelf = ThreadedConnectorArc;
}

impl From<ThreadedOsArc> for CArcSome<ThreadedOs> {
    fn from(ThreadedOsArc(arc): ThreadedOsArc) -> Self {
        arc
    }
}

impl From<CArcSome<ThreadedOs>> for ThreadedOsArc {
    fn from(arc: CArcSome<ThreadedOs>) -> Self {
        ThreadedOsArc(arc)
    }
}

impl ThreadedOs {
    fn read2(&self, data: CIterator<RWData>) -> Result<()> {
        as_mut!(self.0.get() impl MemoryView)
            .ok_or(Error(ErrorOrigin::Read, ErrorKind::NotImplemented))?
            .read_raw_iter(
                (&mut memdata_map(data)).into(),
                &mut (&mut |_: ReadData| true).into(),
            )
            .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
    }

    extern "C" fn read(&self, data: CIterator<RWData>) -> i32 {
        self.read2(data).into_int_result()
    }

    fn write2(&self, data: CIterator<ROData>) -> Result<()> {
        as_mut!(self.0
            .get() impl MemoryView)
        .ok_or(Error(ErrorOrigin::Read, ErrorKind::NotImplemented))?
        .write_raw_iter(
            (&mut memdata_map(data)).into(),
            &mut (&mut |_: WriteData| true).into(),
        )
        .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
    }

    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        self.write2(data).into_int_result()
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}
