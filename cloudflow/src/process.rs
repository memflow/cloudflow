use crate::module::{ModuleArc, ModuleBase};
use crate::os::OsBase;
use crate::util::*;
use crate::MemflowBackend;
use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use dashmap::DashMap;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
use memflow::prelude::v1::*;
use num::Num;

use std::sync::Arc;

use once_cell::sync::OnceCell;

pub extern "C" fn on_node(node: &Node, ctx: CArc<c_void>) {
    node.plugins.register_mapping(
        "mem",
        Mapping::Leaf(self_as_leaf::<LazyProcessArc>, ctx.clone()),
    );

    node.plugins
        .register_mapping("info", Mapping::Leaf(map_into_info, ctx.clone()));

    node.plugins
        .register_mapping("maps", Mapping::Leaf(map_into_maps, ctx.clone()));

    node.plugins
        .register_mapping("phys_maps", Mapping::Leaf(map_into_phys_maps, ctx.clone()));

    node.plugins
        .register_mapping("modules", Mapping::Branch(ModuleList::map_into, ctx));
}

thread_types!(
    IntoProcessInstanceArcBox<'static>,
    ThreadedProcess,
    ThreadedProcessArc
);

arc_types!(LazyProcessBase, LazyProcess, LazyProcessArc);

impl Branch for LazyProcessArc {
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

impl Leaf for LazyProcessArc {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            self.proc().ok_or(ErrorKind::Uninitialized)?.clone().into(),
            Some(ThreadedProcess::read),
            Some(ThreadedProcess::write),
            Some(ThreadedProcess::rpc),
        ))
    }

    fn metadata(&self) -> Result<NodeMetadata> {
        Ok(NodeMetadata {
            is_branch: false,
            has_read: true,
            has_write: true,
            has_rpc: true,
            // TODO: Proc vs Sys arch?
            size: (1 as Size) << self.proc_info.sys_arch.into_obj().address_space_bits(),
            ..Default::default()
        })
    }
}

impl ThreadedProcess {
    pub(crate) extern "C" fn read(&self, data: VecOps<RWData>) -> i32 {
        int_res_wrap! {
            memdata_map(data, |data| {
                self.get()
                    .read_raw_iter(data)
                    .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
            })
        }
    }

    pub(crate) extern "C" fn write(&self, data: VecOps<ROData>) -> i32 {
        int_res_wrap! {
            memdata_map(data, |data| {
                self.get()
                    .write_raw_iter(data)
                    .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
            })
        }
    }

    pub(crate) extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}

#[repr(C)]
#[derive(StableAbi, Clone)]
pub struct LazyProcessBase {
    os: OsBase,
    proc_info: ProcessInfo,
    proc_box: CArcSome<c_void>,
    get_proc: unsafe extern "C" fn(&LazyProcessBase) -> Option<&ThreadedProcessArc>,
}

impl LazyProcessBase {
    unsafe extern "C" fn get_proc(&self) -> Option<&ThreadedProcessArc> {
        let proc_box = &*self.proc_box as *const c_void as *const OnceCell<ThreadedProcessArc>;

        (*proc_box)
            .get_or_try_init(|| {
                self.os
                    .get_orig()
                    .clone()
                    .into_process_by_info(self.proc_info.clone())
                    .map(ThreadedProcessArc::from)
            })
            .ok()
    }

    pub fn proc(&self) -> Option<&ThreadedProcessArc> {
        unsafe { (self.get_proc)(self) }
    }

    pub fn new(os: OsBase, proc_info: ProcessInfo) -> Self {
        Self {
            os,
            proc_info,
            proc_box: CArcSome::from(OnceCell::<ThreadedProcessArc>::new()).into_opaque(),
            get_proc: Self::get_proc,
        }
    }
}

fn format_perms(page_type: PageType) -> String {
    format!(
        "r{}{}",
        if page_type.contains(PageType::WRITEABLE) {
            'w'
        } else {
            '-'
        },
        if !page_type.contains(PageType::NOEXEC) {
            'x'
        } else {
            '-'
        }
    )
}

extern "C" fn map_into_info(
    proc: &LazyProcessArc,
    ctx: &CArc<c_void>,
) -> COption<LeafArcBox<'static>> {
    let file = FnFile::new(proc.clone(), |proc| {
        let proc = proc.proc().ok_or(ErrorKind::Uninitialized)?;
        let info = proc.get_orig().info();
        Ok(format!("{:#?}", info))
    });
    COption::Some(trait_obj!((file, ctx.clone()) as Leaf))
}

