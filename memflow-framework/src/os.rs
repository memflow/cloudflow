use crate::process::{LazyProcessArc, LazyProcessBase};
use crate::util::*;
use crate::MemflowBackend;
use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use dashmap::DashMap;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;
use num::Num;

use std::sync::Arc;

pub extern "C" fn on_node(node: &Node) {
    node.plugins
        .register_mapping("os", Mapping::Leaf(self_as_leaf::<OsRoot>));

    node.plugins
        .register_mapping("processes", Mapping::Branch(ProcessList::map_into));
}

thread_types!(OsInstanceArcBox<'static>, ThreadedOs, ThreadedOsArc);

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct OsRoot {
    os: ThreadedOsArc,
    plist: CArcSome<c_void>,
}

impl core::ops::Deref for OsRoot {
    type Target = ThreadedOsArc;

    fn deref(&self) -> &Self::Target {
        &self.os
    }
}

impl From<ThreadedOsArc> for OsRoot {
    fn from(os: ThreadedOsArc) -> Self {
        Self {
            plist: CArcSome::from(ProcessList::from(os.clone())).into_opaque(),
            os,
        }
    }
}

impl OsRoot {
    unsafe fn plist(&self) -> &CArcSome<ProcessList> {
        (&self.plist as *const CArcSome<c_void> as *const CArcSome<ProcessList>)
            .as_ref()
            .unwrap()
    }
}

impl Branch for OsRoot {
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

impl Leaf for OsRoot {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            (*self.os).clone(),
            Some(ThreadedOs::read),
            Some(ThreadedOs::write),
            Some(ThreadedOs::rpc),
        ))
    }
}

impl StrBuild<CArc<Arc<MemflowBackend>>> for OsRoot {
    fn build(input: &str, ctx: &CArc<Arc<MemflowBackend>>) -> Result<Self> {
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
            .map(Self::from)
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

#[derive(Clone)]
struct ProcessList {
    os: ThreadedOsArc,
    by_pid: PidProcessList,
    by_name: NameProcessList,
}

impl From<ThreadedOsArc> for ProcessList {
    fn from(os: ThreadedOsArc) -> Self {
        Self {
            by_pid: os.clone().into(),
            by_name: os.clone().into(),
            os,
        }
    }
}

impl ProcessList {
    extern "C" fn map_into(os: &OsRoot) -> COption<BranchBox<'static>> {
        COption::Some(trait_obj!(unsafe { &**os.plist() }.clone() as Branch))
    }
}

impl Branch for ProcessList {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        let (entry, path) = branch::split_path(path);

        match entry {
            "by-pid" => branch::forward_entry(self.by_pid.clone(), path, plugins),
            "by-name" => branch::forward_entry(self.by_name.clone(), path, plugins),
            addr => {
                let addr: umem =
                    Num::from_str_radix(addr, 16).map_err(|_| ErrorKind::InvalidPath)?;

                let info = self
                    .os
                    .get()
                    .process_info_by_address(addr.into())
                    .map_err(|_| ErrorKind::NotFound)?;

                let proc = LazyProcessArc::from(LazyProcessBase::new(self.os.clone(), info));

                branch::forward_entry(proc, path, plugins)
            }
        }
    }

    fn list(
        &self,
        _plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        let _ = out.call(BranchListEntry::new(
            "by-pid".into(),
            DirEntry::Branch(trait_obj!(self.by_pid.clone() as Branch)),
        ));
        let _ = out.call(BranchListEntry::new(
            "by-name".into(),
            DirEntry::Branch(trait_obj!(self.by_name.clone() as Branch)),
        ));

        self.os.get().process_info_list_callback(
            (&mut |info: ProcessInfo| {
                let addr = info.address.to_umem();
                let proc = LazyProcessArc::from(LazyProcessBase::new(self.os.clone(), info));
                let entry = DirEntry::Branch(trait_obj!(proc as Branch));
                out.call(BranchListEntry::new(format!("{:x}", addr).into(), entry))
            })
                .into(),
        );

        Ok(())
    }
}

#[derive(Clone)]
struct PidProcessList {
    os: ThreadedOsArc,
    pid_cache: CArcSome<DashMap<Pid, Address>>,
}

impl From<ThreadedOsArc> for PidProcessList {
    fn from(os: ThreadedOsArc) -> Self {
        Self {
            os,
            pid_cache: DashMap::default().into(),
        }
    }
}

