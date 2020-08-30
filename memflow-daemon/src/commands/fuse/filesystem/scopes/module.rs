use super::super::{FileSystemEntry, FileSystemFileHandler};
use crate::error::{Error, Result};
use crate::state::{CachedWin32Process, KernelHandle};

use std::sync::{Arc, Mutex};

use memflow_core::*;
use memflow_win32::*;

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