extern "C" fn map_into_maps(
    proc: &LazyProcessArc,
    ctx: &CArc<c_void>,
) -> COption<LeafArcBox<'static>> {
    let file = FnFile::new(proc.clone(), |proc| {
        let proc = proc.proc().ok_or(ErrorKind::Uninitialized)?;
        let mut proc = proc.get();

        let maps = proc.mapped_mem_vec(-1);
        let mut modules = proc.module_list().map_err(|_| ErrorKind::Uninitialized)?;
        modules.sort_by_key(|m| m.base.to_umem());

        let out = maps
            .into_iter()
            .map(|CTup3(vaddr, size, page_type)| {
                let module = modules
                    .iter()
                    .find(|m| m.base <= vaddr && m.base + m.size > vaddr);

                let perms = format_perms(page_type);

                format!(
                    "{:x}-{:x} {} {}\n",
                    vaddr,
                    vaddr + size,
                    perms,
                    module.map(|m| m.name.as_ref()).unwrap_or_default()
                )
            })
            .collect::<String>();

        Ok(out)
    });
    COption::Some(trait_obj!((file, ctx.clone()) as Leaf))
}

extern "C" fn map_into_phys_maps(
    proc: &LazyProcessArc,
    ctx: &CArc<c_void>,
) -> COption<LeafArcBox<'static>> {
    if proc
        .proc()
        .and_then(|proc| as_ref!(proc.get_orig() impl VirtualTranslate))
        .is_some()
    {
        let file = FnFile::new(proc.clone(), |proc| {
            let proc = proc.proc().ok_or(ErrorKind::Uninitialized)?;
            let mut proc = proc.get();
            let proc = as_mut!(proc impl VirtualTranslate).ok_or(ErrorKind::NotSupported)?;

            let maps = proc.virt_translation_map_vec();
            let mut modules = proc.module_list().map_err(|_| ErrorKind::Unknown)?;
            modules.sort_by_key(|m| m.base.to_umem());

            let out = maps
                .into_iter()
                .map(|tr| {
                    let module = modules
                        .iter()
                        .find(|m| m.base <= tr.in_virtual && m.base + m.size > tr.in_virtual);

                    let perms = format_perms(tr.out_physical.page_type());

                    format!(
                        "{:x}-{:x} {} {:9x} {}\n",
                        tr.in_virtual,
                        tr.in_virtual + tr.size,
                        perms,
                        tr.out_physical.address,
                        module.map(|m| m.name.as_ref()).unwrap_or_default()
                    )
                })
                .collect::<String>();

            Ok(out)
        });
        COption::Some(trait_obj!((file, ctx.clone()) as Leaf))
    } else {
        COption::None
    }
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct LazyProcessRoot {
    process: LazyProcessBase,
    mlist: CArcSome<c_void>,
}

impl core::ops::Deref for LazyProcessRoot {
    type Target = LazyProcessBase;

    fn deref(&self) -> &Self::Target {
        &self.process
    }
}

impl From<LazyProcessBase> for LazyProcessRoot {
    fn from(process: LazyProcessBase) -> Self {
        Self {
            mlist: CArcSome::from(ModuleList::from(process.clone())).into_opaque(),
            process,
        }
    }
}

impl LazyProcessRoot {
    unsafe fn mlist(&self) -> &CArcSome<ModuleList> {
        (&self.mlist as *const CArcSome<c_void> as *const CArcSome<ModuleList>)
            .as_ref()
            .unwrap()
    }
}

impl Branch for LazyProcessRoot {
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

impl Leaf for LazyProcessRoot {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            self.proc().ok_or(ErrorKind::Uninitialized)?.clone().into(),
            Some(ThreadedProcess::read),
            Some(ThreadedProcess::write),
            Some(ThreadedProcess::rpc),
        ))
    }

    fn metadata(&self) -> Result<NodeMetadata> {
        Ok(NodeMetadata {
            is_branch: false,
            has_read: true,
            has_write: true,
            has_rpc: true,
            size: (1 as Size)
                << self
                    .os
                    .get_orig()
                    .info()
                    .arch
                    .into_obj()
                    .address_space_bits(),
            ..Default::default()
        })
    }
}

#[derive(Clone)]
struct ModuleList {
    process: LazyProcessBase,
    by_sys_arch: ModuleArchList,
    by_proc_arch: Option<ModuleArchList>,
}

