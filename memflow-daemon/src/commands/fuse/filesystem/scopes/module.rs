use super::super::{
    ChildrenList, FileSystemChildren, FileSystemEntry, FileSystemFileHandler, StaticFileReader,
};
use crate::error::{Error, Result};
use crate::state::{CachedWin32Process, KernelHandle};

use std::sync::{Arc, Mutex};

use memflow::*;
use memflow_win32::*;

use pelite::pe64::imports::Import;
use pelite::pe64::*;

pub struct ModuleDumpFile {
    kernel: Arc<Mutex<KernelHandle>>,
    pi: Win32ProcessInfo,
    mi: Win32ModuleInfo,
}

impl ModuleDumpFile {
    pub fn new(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Self {
        Self { kernel, pi, mi }
    }
}

impl FileSystemEntry for ModuleDumpFile {
    fn name(&self) -> &str {
        "dump"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        self.mi.size
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        if let Ok(kernel) = self.kernel.lock() {
            match &*kernel {
                KernelHandle::Win32(kernel) => {
                    let process = Win32Process::with_kernel(kernel.clone(), self.pi.clone());
                    Ok(Box::new(ModuleDumpReader::new(process, self.mi.clone())))
                }
            }
        } else {
            Err(Error::Other("unable to lock kernel"))
        }
    }
}

struct ModuleDumpReader {
    process: CachedWin32Process,
    mi: Win32ModuleInfo,
}

impl ModuleDumpReader {
    pub fn new(process: CachedWin32Process, mi: Win32ModuleInfo) -> Self {
        Self { process, mi }
    }
}

impl FileSystemFileHandler for ModuleDumpReader {
    fn read(&mut self, offset: u64, size: u32) -> Result<Vec<u8>> {
        let mod_size = self.mi.size;
        let real_size = std::cmp::min(size as usize, mod_size - offset as usize);

        self.process
            .virt_mem
            .virt_read_raw(self.mi.base + offset as usize, real_size)
            .data_part()
            .map_err(Error::from)
    }

    fn write(&mut self, offset: u64, data: Vec<u8>) -> Result<usize> {
        let mod_size = self.mi.size;
        let real_size = std::cmp::min(data.len(), mod_size - offset as usize);
        if real_size > 0 {
            self.process
                .virt_mem
                .virt_write_raw(self.mi.base + offset as usize, &data[0..real_size])
                .data_part()
                .map_err(Error::from)?;
            Ok(real_size)
        } else {
            Err(Error::Other("cannot write past the module dump file size"))
        }
    }
}

/// Generates a virtual folder which contains PE header, imports and exports.
pub struct ModulePeFolder {
    kernel: Arc<Mutex<KernelHandle>>,
    pi: Win32ProcessInfo,
    mi: Win32ModuleInfo,

    children: FileSystemChildren,
}

impl ModulePeFolder {
    pub fn new(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Self {
        Self {
            kernel,
            pi,
            mi,

            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for ModulePeFolder {
    fn name(&self) -> &str {
        "pe"
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        Some(self.children.get_or_insert(|| {
            vec![
                Box::new(ModulePeHeaderFile::new(
                    self.kernel.clone(),
                    self.pi.clone(),
                    self.mi.clone(),
                )),
                Box::new(ModulePeImportsFile::new(
                    self.kernel.clone(),
                    self.pi.clone(),
                    self.mi.clone(),
                )),
                Box::new(ModulePeExportsFile::new(
                    self.kernel.clone(),
                    self.pi.clone(),
                    self.mi.clone(),
                )),
            ]
        }))
    }
}

/// Generates a virtual file containing the serialized PE Header from PELite.
pub struct ModulePeHeaderFile {
    pe_header: String,
}

impl ModulePeHeaderFile {
    pub fn new(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Self {
        let pe_header = Self::try_get_pe_header(kernel, pi, mi).unwrap_or_default();
        Self { pe_header }
    }

    fn try_get_pe_header(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Result<String> {
        let mut kernel = kernel
            .lock()
            .map_err(|_| Error::Other("unable to acquire kernel lock"))?;
        match &mut *kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = Win32Process::with_kernel_ref(kernel, pi);
                let image = process
                    .virt_mem
                    .virt_read_raw(mi.base, mi.size)
                    .data_part()?;
                let pe = PeView::from_bytes(&image).map_err(Error::PE)?;
                serde_json::to_string_pretty(&pe).map_err(|_| Error::Serialize)
            }
        }
    }
}

impl FileSystemEntry for ModulePeHeaderFile {
    fn name(&self) -> &str {
        "header"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        self.pe_header.len()
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        Ok(Box::new(StaticFileReader::new(&self.pe_header)))
    }
}

/// Generates a virtual file containing all imports from the PE file
pub struct ModulePeImportsFile {
    pe_imports: String,
}

impl ModulePeImportsFile {
    pub fn new(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Self {
        let pe_imports = Self::try_get_pe_imports(kernel, pi, mi).unwrap_or_default();
        Self { pe_imports }
    }

    fn try_get_pe_imports(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Result<String> {
        let mut kernel = kernel
            .lock()
            .map_err(|_| Error::Other("unable to acquire kernel lock"))?;
        match &mut *kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = Win32Process::with_kernel_ref(kernel, pi);
                let image = process
                    .virt_mem
                    .virt_read_raw(mi.base, mi.size)
                    .data_part()?;
                let pe = PeView::from_bytes(&image).map_err(Error::PE)?;

                let imports = pe.imports().map_err(Error::PE)?;
                let mut out = String::new();
                for desc in imports {
                    let dll_name = desc.dll_name().map_err(Error::PE)?;
                    let iat = desc.iat().map_err(Error::PE)?;
                    let int = desc.int().map_err(Error::PE)?;

                    for (_va, import) in Iterator::zip(iat, int) {
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
                Ok(out)
            }
        }
    }
}

impl FileSystemEntry for ModulePeImportsFile {
    fn name(&self) -> &str {
        "imports"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        self.pe_imports.len()
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        Ok(Box::new(StaticFileReader::new(&self.pe_imports)))
    }
}

/// Generates a virtual file containing all exports from the PE file
pub struct ModulePeExportsFile {
    pe_exports: String,
}

impl ModulePeExportsFile {
    pub fn new(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Self {
        let pe_exports = Self::try_get_pe_exports(kernel, pi, mi).unwrap_or_default();
        Self { pe_exports }
    }

    fn try_get_pe_exports(
        kernel: Arc<Mutex<KernelHandle>>,
        pi: Win32ProcessInfo,
        mi: Win32ModuleInfo,
    ) -> Result<String> {
        let mut kernel = kernel
            .lock()
            .map_err(|_| Error::Other("unable to acquire kernel lock"))?;
        match &mut *kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = Win32Process::with_kernel_ref(kernel, pi);
                let image = process
                    .virt_mem
                    .virt_read_raw(mi.base, mi.size)
                    .data_part()?;
                let pe = PeView::from_bytes(&image).map_err(Error::PE)?;

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
                                mi.name,
                                function_rva,
                                mi.base + function_rva,
                            ));
                        }
                    }
                }
                Ok(out)
            }
        }
    }
}

impl FileSystemEntry for ModulePeExportsFile {
    fn name(&self) -> &str {
        "exports"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        self.pe_exports.len()
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        Ok(Box::new(StaticFileReader::new(&self.pe_exports)))
    }
}
