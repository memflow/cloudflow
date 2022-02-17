use crate::os::ThreadedOsArc;
use crate::util::*;
use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;

use once_cell::sync::OnceCell;

pub extern "C" fn on_node(node: &Node) {
    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<LazyProcessArc>));

    node.plugins
        .register_mapping("maps", Mapping::Leaf(map_into_maps));

    node.plugins
        .register_mapping("phys_maps", Mapping::Leaf(map_into_phys_maps));
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
}

impl ThreadedProcess {
    extern "C" fn read(&self, data: VecOps<RWData>) -> i32 {
        int_res_wrap! {
            self.get()
                .read_raw_iter(
                    (&mut memdata_map(data.inp)).into(),
                    &mut (&mut memdata_unmap(data.out_fail)).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
        }
    }

    extern "C" fn write(&self, data: VecOps<ROData>) -> i32 {
        int_res_wrap! {
            self.get()
                .write_raw_iter(
                    (&mut memdata_map(data.inp)).into(),
                    &mut (&mut memdata_unmap(data.out_fail)).into(),
                )
                .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
        }
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}

#[repr(C)]
#[derive(StableAbi, Clone)]
pub struct LazyProcessBase {
    os: ThreadedOsArc,
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

    pub fn new(os: ThreadedOsArc, proc_info: ProcessInfo) -> Self {
        Self {
            os,
            proc_info,
            proc_box: CArcSome::from(OnceCell::<ThreadedProcessArc>::new()).into_opaque(),
            get_proc: Self::get_proc,
        }
    }
}

extern "C" fn map_into_maps(proc: &LazyProcessArc) -> COption<LeafBox<'static>> {
    let file = FnFile::new(proc.clone(), |proc| {
        let proc = proc.proc().ok_or(ErrorKind::Uninitialized)?;
        let mut proc = proc.get();

        let maps = proc.mapped_mem_vec(-1);
        let mut modules = proc.module_list().map_err(|_| ErrorKind::Uninitialized)?;
        modules.sort_by_key(|m| m.base.to_umem());

        let out = maps
            .into_iter()
            .map(|MemData(vaddr, size)| {
                let module = modules
                    .iter()
                    .find(|m| m.base <= vaddr && m.base + m.size > vaddr);

                /*let perms = format!(
                    "r{}{}",
                    if paddr.page_type().contains(PageType::WRITEABLE) {
                        'w'
                    } else {
                        '-'
                    },
                    if !paddr.page_type().contains(PageType::NOEXEC) {
                        'x'
                    } else {
                        '-'
                    }
                );*/

                // TODO: Add perms to memflow
                let perms = "r??";

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
    COption::Some(trait_obj!(file as Leaf))
}

extern "C" fn map_into_phys_maps(proc: &LazyProcessArc) -> COption<LeafBox<'static>> {
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

                    let perms = format!(
                        "r{}{}",
                        if tr.out_physical.page_type().contains(PageType::WRITEABLE) {
                            'w'
                        } else {
                            '-'
                        },
                        if !tr.out_physical.page_type().contains(PageType::NOEXEC) {
                            'x'
                        } else {
                            '-'
                        }
                    );

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
        COption::Some(trait_obj!(file as Leaf))
    } else {
        COption::None
    }
}