impl From<LazyProcessBase> for ModuleList {
    fn from(process: LazyProcessBase) -> Self {
        let sys_arch = process.os.get().info().arch;
        let by_sys_arch = (process.clone(), sys_arch).into();

        let by_proc_arch = if let Some(proc) = process.proc() {
            let proc = proc.get();
            let info = proc.info();
            if info.proc_arch != info.sys_arch {
                Some((process.clone(), info.proc_arch).into())
            } else {
                None
            }
        } else {
            None
        };

        Self {
            process,
            by_sys_arch,
            by_proc_arch,
        }
    }
}

impl ModuleList {
    extern "C" fn map_into(
        process: &LazyProcessArc,
        ctx: &CArc<c_void>,
    ) -> COption<BranchArcBox<'static>> {
        // TODO: improve this workaround
        let process: LazyProcessRoot = LazyProcessBase::clone(process).into();
        COption::Some(trait_obj!(
            (unsafe { &**process.mlist() }.clone(), ctx.clone()) as Branch
        ))
    }
}

impl Branch for ModuleList {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        let (entry, path) = branch::split_path(path);

        if entry == self.by_sys_arch.arch.to_string() {
            return branch::forward_entry(
                self.by_sys_arch.clone(),
                self.process.os.ctx.clone(),
                path,
                plugins,
            );
        }

        if let Some(by_proc_arch) = &self.by_proc_arch {
            if entry == by_proc_arch.arch.to_string() {
                return branch::forward_entry(
                    by_proc_arch.clone(),
                    self.process.os.ctx.clone(),
                    path,
                    plugins,
                );
            }
        }

        Err(Error(ErrorOrigin::Branch, ErrorKind::NotFound))
    }

    fn list(
        &self,
        _plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        // display system architecture subfolder
        let _ = out.call(BranchListEntry::new(
            self.by_sys_arch.arch.to_string().into(),
            DirEntry::Branch(trait_obj!(
                (self.by_sys_arch.clone(), self.process.os.ctx.clone()) as Branch
            )),
        ));

        // display process architecture subfolder only if it differs from system architecture
        if let Some(by_proc_arch) = &self.by_proc_arch {
            let _ = out.call(BranchListEntry::new(
                by_proc_arch.arch.to_string().into(),
                DirEntry::Branch(trait_obj!(
                    (by_proc_arch.clone(), self.process.os.ctx.clone()) as Branch
                )),
            ));
        }

        Ok(())
    }
}

#[derive(Clone)]
struct ModuleArchList {
    process: LazyProcessBase,
    arch: ArchitectureIdent,
    name_cache: CArcSome<DashMap<String, ModuleInfo>>,
}

impl From<(LazyProcessBase, ArchitectureIdent)> for ModuleArchList {
    fn from((process, arch): (LazyProcessBase, ArchitectureIdent)) -> Self {
        Self {
            process,
            arch,
            name_cache: DashMap::default().into(),
        }
    }
}

impl ModuleArchList {
    pub fn find_module_by_name(&self, name: &str) -> Option<ModuleInfo> {
        let info = self.name_cache.get(name);
        if let Some(info) = info {
            Some(info.clone())
        } else {
            let proc = self.process.proc()?;
            let info = proc.get().module_by_name(name).ok()?;
            self.name_cache.insert(name.to_string(), info.clone());
            Some(info)
        }
    }
}

impl Branch for ModuleArchList {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry> {
        let (name, path) = branch::split_path(path);

        let info = self.find_module_by_name(name).ok_or(ErrorKind::NotFound)?;

        let proc = self.process.proc().ok_or(ErrorKind::Unknown)?;
        let module = ModuleArc::from(ModuleBase::new(proc.clone(), info.clone()));

        if let Some(path) = path {
            module.get_entry(path, plugins)
        } else {
            Ok(DirEntry::Branch(trait_obj!(
                (module, self.process.os.ctx.clone()) as Branch
            )))
        }
    }

    fn list(
        &self,
        _plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        self.name_cache.clear();

        let proc = self.process.proc().ok_or(ErrorKind::Unknown)?;
        proc.get()
            .module_list_callback(
                Some(&self.arch),
                (&mut |info: ModuleInfo| {
                    let name = info.name.to_string();
                    if self.name_cache.insert(name.clone(), info.clone()).is_none() {
                        let module = ModuleArc::from(ModuleBase::new(proc.clone(), info));
                        let entry =
                            DirEntry::Branch(trait_obj!(
                                (module, self.process.os.ctx.clone()) as Branch
                            ));
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
