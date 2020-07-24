use clap::ArgMatches;

use memflow_core::*;

#[cfg(all(target_os = "linux", feature = "connector-qemu-procfs"))]
pub fn init_qemu_procfs(argv: &ArgMatches) -> Result<memflow_qemu_procfs::QemuProcfs> {
    if argv.is_present("connector_args") {
        memflow_qemu_procfs::QemuProcfs::with_guest_name(argv.value_of("connector_args").unwrap())
    } else {
        memflow_qemu_procfs::QemuProcfs::new()
    }
}

#[cfg(all(feature = "connector-qemu-procfs", not(target_os = "linux")))]
pub fn init_qemu_procfs(_argv: &ArgMatches) -> Result<super::EmptyPhysicalMemory> {
    Err(Error::Other(
        "connector qemu_procfs is not available on this system",
    ))
}

#[cfg(not(feature = "connector-qemu-procfs"))]
pub fn init_qemu_procfs(_argv: &ArgMatches) -> Result<super::EmptyPhysicalMemory> {
    Err(Error::Other("connector qemu-procfs is not enabled"))
}
