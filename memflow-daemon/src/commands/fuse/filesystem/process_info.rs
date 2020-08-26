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
            name: "process_info".to_string(),
            data_source: Box::new(VMFSProcessInfoDS::new(format!("{:?}", process.proc_info))),
        }))
    }
}

struct VMFSProcessInfoDS {
    proc_info: String,
}

impl VMFSProcessInfoDS {
    pub fn new(proc_info: String) -> Self {
        Self { proc_info }
    }
}

impl VirtualFileDataSource for VMFSProcessInfoDS {
    fn content_length(&self) -> Result<u64> {
        Ok(self.proc_info.len() as u64)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let proc_info = self.proc_info.as_bytes();
        let end = std::cmp::min((offset + size as i64) as usize, proc_info.len());
        Ok(proc_info[offset as usize..end].to_vec())
    }
}
