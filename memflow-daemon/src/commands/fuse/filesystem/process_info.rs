use super::static_ds::VMFSStaticDS;
use super::{CachedWin32Process, VMFSProcessExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::Result;

pub struct VMFSProcessInfo;

impl VMFSProcessExt for VMFSProcessInfo {
    fn entry(
        &self,
        inode: u64,
        _conn_id: &str,
        process: &mut CachedWin32Process,
    ) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            inode,
            name: "info".to_string(),
            data_source: Box::new(VMFSStaticDS::new(format!("{:?}", process.proc_info))),
        }))
    }
}
