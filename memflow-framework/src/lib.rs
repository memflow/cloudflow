use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;

pub fn create_node() -> Node {
    let node = Node::default();

    let connector = LocalBackend::<ThreadedConnectorArc>::default();

    let inventory = Inventory::scan();

    let kcore = inventory.create_connector("kcore", None, None).unwrap();
    connector.insert("kcore", ThreadedConnector::from(kcore).self_arc_up());

    node.backend.add_backend("connector", connector);

    let os = LocalBackend::<ThreadedOsArc>::default();

    let native = inventory.create_os("native", None, None).unwrap();
    os.insert("native", ThreadedOs::from(native).self_arc_up());

    node.backend.add_backend("os", os);

    extern "C" fn self_as_leaf<T: Leaf + Into<LeafBaseBox<'static, T>> + Clone + 'static>(
        obj: &T,
    ) -> COption<LeafBox<'static>> {
        COption::Some(trait_obj!(obj.clone() as Leaf))
    }

    node.plugins
        .register_mapping("os", Mapping::Leaf(self_as_leaf::<ThreadedOsArc>));

    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<ThreadedConnectorArc>));

    node
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

#[derive(StableAbi)]
#[repr(transparent)]
pub struct ThreadedConnector(ThreadCtx<ConnectorInstanceArcBox<'static>>);

#[derive(Clone, StableAbi)]
#[repr(transparent)]
pub struct ThreadedConnectorArc(CArc<ThreadedConnector>);

impl From<ThreadedConnectorArc> for CArc<ThreadedConnector> {
    fn from(ThreadedConnectorArc(arc): ThreadedConnectorArc) -> Self {
        arc
    }
}

impl From<CArc<ThreadedConnector>> for ThreadedConnectorArc {
    fn from(arc: CArc<ThreadedConnector>) -> Self {
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
pub struct ThreadedOsArc(CArc<ThreadedOs>);

impl ArcType for ThreadedOs {
    type ArcSelf = ThreadedOsArc;
}

impl ArcType for ThreadedConnector {
    type ArcSelf = ThreadedConnectorArc;
}

impl From<ThreadedOsArc> for CArc<ThreadedOs> {
    fn from(ThreadedOsArc(arc): ThreadedOsArc) -> Self {
        arc
    }
}

impl From<CArc<ThreadedOs>> for ThreadedOsArc {
    fn from(arc: CArc<ThreadedOs>) -> Self {
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
