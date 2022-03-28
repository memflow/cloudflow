use crate::os::OsBase;
use crate::process::{ThreadedProcess, ThreadedProcessArc};
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
            memdata_map(data, |mut data| {
                // wrap inp, out, out_fail to add/subtract the base offset
                let inp1 = data.inp.map(|CTup3(addr, meta_addr, data)| CTup3(self.module_info.base + addr.to_umem(), self.module_info.base + meta_addr.to_umem(), data));

                let mut out1 = data.out
                    .as_mut()
                    .map(|of| move |data: CTup2<Address, _>| of.call(CTup2(Address::from(data.0) - self.module_info.base.to_umem(), data.1)));
                let mut out1 = out1.as_mut().map(<_>::into);
                let out1 = out1.as_mut();

                let mut out_fail1 = data.out_fail
                    .as_mut()
                    .map(|of| move |data: CTup2<Address, _>| of.call(CTup2(Address::from(data.0) - self.module_info.base.to_umem(), data.1)));
                let mut out_fail1 = out_fail1.as_mut().map(<_>::into);
                let out_fail1 = out_fail1.as_mut();

                // create a new MemOps object with the wrapped values
                MemOps::with_raw(inp1, out1, out_fail1, |data| {
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
            memdata_map(data, |mut data| {
                // wrap inp, out, out_fail to add/subtract the base offset
                let inp1 = data.inp.map(|CTup3(addr, meta_addr, data)| CTup3(self.module_info.base + addr.to_umem(), self.module_info.base + meta_addr.to_umem(), data));

                let mut out1 = data.out
                    .as_mut()
                    .map(|of| move |data: CTup2<Address, _>| of.call(CTup2(Address::from(data.0) - self.module_info.base.to_umem(), data.1)));
                let mut out1 = out1.as_mut().map(<_>::into);
                let out1 = out1.as_mut();

                let mut out_fail1 = data.out_fail
                    .as_mut()
                    .map(|of| move |data: CTup2<Address, _>| of.call(CTup2(Address::from(data.0) - self.module_info.base.to_umem(), data.1)));
                let mut out_fail1 = out_fail1.as_mut().map(<_>::into);
                let out_fail1 = out_fail1.as_mut();

                // create a new MemOps object with the wrapped values
                MemOps::with_raw(inp1, out1, out_fail1, |data| {
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
