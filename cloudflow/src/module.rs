use crate::process::ThreadedProcessArc;
use crate::util::*;
use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
use memflow::prelude::v1::*;

pub extern "C" fn on_node(node: &Node, ctx: CArc<c_void>) {
    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<ModuleArc>, ctx.clone()));

    node.plugins
        .register_mapping("info", Mapping::Leaf(map_into_info, ctx.clone()));
}

arc_types!(ModuleBase, Module, ModuleArc);

impl Branch for ModuleArc {
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

impl Leaf for ModuleArc {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            ModuleBase::clone(self).into(),
            Some(ModuleBase::read),
            Some(ModuleBase::write),
            Some(ModuleBase::rpc),
        ))
    }

    fn metadata(&self) -> Result<NodeMetadata> {
        Ok(NodeMetadata {
            is_branch: false,
            has_read: true,
            has_write: true,
            has_rpc: true,
            size: self.0.module_info.size,
            ..Default::default()
        })
    }
}

#[repr(C)]
#[derive(StableAbi, Clone)]
pub struct ModuleBase {
    process: ThreadedProcessArc,
    module_info: ModuleInfo,
}
use std::cell::RefCell;
impl ModuleBase {
    pub fn new(process: ThreadedProcessArc, module_info: ModuleInfo) -> Self {
        Self {
            process,
            module_info,
        }
    }

    extern "C" fn read(&self, data: VecOps<RWData>) -> i32 {
        // TODO: size constraint
        int_res_wrap! {
            memdata_map(data, |data| {
                let base = self.module_info.base;
                let size = self.module_info.size;

                // wrap the data.inp iterator by first splitting it up at the modules max size
                // and then remapping the desired addr and meta_addr by the module base
                let inp = data.inp.flat_map(|CTup3(addr, meta_addr, data)| {
                    let split = size - addr.to_umem();
                    let (left, right) = data.split_at(split);

                    let new_addr = base + addr.to_umem();
                    let new_meta_addr = base + meta_addr.to_umem();
                    let left = left.map(|out| CTup3(new_addr, new_meta_addr, out));
                    let right = right.map(|out| CTup3(new_addr + split, new_meta_addr + split, out));

                    left.into_iter().chain(right.into_iter())
                });

                // TODO: handle non existent data.out / data.out_fail callbacks

                // wrap the data.out and data.out_fail callbacks:
                // first we check if the desired address falls inside or outside the module and call the appropiate callback.
                // second we subtract the module base that was added in the above function again.
                let out_cell = RefCell::new(data.out.unwrap());
                let out_fail_cell = RefCell::new(data.out_fail.unwrap());

                let out = &mut |CTup2(addr, out)| {
                    if addr < base + size {
                        let mut out_cb = out_cell.borrow_mut();
                        out_cb.call(CTup2(addr - base.to_umem(), out))
                    } else {
                        let mut out_fail_cb = out_fail_cell.borrow_mut();
                        out_fail_cb.call(CTup2(addr - base.to_umem(), out))
                    }
                };
                let out = &mut out.into();
                let out = Some(out);

                let out_fail = &mut |CTup2(addr, out)| {
                    if addr < base + size {
                        let mut out_cb = out_cell.borrow_mut();
                        out_cb.call(CTup2(addr - base.to_umem(), out))
                    } else {
                        let mut out_fail_cb = out_fail_cell.borrow_mut();
                        out_fail_cb.call(CTup2(addr - base.to_umem(), out))
                    }
                };
                let out_fail = &mut out_fail.into();
                let out_fail = Some(out_fail);

                // create a new MemOps object with the wrapped values
                MemOps::with_raw(inp, out, out_fail, |data| {
                    self.process.get()
                        .read_raw_iter(data)
                        .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
                    })
            })
        }
    }

    extern "C" fn write(&self, data: VecOps<ROData>) -> i32 {
        // TODO: size constraint
        int_res_wrap! {
            memdata_map(data, |data| {
                let base = self.module_info.base;
                let size = self.module_info.size;

                // wrap the data.inp iterator by first splitting it up at the modules max size
                // and then remapping the desired addr and meta_addr by the module base
                let inp = data.inp.flat_map(|CTup3(addr, meta_addr, data)| {
                    let split = size - addr.to_umem();
                    let (left, right) = data.split_at(split);

                    let new_addr = base + addr.to_umem();
                    let new_meta_addr = base + meta_addr.to_umem();
                    let left = left.map(|out| CTup3(new_addr, new_meta_addr, out));
                    let right = right.map(|out| CTup3(new_addr + split, new_meta_addr + split, out));

                    left.into_iter().chain(right.into_iter())
                });

                // TODO: handle non existent data.out / data.out_fail callbacks

                // wrap the data.out and data.out_fail callbacks:
                // first we check if the desired address falls inside or outside the module and call the appropiate callback.
                // second we subtract the module base that was added in the above function again.
                let out_cell = RefCell::new(data.out.unwrap());
                let out_fail_cell = RefCell::new(data.out_fail.unwrap());

                let out = &mut |CTup2(addr, out)| {
                    if addr < base + size {
                        let mut out_cb = out_cell.borrow_mut();
                        out_cb.call(CTup2(addr - base.to_umem(), out))
                    } else {
                        let mut out_fail_cb = out_fail_cell.borrow_mut();
                        out_fail_cb.call(CTup2(addr - base.to_umem(), out))
                    }
                };
                let out = &mut out.into();
                let out = Some(out);

                let out_fail = &mut |CTup2(addr, out)| {
                    if addr < base + size {
                        let mut out_cb = out_cell.borrow_mut();
                        out_cb.call(CTup2(addr - base.to_umem(), out))
                    } else {
                        let mut out_fail_cb = out_fail_cell.borrow_mut();
                        out_fail_cb.call(CTup2(addr - base.to_umem(), out))
                    }
                };
                let out_fail = &mut out_fail.into();
                let out_fail = Some(out_fail);

                // create a new MemOps object with the wrapped values
                MemOps::with_raw(inp, out, out_fail, |data| {
                    self.process.get()
                        .write_raw_iter(data)
                        .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
                    })
            })
        }
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}

extern "C" fn map_into_info(
    module: &ModuleArc,
    ctx: &CArc<c_void>,
) -> COption<LeafArcBox<'static>> {
    let file = FnFile::new(module.module_info.clone(), |module_info| {
        Ok(format!("{:#?}", module_info))
    });
    COption::Some(trait_obj!((file, ctx.clone()) as Leaf))
}
