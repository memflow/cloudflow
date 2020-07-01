//pub mod init_bridge;
pub mod init_qemu_procfs;

use flow_core::*;

pub struct EmptyPhysicalMemory {}

impl PhysicalMemory for EmptyPhysicalMemory {
    fn phys_read_raw_list(&mut self, _data: &mut [PhysicalReadData]) -> Result<()> {
        Err(Error::Other("phys_read not implemented"))
    }

    fn phys_write_raw_list(&mut self, _data: &[PhysicalWriteData]) -> Result<()> {
        Err(Error::Other("phys_read not implemented"))
    }
}
