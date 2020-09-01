use super::super::{FileSystemEntry, FileSystemFileHandler};
use crate::error::{Error, Result};
use crate::state::KernelHandle;

use std::sync::{Arc, Mutex};

use memflow::*;

// TODO: block storage?
pub struct PhysicalDumpFile {
    kernel: Arc<Mutex<KernelHandle>>,
    phys_size: usize,
}

impl PhysicalDumpFile {
    pub fn new(kernel: Arc<Mutex<KernelHandle>>) -> Self {
        let phys_size = if let Ok(kernel) = kernel.lock() {
            match &*kernel {
                KernelHandle::Win32(kernel) => kernel.phys_mem.metadata().size,
            }
        } else {
            0
        };

        Self { kernel, phys_size }
    }
}

impl FileSystemEntry for PhysicalDumpFile {
    fn name(&self) -> &str {
        "dump"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    // TODO: type regularfile,etc
    fn size(&self) -> usize {
        self.phys_size
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        if let Ok(kernel) = self.kernel.lock() {
            Ok(Box::new(PhysicalDumpReader::new(kernel.clone())))
        } else {
            Err(Error::Other("unable to lock kernel"))
        }
    }
}

struct PhysicalDumpReader {
    kernel: KernelHandle,
}

impl PhysicalDumpReader {
    pub fn new(kernel: KernelHandle) -> Self {
        Self { kernel }
    }
}

impl FileSystemFileHandler for PhysicalDumpReader {
    fn read(&mut self, offset: u64, size: u32) -> Result<Vec<u8>> {
        match &mut self.kernel {
            KernelHandle::Win32(kernel) => {
                let phys_size = kernel.phys_mem.metadata().size;
                let real_size = std::cmp::min(size as usize, phys_size - offset as usize);

                kernel
                    .phys_mem
                    .phys_read_raw((offset as u64).into(), real_size)
                    .map_err(Error::from)
            }
        }
    }
}
