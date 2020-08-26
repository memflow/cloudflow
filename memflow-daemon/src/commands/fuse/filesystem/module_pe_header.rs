use super::static_ds::VMFSStaticDS;
use super::{CachedWin32Process, VMFSModuleExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};

use memflow_core::*;
use memflow_win32::*;

use pelite::pe64::*;

pub struct VMFSModulePEHeader;

// TODO: only enable this for windows targets
impl VMFSModuleExt for VMFSModulePEHeader {
    fn entry(
        &self,
        inode: u64,
        _conn_id: &str,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
    ) -> Result<VirtualEntry> {
        let image = process
            .virt_mem
            .virt_read_raw(mod_info.base(), size::kb(4))
            .data_part()?;
        let pe = PeView::from_bytes(&image).map_err(Error::PE)?;

        // TODO: once serde works use a lazy pe view here and not a static ds
        /*
        let pectx = MemoryPeViewContext::new(&mut ctx.process.virt_mem, mod_info.base())
            .map_err(Error::PE)?;
        let pe = pe32::MemoryPeView::new(&pectx).map_err(Error::PE)?;
        */

        let pestr = serde_json::to_string_pretty(&pe).map_err(|_| Error::Serialize)?;

        Ok(VirtualEntry::File(VirtualFile {
            inode,
            name: "pe_header".to_string(),
            data_source: Box::new(VMFSStaticDS::new(pestr)),
        }))
    }
}
