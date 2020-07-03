mod init;
use init::*;

mod cli;
use cli::*;

#[macro_use]
extern crate clap;
use clap::{App, ArgMatches};

use log::{trace, Level};
use std::time::Duration;

use flow_core::timed_validator::*;
use flow_core::*;
use flow_win32::*;
use flow_win32::{Error, Result};

fn init_backend(argv: &ArgMatches) -> Result<Box<dyn PhysicalMemory>> {
    match argv.value_of("connector").unwrap() {
        "coredump" => Ok(Box::new(
            flow_coredump::CoreDump::open(argv.value_of("connector_args").unwrap()).unwrap(),
        )),
        "qemu_procfs" => Ok(Box::new(init_qemu_procfs::init_qemu_procfs(&argv).unwrap())),
        _ => Err(Error::Other("the connector requested does not exist")),
    }
}

fn main() -> Result<()> {
    let yaml = load_yaml!("cli.yml");
    let argv = App::from(yaml).get_matches();

    match argv.occurrences_of("verbose") {
        1 => simple_logger::init_with_level(Level::Warn).unwrap(),
        2 => simple_logger::init_with_level(Level::Info).unwrap(),
        3 => simple_logger::init_with_level(Level::Debug).unwrap(),
        4 => simple_logger::init_with_level(Level::Trace).unwrap(),
        _ => simple_logger::init_with_level(Level::Error).unwrap(),
    }

    let mut phys_mem = init_backend(&argv).unwrap();

    let kernel_info = KernelInfo::scanner().mem(&mut *phys_mem).scan()?;

    let mut phys_mem_cached = CachedMemoryAccess::builder()
        .mem(&mut *phys_mem)
        .arch(kernel_info.start_block.arch)
        .validator(TimedCacheValidator::new(Duration::from_millis(1000).into()))
        .build()?;

    let vat = TranslateArch::new(kernel_info.start_block.arch);
    let vat_cached = CachedVirtualTranslate::builder()
        .vat(vat)
        .arch(kernel_info.start_block.arch)
        .validator(TimedCacheValidator::new(Duration::from_millis(1000).into()))
        .build()?;

    let offsets = Win32Offsets::try_with_guid(&kernel_info.kernel_guid)?;
    trace!("offsets: {:?}", offsets);
    let mut kernel = Kernel::new(&mut phys_mem_cached, vat_cached, offsets, kernel_info);

    let mut win32 = Win32Interface::new(&mut kernel)?;
    win32.run()

    //Ok(())
}
