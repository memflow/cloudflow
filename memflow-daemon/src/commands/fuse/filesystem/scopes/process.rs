use super::super::{FileSystemEntry, FileSystemFileHandler, StaticFileReader};
use crate::error::{Error, Result};
use crate::state::KernelHandle;

use memflow_core::mem::VirtualMemory;
use memflow_core::types::size;
use memflow_win32::*;

use std::cell::RefCell;

use std::sync::{Arc, Mutex};

use memflow_core::types::PageType;

pub struct ProcessInfoFile {
    pistr: String,
}

impl ProcessInfoFile {
    pub fn new(pi: &Win32ProcessInfo) -> Self {
        let pistr = serde_json::to_string_pretty(pi).unwrap_or_default();
        Self { pistr }
    }
}

impl FileSystemEntry for ProcessInfoFile {
    fn name(&self) -> &str {
        "info"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        self.pistr.len()
    }

    fn is_writable(&self) -> bool {
        false
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        Ok(Box::new(StaticFileReader::new(&self.pistr)))
    }
}

pub struct ProcessMemoryMaps {
    kernel: Arc<Mutex<KernelHandle>>,
    process_info: Win32ProcessInfo,
    cached_out: Mutex<RefCell<Option<String>>>,
}

impl ProcessMemoryMaps {
    pub fn new(kernel: Arc<Mutex<KernelHandle>>, process_info: Win32ProcessInfo) -> Self {
        Self {
            kernel,
            process_info,
            cached_out: Mutex::new(RefCell::new(None)),
        }
    }
}

impl FileSystemEntry for ProcessMemoryMaps {
    fn name(&self) -> &str {
        "maps"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        // We would normally just return 0, but for some reason reads with 0 sized files just don't
        // work!!!
        self.cached_out
            .lock()
            .map(|l| {
                l.borrow()
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(size::gb(256))
            })
            .unwrap_or_default()
    }

    fn is_writable(&self) -> bool {
        false
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        let lock = self.cached_out.lock().unwrap();
        let mut locked_cache = lock.borrow_mut();

        if let Some(out) = locked_cache.as_ref() {
            Ok(Box::new(StaticFileReader::from_string(out.clone())))
        } else {
            let mut kernel = self
                .kernel
                .lock()
                .map_err(|_| Error::Other("Poisoned lock"))?;

            match &mut *kernel {
                KernelHandle::Win32(kernel) => {
                    let mut process =
                        Win32Process::with_kernel_ref(kernel, self.process_info.clone());
                    let maps = process.virt_mem.virt_translation_map();
                    let module_list = process.module_list()?;

                    let ret: String = maps
                        .into_iter()
                        .map(|(vaddr, size, paddr)| {
                            let module = module_list
                                .iter()
                                .find(|m| m.base <= vaddr && m.base + m.size > vaddr);
                            let perms = format!(
                                "r{}{}",
                                if paddr.page_type().contains(PageType::WRITEABLE) {
                                    'w'
                                } else {
                                    '-'
                                },
                                if !paddr.page_type().contains(PageType::NOEXEC) {
                                    'x'
                                } else {
                                    '-'
                                }
                            );
                            format!(
                                "{:x}-{:x} {} {:9x} {}\n",
                                vaddr,
                                vaddr + size,
                                perms,
                                paddr,
                                module.map(|m| m.name.clone()).unwrap_or_default()
                            )
                            .to_string()
                        })
                        .collect();

                    *locked_cache = Some(ret.clone());

                    Ok(Box::new(StaticFileReader::from_string(ret)))
                }
            }
        }
    }
}
