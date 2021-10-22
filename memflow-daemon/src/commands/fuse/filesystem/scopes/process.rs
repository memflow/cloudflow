use super::super::{FileSystemEntry, FileSystemFileHandler, StaticFileReader};
use crate::error::{Error, Result};
use crate::state::{CachedWin32Process, KernelHandle};

use minidump_writer::{
    minidump::Minidump,
    streams::{
        Memory64ListStream, MemoryDescriptor, MinidumpModule, ModuleListStream, SystemInfoStream,
    },
};

use memflow::mem::VirtualMemory;
use memflow::types::size;
use memflow_win32::*;

use std::cell::RefCell;

use std::sync::{Arc, Mutex};

use memflow::types::PageType;

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

pub struct ProcessDumpFile {
    kernel: Arc<Mutex<KernelHandle>>,
    pi: Win32ProcessInfo,
}

impl ProcessDumpFile {
    pub fn new(kernel: Arc<Mutex<KernelHandle>>, pi: Win32ProcessInfo) -> Self {
        Self { kernel, pi }
    }
}

impl FileSystemEntry for ProcessDumpFile {
    fn name(&self) -> &str {
        "dump"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        0
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn open(&self) -> Result<Box<dyn FileSystemFileHandler>> {
        if let Ok(kernel) = self.kernel.lock() {
            match &*kernel {
                KernelHandle::Win32(kernel) => {
                    let process = Win32Process::with_kernel(kernel.clone(), self.pi.clone());
                    Ok(Box::new(ProcessDumpReader::new(process)))
                }
            }
        } else {
            Err(Error::Other("unable to lock kernel".to_string()))
        }
    }
}

struct ProcessDumpReader {
    process: CachedWin32Process,
}

impl ProcessDumpReader {
    pub fn new(process: CachedWin32Process) -> Self {
        Self { process }
    }
}

impl FileSystemFileHandler for ProcessDumpReader {
    fn read(&mut self, offset: u64, size: u32) -> Result<Vec<u8>> {
        self.process
            .virt_mem
            .virt_read_raw(offset.into(), size as usize)
            .data_part()
            .map_err(Error::from)
    }

    fn write(&mut self, offset: u64, data: Vec<u8>) -> Result<usize> {
        let real_size = data.len();
        if real_size > 0 {
            self.process
                .virt_mem
                .virt_write_raw(offset.into(), &data[0..real_size])
                .data_part()
                .map_err(Error::from)?;
            Ok(real_size)
        } else {
            Err(Error::Other(
                "cannot write past the module dump file size".to_string(),
            ))
        }
    }
}

pub struct ProcessMiniDump {
    kernel: Arc<Mutex<KernelHandle>>,
    process_info: Win32ProcessInfo,
    cached_out: Mutex<RefCell<Option<Vec<u8>>>>,
}

impl ProcessMiniDump {
    pub fn new(kernel: Arc<Mutex<KernelHandle>>, process_info: Win32ProcessInfo) -> Self {
        Self {
            kernel,
            process_info,
            cached_out: Mutex::new(RefCell::new(None)),
        }
    }
}

impl FileSystemEntry for ProcessMiniDump {
    fn name(&self) -> &str {
        "mini.dmp"
    }

    fn is_leaf(&self) -> bool {
        true
    }

    fn size(&self) -> usize {
        self.cached_out
            .lock()
            .map(|l| {
                l.borrow()
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or_else(|| size::gb(256))
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
            Ok(Box::new(StaticFileReader::from_vec(out.clone())))
        } else {
            let mut kernel = self
                .kernel
                .lock()
                .map_err(|_| Error::Other("Poisoned lock".to_string()))?;

            match &mut *kernel {
                KernelHandle::Win32(kernel) => {
                    let (major, minor, build) = kernel.kernel_info.kernel_winver.as_tuple();

                    let mut process =
                        Win32Process::with_kernel_ref(kernel, self.process_info.clone());
                    let maps = process.virt_mem.virt_page_map(0);

                    let mut ret = vec![];
                    let mut cursor = std::io::Cursor::new(&mut ret);
                    let mut minidump = Minidump::default();

                    let mut module_list = ModuleListStream::default();

                    for i in process.module_list()? {
                        module_list.add_module(MinidumpModule {
                            base_of_image: i.base.as_u64(),
                            size_of_image: i.size as _,
                            checksum: 0,
                            time_date_stamp: 0,
                            name: i.name,
                        });
                    }

                    minidump
                        .directory
                        .push(Box::new(SystemInfoStream::with_arch_and_version(
                            9, major, minor, build,
                        )));
                    minidump.directory.push(Box::new(module_list));

                    let mut memory_list = Memory64ListStream::default();

                    for (addr, size) in maps {
                        let mut buf = vec![0; size];
                        process
                            .virt_mem
                            .virt_read_raw_into(addr, &mut buf)
                            .data_part()?;
                        memory_list.list.push(MemoryDescriptor {
                            start_of_memory: addr.as_u64(),
                            buf,
                        });
                    }

                    minidump.directory.push(Box::new(memory_list));
                    minidump
                        .write_all(&mut cursor)
                        .map_err(|_| Error::Other("Failed to write minidump".to_string()))?;

                    *locked_cache = Some(ret.clone());

                    Ok(Box::new(StaticFileReader::from_vec(ret)))
                }
            }
        }
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
                    .unwrap_or_else(|| size::gb(256))
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
                .map_err(|_| Error::Other("Poisoned lock".to_string()))?;

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
                        })
                        .collect();

                    *locked_cache = Some(ret.clone());

                    Ok(Box::new(StaticFileReader::from_string(ret)))
                }
            }
        }
    }
}
