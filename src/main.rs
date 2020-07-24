mod init;
use init::*;

mod cli;
use cli::*;

#[macro_use]
extern crate clap;
use clap::{App, ArgMatches};

use log::{trace, Level};
use std::time::Duration;

use memflow_core::timed_validator::*;
use memflow_core::*;
use memflow_core::connector::*;
use memflow_win32::{Error, Result};

use memflow_win32::*;

fn init_backend(argv: &ArgMatches) -> Result<Box<dyn PhysicalMemory>> {
    match argv.value_of("connector").unwrap() {
        "coredump" => Ok(Box::new(
            memflow_coredump::create_connector(&ConnectorArgs::with_default(argv.value_of("connector_args").unwrap())).unwrap(),
        )),
        "qemu_procfs" => Ok(Box::new(init_qemu_procfs::init_qemu_procfs(&argv).unwrap())),
        _ => return Err(Error::Other("the connector requested does not exist")),
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

    /*
    let phys_page_cache = PageCache::new(
        kernel_info.start_block.arch,
        Length::from_mb(32),
        PageType::PAGE_TABLE | PageType::READ_ONLY,
        TimedCacheValidator::new(Duration::from_millis(1000).into()),
    );
    let mem_cached = CachedMemoryAccess::with(&mut phys_mem, phys_page_cache);
    */
    let mut mem_cached = phys_mem;

    let mut vat = TranslateArch::new(kernel_info.start_block.arch);

    /*
    println!("------------------ VTOP ----------------");
    let phys_addr = vat
        .virt_to_phys(
            &mut mem_cached,
            Address::from(0x185000u64),
            Address::from(0x85dde9d8u64 + 0xb8u64),
        )
        .unwrap();
    println!("phys_addr: {}", phys_addr);
    */

    /*
    let tlb_cache = TLBCache::new(
        2048.into(),
        TimedCacheValidator::new(Duration::from_millis(1000).into()),
    );
    let vat_cached =
        CachedVirtualTranslate::with(&mut vat, tlb_cache, kernel_info.start_block.arch);
    */

    let vat_cached = vat;

    let offsets = Win32Offsets::try_with_kernel_info(&kernel_info)?;
    trace!("offsets: {:?}", offsets);
    let mut kernel = Kernel::new(&mut *mem_cached, vat_cached, offsets, kernel_info);

    let mut win32 = Win32Interface::new(&mut kernel)?;
    win32.run()

    //Ok(())
}
