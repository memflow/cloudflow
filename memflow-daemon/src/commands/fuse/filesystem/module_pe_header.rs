use super::{
    CachedWin32Process, VMFSModuleExt, VirtualEntry, VirtualFile, VirtualFileDataSource,
    VirtualFolder,
};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

use pelite::pe64::imports::Import;
use pelite::pe64::*;

pub struct VMFSModulePEHeader;

// TODO: only enable this for windows targets
impl VMFSModuleExt for VMFSModulePEHeader {
    fn entry(
        &self,
        conn_id: &str,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
        add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry> {
        // create 'pe' folder
        let mut folder = VirtualFolder::new("pe");

        // insert 'header' file as child entry
        folder
            .children
            .push(add_child(VirtualEntry::File(VirtualFile {
                name: "header".to_string(),
                data_source: Box::new(PeFileDS::new(
                    PeFileType::Header,
                    &conn_id,
                    process.proc_info.pid,
                    mod_info,
                )),
            })));

        // insert 'imports' file as child entry
        folder
            .children
            .push(add_child(VirtualEntry::File(VirtualFile {
                name: "imports".to_string(),
                data_source: Box::new(PeFileDS::new(
                    PeFileType::Imports,
                    &conn_id,
                    process.proc_info.pid,
                    mod_info,
                )),
            })));

        // insert 'exports' file as child entry
        folder
            .children
            .push(add_child(VirtualEntry::File(VirtualFile {
                name: "exports".to_string(),
                data_source: Box::new(PeFileDS::new(
                    PeFileType::Exports,
                    &conn_id,
                    process.proc_info.pid,
                    mod_info,
                )),
            })));

        // insert 'resources' file as child entry

        Ok(VirtualEntry::Folder(folder))
    }
}

enum PeFileType {
    Header,
    Imports,
    Exports,
}

struct PeFileDS {
    ty: PeFileType,
    conn_id: String,
    pid: PID,
    mod_info: Win32ModuleInfo,
    data: Option<String>,
}

impl PeFileDS {
    pub fn new(ty: PeFileType, conn_id: &str, pid: PID, mod_info: &Win32ModuleInfo) -> Self {
        Self {
            ty,
            conn_id: conn_id.to_string(),
            pid,
            mod_info: mod_info.clone(),
            data: None,
        }
    }

    fn update_data(&mut self) -> Result<()> {
        if self.data.is_some() {
            return Ok(());
        }

        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;

                let image = process
                    .virt_mem
                    .virt_read_raw(self.mod_info.base, self.mod_info.size)
                    .data_part()?;
                let pe = PeView::from_bytes(&image).map_err(Error::PE)?;

                // TODO: once serde works use a lazy pe view here and not a static ds
                /*
                let pectx = MemoryPeViewContext::new(&mut ctx.process.virt_mem, mod_info.base())
                    .map_err(Error::PE)?;
                let pe = pe32::MemoryPeView::new(&pectx).map_err(Error::PE)?;
                */

                let pestr = match self.ty {
                    PeFileType::Header => {
                        serde_json::to_string_pretty(&pe).map_err(|_| Error::Serialize)?
                    }
                    PeFileType::Imports => {
                        let imports = pe.imports().map_err(Error::PE)?;
                        let mut out = String::new();
                        for desc in imports {
                            let dll_name = desc.dll_name().map_err(Error::PE)?;
                            let iat = desc.iat().map_err(Error::PE)?;
                            let int = desc.int().map_err(Error::PE)?;

                            for (va, import) in Iterator::zip(iat, int) {
                                if let Ok(import) = import {
                                    match import {
                                        Import::ByName { hint: _, name } => {
                                            out.push_str(&format!("{}!{}\n", dll_name, name,));
                                        }
                                        Import::ByOrdinal { ord: _ } => {
                                            // TODO:
                                        }
                                    }
                                }
                            }
                        }
                        out
                    }
                    PeFileType::Exports => {
                        let exports = pe.exports().map_err(Error::PE)?;
                        let mut out = String::new();
                        for (&name_rva, function_rva) in exports
                            .by()
                            .map_err(Error::PE)?
                            .names()
                            .iter()
                            .zip(exports.by().map_err(Error::PE)?.functions())
                        {
                            if let Ok(name_it) = pe.derva_c_str(name_rva) {
                                if let Ok(name_str) = std::str::from_utf8(name_it.as_ref()) {
                                    out.push_str(&format!(
                                        "{} = {}!0x{:x} (0x{:x})\n",
                                        name_str,
                                        self.mod_info.name,
                                        function_rva,
                                        self.mod_info.base + function_rva,
                                    ));
                                }
                            }
                        }
                        out
                    }
                };

                self.data = Some(pestr);
                Ok(())
            }
        }
    }
}

impl VirtualFileDataSource for PeFileDS {
    fn content_length(&mut self) -> Result<u64> {
        self.update_data()?;

        Ok(self.data.as_ref().unwrap().len() as u64)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        self.update_data()?;

        let contents = self.data.as_ref().unwrap().as_bytes();
        let end = std::cmp::min((offset + size as i64) as usize, contents.len());
        Ok(contents[offset as usize..end].to_vec())
    }
}
