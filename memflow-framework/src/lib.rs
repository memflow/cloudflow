#[macro_use]
extern crate filer;

use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;
use std::sync::Arc;

#[repr(transparent)]
#[derive(StableAbi, Clone)]
pub struct ProcessListArc(ThreadedOsArc);

impl ProcessListArc {
    extern "C" fn map_into(os: &ThreadedOsArc) -> COption<BranchBox<'static>> {
        COption::Some(trait_obj!(ProcessListArc(os.clone()) as Branch))
    }
}

impl Branch for ProcessListArc {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        let (pid, path) = branch::split_path(path);

        let pid = str::parse(pid).map_err(|_| ErrorKind::InvalidPath)?;

        let proc = self
            .0
            .get_orig()
            .clone()
            .into_process_by_pid(pid)
            .map_err(|_| ErrorKind::NotFound)?;

        let proc = ThreadedProcessArc::from(proc);

        if let Some(path) = path {
            proc.get_entry(path, plugins)
        } else {
            Ok(DirEntry::Branch(trait_obj!(proc as Branch)))
        }
    }

    fn list(
        &self,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        self.0.get().process_info_list_callback(
            (&mut |info: ProcessInfo| {
                if let Ok(proc) = self.0.get_orig().clone().into_process_by_info(info) {
                    let pid = proc.info().pid;
                    let proc = ThreadedProcessArc::from(proc);
                    let entry = DirEntry::Branch(trait_obj!(proc as Branch));
                    out.call(BranchListEntry::new(format!("{}", pid).into(), entry))
                } else {
                    true
                }
            })
                .into(),
        );

        Ok(())
    }
}

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
        .register_mapping("processes", Mapping::Branch(ProcessListArc::map_into));

    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<ThreadedProcessArc>));

    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<ThreadedConnectorArc>));

    node.into()
}

/// Splits the connector/os arguments into parts.
///
/// The parts provided are:
///
/// 1. Parent OS/Connector to chain with.
/// 2. Name of the plugin library.
/// 3. Arguments for the plugin.
///
fn split_args(input: &str) -> (Option<&str>, &str, &str) {
    let input = input.trim();

    let (chain_with, input) = if input.starts_with("-c ") {
        let input = input.strip_prefix("-c").unwrap().trim();

        input
            .split_once(" ")
            .map(|(a, b)| (Some(a), b))
            .unwrap_or((Some(input), ""))
    } else {
        (None, input)
    };

    let (name, args) = input.split_once(":").unwrap_or((input, ""));
    (chain_with, name, args)
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

thread_types!(
    ConnectorInstanceArcBox<'static>,
    ThreadedConnector,
    ThreadedConnectorArc
);

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
            (**self).clone(),
            Some(ThreadedConnector::read),
            Some(ThreadedConnector::write),
            Some(ThreadedConnector::rpc),
        ))
    }
}

impl StrBuild<CArc<Arc<MemflowBackend>>> for ThreadedConnectorArc {
    fn build(input: &str, ctx: &CArc<Arc<MemflowBackend>>) -> Result<ThreadedConnectorArc> {
        let (chain_with, name, args) = split_args(input);

        let ctx = ctx.as_ref().ok_or(ErrorKind::NotFound)?;

        let chain_with = if let Some(cw) = chain_with {
            Some(
                ctx.os
                    .get(cw)
                    .ok_or(ErrorKind::NotFound)?
                    .get_orig()
                    .clone(),
            )
        } else {
            None
        };

        ctx.inventory
            .create_connector(
                name,
                chain_with,
                Some(&str::parse(args).map_err(|_| ErrorKind::InvalidArgument)?),
            )
            .map(|c| ThreadedConnector::from(c).self_arc_up())
            .map_err(|_| ErrorKind::Uninitialized.into())
    }
}

fn memdata_map<A: Into<memflow::types::Address>, B>(
    iter: impl Iterator<Item = CTup2<A, B>>,
) -> impl Iterator<Item = MemData<memflow::types::Address, B>> {
    iter.map(|CTup2(a, b)| MemData(a.into(), b))
}

impl ThreadedConnector {
    extern "C" fn read(&self, data: CIterator<RWData>) -> i32 {
        int_res_wrap! {
            self.get()
                .phys_view()
                .read_raw_iter(
                    (&mut memdata_map(data)).into(),
                    &mut (&mut |_: ReadData| true).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
        }
    }

    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        int_res_wrap! {
            self.get()
                .phys_view()
                .write_raw_iter(
                    (&mut memdata_map(data)).into(),
                    &mut (&mut |_: WriteData| true).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
        }
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
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
            (**self).clone(),
            Some(ThreadedOs::read),
            Some(ThreadedOs::write),
            Some(ThreadedOs::rpc),
        ))
    }
}

thread_types!(OsInstanceArcBox<'static>, ThreadedOs, ThreadedOsArc);

impl StrBuild<CArc<Arc<MemflowBackend>>> for ThreadedOsArc {
    fn build(input: &str, ctx: &CArc<Arc<MemflowBackend>>) -> Result<ThreadedOsArc> {
        let (chain_with, name, args) = split_args(input);

        let ctx = ctx.as_ref().ok_or(ErrorKind::NotFound)?;

        let chain_with = if let Some(cw) = chain_with {
            Some(
                ctx.connector
                    .get(cw)
                    .ok_or(ErrorKind::NotFound)?
                    .get_orig()
                    .clone(),
            )
        } else {
            None
        };

        ctx.inventory
            .create_os(
                name,
                chain_with,
                Some(&str::parse(args).map_err(|_| ErrorKind::InvalidArgument)?),
            )
            .map(|c| ThreadedOs::from(c).self_arc_up())
            .map_err(|_| ErrorKind::Uninitialized.into())
    }
}

impl ThreadedOs {
    extern "C" fn read(&self, data: CIterator<RWData>) -> i32 {
        int_res_wrap! {
            as_mut!(self.get() impl MemoryView)
                .ok_or(Error(ErrorOrigin::Read, ErrorKind::NotImplemented))?
                .read_raw_iter(
                    (&mut memdata_map(data)).into(),
                    &mut (&mut |_: ReadData| true).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
        }
    }

    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        int_res_wrap! {
            as_mut!(self
                .get() impl MemoryView)
                .ok_or(Error(ErrorOrigin::Read, ErrorKind::NotImplemented))?
                .write_raw_iter(
                    (&mut memdata_map(data)).into(),
                    &mut (&mut |_: WriteData| true).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
        }
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}

thread_types!(
    IntoProcessInstanceArcBox<'static>,
    ThreadedProcess,
    ThreadedProcessArc
);

impl Branch for ThreadedProcessArc {
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

impl Leaf for ThreadedProcessArc {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            (**self).clone(),
            Some(ThreadedProcess::read),
            Some(ThreadedProcess::write),
            Some(ThreadedProcess::rpc),
        ))
    }
}

impl ThreadedProcess {
    extern "C" fn read(&self, data: CIterator<RWData>) -> i32 {
        int_res_wrap! {
            self.get()
                .read_raw_iter(
                    (&mut memdata_map(data)).into(),
                    &mut (&mut |_: ReadData| true).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
        }
    }

    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        int_res_wrap! {
            self.get()
                .write_raw_iter(
                    (&mut memdata_map(data)).into(),
                    &mut (&mut |_: WriteData| true).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
        }
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}
