mod connection;
use connection::PhysicalDumpFile;

mod process;
use process::ProcessInfoFile;

mod module;
use module::{ModuleDumpFile, ModulePeFolder};

use super::{ChildrenList, FileSystemChildren, FileSystemEntry};
use crate::state::KernelHandle;

use std::sync::{Arc, Mutex};

use memflow_win32::{Win32ModuleInfo, Win32Process, Win32ProcessInfo};

pub struct ConnectionScope {
    kernel: Arc<Mutex<KernelHandle>>,
    name: String,
    children: FileSystemChildren,
}

impl ConnectionScope {
    pub fn new(kernel: KernelHandle) -> Self {
        Self {
            kernel: Arc::new(Mutex::new(kernel)),
            name: std::path::MAIN_SEPARATOR.to_string(),
            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for ConnectionScope {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        Some(self.children.get_or_insert(|| {
            vec![
                Box::new(DriverRootFolder::new(self.kernel.clone())),
                Box::new(ProcessRootFolder::new(self.kernel.clone())),
                Box::new(PhysicalDumpFile::new(self.kernel.clone())),
            ]
        }))
    }
}

/// Describes the root level 'drivers' folder
pub struct DriverRootFolder {
    kernel: Arc<Mutex<KernelHandle>>,
    children: FileSystemChildren,
}

impl DriverRootFolder {
    pub fn new(kernel: Arc<Mutex<KernelHandle>>) -> Self {
        Self {
            kernel,
            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for DriverRootFolder {
    fn name(&self) -> &str {
        "drivers"
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        Some(self.children.get_or_insert(|| {
            let mut result = Vec::new();

            if let Ok(mut kernel) = self.kernel.lock() {
                match &mut *kernel {
                    KernelHandle::Win32(kernel) => {
                        if let Ok(mut kernel_proc) = kernel.kernel_process() {
                            if let Ok(modules) = kernel_proc.module_list() {
                                for mi in modules.into_iter() {
                                    result.push(Box::new(ModuleFolder::new(
                                        self.kernel.clone(),
                                        kernel_proc.proc_info.clone(),
                                        mi,
                                    ))
                                        as Box<dyn FileSystemEntry>);
                                }
                            }
                        }
                    }
                }
            }

            result
        }))
    }
}

/// Describes the root level 'processes' folder
pub struct ProcessRootFolder {
    kernel: Arc<Mutex<KernelHandle>>,
    children: FileSystemChildren,
}

impl ProcessRootFolder {
    pub fn new(kernel: Arc<Mutex<KernelHandle>>) -> Self {
        Self {
            kernel,
            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for ProcessRootFolder {
    fn name(&self) -> &str {
        "processes"
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        Some(self.children.get_or_insert(|| {
            let mut result = Vec::new();

            if let Ok(mut kernel) = self.kernel.lock() {
                match &mut *kernel {
                    KernelHandle::Win32(kernel) => {
                        if let Ok(processes) = kernel.process_info_list() {
                            for pi in processes.into_iter() {
                                result.push(Box::new(ProcessFolder::new(self.kernel.clone(), pi))
                                    as Box<dyn FileSystemEntry>);
                            }
                        }
                    }
                }
            }

            result
        }))
    }
}

// TODO: unify process_info for different osses
pub struct ProcessFolder {
    kernel: Arc<Mutex<KernelHandle>>,
    pi: Win32ProcessInfo,

    name: String,
    children: FileSystemChildren,
}

impl ProcessFolder {
    fn new(kernel: Arc<Mutex<KernelHandle>>, pi: Win32ProcessInfo) -> Self {
        let name = format!("{}_{}", pi.pid, pi.name);
        Self {
            kernel,
            pi,

            name,
            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for ProcessFolder {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        Some(self.children.get_or_insert(|| {
            vec![
                Box::new(ProcessInfoFile::new(&self.pi)),
                Box::new(ModuleRootFolder::new(self.kernel.clone(), self.pi.clone())),
            ]
        }))
    }
}

pub struct ModuleRootFolder {
    kernel: Arc<Mutex<KernelHandle>>,
    pi: Win32ProcessInfo,

    children: FileSystemChildren,
}

impl ModuleRootFolder {
    fn new(kernel: Arc<Mutex<KernelHandle>>, pi: Win32ProcessInfo) -> Self {
        Self {
            kernel,
            pi,

            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for ModuleRootFolder {
    fn name(&self) -> &str {
        "modules"
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        // TODO: create files for each driver
        Some(self.children.get_or_insert(|| {
            let mut result = Vec::new();

            if let Ok(mut kernel) = self.kernel.lock() {
                match &mut *kernel {
                    KernelHandle::Win32(kernel) => {
                        let mut process = Win32Process::with_kernel_ref(kernel, self.pi.clone());
                        if let Ok(modules) = process.module_list() {
                            for mi in modules.into_iter() {
                                result.push(Box::new(ModuleFolder::new(
                                    self.kernel.clone(),
                                    self.pi.clone(),
                                    mi,
                                ))
                                    as Box<dyn FileSystemEntry>);
                            }
                        }
                    }
                }
            }

            result
        }))
    }
}

pub struct ModuleFolder {
    kernel: Arc<Mutex<KernelHandle>>,
    pi: Win32ProcessInfo,
    mi: Win32ModuleInfo,

    name: String,
    children: FileSystemChildren,
}

// TODO: unify Win32ModuleInfo for different targets
impl ModuleFolder {
    fn new(kernel: Arc<Mutex<KernelHandle>>, pi: Win32ProcessInfo, mi: Win32ModuleInfo) -> Self {
        let name = format!("{:x}_{}", mi.base, mi.name);
        Self {
            kernel,
            pi,
            mi,

            name,
            children: FileSystemChildren::default(),
        }
    }
}

impl FileSystemEntry for ModuleFolder {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_leaf(&self) -> bool {
        false
    }

    fn children(&self) -> Option<ChildrenList> {
        Some(self.children.get_or_insert(|| {
            vec![
                Box::new(ModulePeFolder::new(
                    self.kernel.clone(),
                    self.pi.clone(),
                    self.mi.clone(),
                )),
                Box::new(ModuleDumpFile::new(
                    self.kernel.clone(),
                    self.pi.clone(),
                    self.mi.clone(),
                )),
            ]
        }))
    }
}