impl PidProcessList {
    fn get_info(&self, pid: Pid) -> Result<ProcessInfo> {
        let info = if let Some(addr) = self.pid_cache.get(&pid) {
            let info = self
                .os
                .get()
                .process_info_by_address(*addr)
                .map_err(|_| ErrorKind::NotFound)?;

            if info.pid == pid {
                Some(info)
            } else {
                None
            }
        } else {
            None
        };

        info.map(|i| Ok(i)).unwrap_or_else(|| {
            self.os
                .get()
                .process_info_by_pid(pid)
                .map_err(|_| ErrorKind::NotFound.into())
                .map(|info| {
                    self.pid_cache.insert(pid, info.address);
                    info
                })
        })
    }
}

impl Branch for PidProcessList {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        let (pid, path) = branch::split_path(path);

        let pid: Pid = str::parse(pid).map_err(|_| ErrorKind::InvalidPath)?;

        let info = self.get_info(pid)?;

        let proc = LazyProcessArc::from(LazyProcessBase::new(self.os.clone(), info));

        if let Some(path) = path {
            proc.get_entry(path, plugins)
        } else {
            Ok(DirEntry::Branch(trait_obj!(proc as Branch)))
        }
    }

    fn list(&self, _: &CPluginStore, out: &mut OpaqueCallback<BranchListEntry>) -> Result<()> {
        self.pid_cache.clear();
        self.os
            .get()
            .process_info_list_callback(
                (&mut |info: ProcessInfo| {
                    let pid = info.pid;
                    if self.pid_cache.insert(pid, info.address).is_none() {
                        let proc =
                            LazyProcessArc::from(LazyProcessBase::new(self.os.clone(), info));
                        let entry = DirEntry::Branch(trait_obj!(proc as Branch));
                        out.call(BranchListEntry::new(format!("{}", pid).into(), entry))
                    } else {
                        true
                    }
                })
                    .into(),
            )
            .map_err(|_| ErrorKind::Unknown)?;

        Ok(())
    }
}

#[derive(Clone)]
struct NameProcessList {
    os: ThreadedOsArc,
    name_cache: CArcSome<DashMap<String, Address>>,
}

impl From<ThreadedOsArc> for NameProcessList {
    fn from(os: ThreadedOsArc) -> Self {
        Self {
            os,
            name_cache: DashMap::default().into(),
        }
    }
}

impl NameProcessList {
    fn get_info(&self, name: &str) -> Result<ProcessInfo> {
        let info = if let Some(addr) = self.name_cache.get(name) {
            let info = self
                .os
                .get()
                .process_info_by_address(*addr)
                .map_err(|_| ErrorKind::NotFound)?;

            if &*info.name == name {
                Some(info)
            } else {
                None
            }
        } else {
            None
        };

        info.map(|i| Ok(i)).unwrap_or_else(|| {
            self.os
                .get()
                .process_info_by_name(name)
                .map_err(|_| ErrorKind::NotFound.into())
                .map(|info| {
                    self.name_cache.insert(name.into(), info.address);
                    info
                })
        })
    }
}

impl Branch for NameProcessList {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        let (name, path) = branch::split_path(path);

        let info = self.get_info(name)?;

        let proc = LazyProcessArc::from(LazyProcessBase::new(self.os.clone(), info));

        if let Some(path) = path {
            proc.get_entry(path, plugins)
        } else {
            Ok(DirEntry::Branch(trait_obj!(proc as Branch)))
        }
    }

    fn list(&self, _: &CPluginStore, out: &mut OpaqueCallback<BranchListEntry>) -> Result<()> {
        self.name_cache.clear();
        self.os
            .get()
            .process_info_list_callback(
                (&mut |info: ProcessInfo| {
                    let name = info.name.to_string();
                    if self.name_cache.insert(name.clone(), info.address).is_none() {
                        let proc =
                            LazyProcessArc::from(LazyProcessBase::new(self.os.clone(), info));
                        let entry = DirEntry::Branch(trait_obj!(proc as Branch));
                        out.call(BranchListEntry::new(format!("{}", name).into(), entry))
                    } else {
                        true
                    }
                })
                    .into(),
            )
            .map_err(|_| ErrorKind::Unknown)?;

        Ok(())
    }
}
