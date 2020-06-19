//pub mod init_bridge;
pub mod init_qemu_procfs;

use flow_core::*;

pub struct EmptyPhysicalMemory {}

impl PhysicalMemory for EmptyPhysicalMemory {
    fn phys_read_iter<'a, PI: PhysicalReadIterator<'a>>(&'a mut self, _iter: PI) -> Result<()> {
        Err(Error::Other("phys_read not implemented"))
    }

    fn phys_write_iter<'a, PI: PhysicalWriteIterator<'a>>(&'a mut self, _iter: PI) -> Result<()> {
        Err(Error::Other("phys_read not implemented"))
    }
}
