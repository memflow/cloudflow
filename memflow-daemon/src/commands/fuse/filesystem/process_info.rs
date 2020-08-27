use super::static_ds::VMFSStaticDS;
use super::{CachedWin32Process, VMFSProcessExt, VirtualEntry, VirtualFile};
use crate::error::Result;

pub struct VMFSProcessInfo;

impl VMFSProcessExt for VMFSProcessInfo {
    fn entry(
        &self,
        _conn_id: &str,
        process: &mut CachedWin32Process,
        _add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            name: "info".to_string(),
            data_source: Box::new(VMFSStaticDS::new(format!("{:?}", process.proc_info))),
        }))
    }
}
