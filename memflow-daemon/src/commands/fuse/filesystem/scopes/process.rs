use super::super::{FileSystemEntry, FileSystemFileHandler, StaticFileReader};
use crate::error::Result;

use memflow_win32::*;

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
