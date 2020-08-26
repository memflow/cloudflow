use super::VirtualFileDataSource;
use crate::error::Result;

pub struct VMFSStaticDS {
    contents: String,
}

impl VMFSStaticDS {
    pub fn new(contents: String) -> Self {
        Self { contents }
    }
}

impl VirtualFileDataSource for VMFSStaticDS {
    fn content_length(&self) -> Result<u64> {
        Ok(self.contents.len() as u64)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let contents = self.contents.as_bytes();
        let end = std::cmp::min((offset + size as i64) as usize, contents.len());
        Ok(contents[offset as usize..end].to_vec())
    }
}
