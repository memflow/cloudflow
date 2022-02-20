use crate::util::*;
use crate::MemflowBackend;
use abi_stable::StableAbi;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::branch;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
use memflow::prelude::v1::*;

use std::sync::Arc;

pub extern "C" fn on_node(node: &Node) {
    node.plugins
        .register_mapping("mem", Mapping::Leaf(self_as_leaf::<ThreadedConnectorArc>));
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

    fn metadata(&self) -> Result<NodeMetadata> {
        Ok(NodeMetadata {
            is_branch: false,
            has_read: true,
            has_write: true,
            has_rpc: true,
            size: self.get_orig().metadata().max_address.to_umem() as Size,
            ..Default::default()
        })
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

impl ThreadedConnector {
    extern "C" fn read(&self, data: VecOps<RWData>) -> i32 {
        int_res_wrap! {
            memdata_map(data, |data| {
                self.get()
                    .phys_view()
                    .read_raw_iter(
                        data,
                    )
                    .map_err(|_| Error(ErrorOrigin::Read, ErrorKind::Unknown))
            })
        }
    }

    extern "C" fn write(&self, data: VecOps<ROData>) -> i32 {
        int_res_wrap! {
            memdata_map(data, |data| {
                self.get()
                    .phys_view()
                    .write_raw_iter(
                        data,
                    )
                    .map_err(|_| Error(ErrorOrigin::Write, ErrorKind::Unknown))
            })
        }
    }

    extern "C" fn rpc(&self, _input: CSliceRef<u8>, _output: CSliceMut<u8>) -> i32 {
        Result::Ok(()).into_int_result()
    }
}
