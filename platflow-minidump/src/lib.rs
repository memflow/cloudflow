use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use filer::prelude::v1::{Error, ErrorKind, ErrorOrigin, Result, *};
use memflow::prelude::v1::*;
use memflow_framework::process::LazyProcessArc;

use minidump_writer::{
    minidump::Minidump,
    streams::{
        Memory64ListStream, MemoryDescriptor, MinidumpModule, ModuleListStream, SystemInfoStream,
    },
};

pub extern "C" fn on_node(node: &Node, ctx: CArc<c_void>) {
    node.plugins
        .register_mapping("mini.dmp", Mapping::Leaf(map_into_minidump, ctx));
}

extern "C" fn map_into_minidump(
    proc: &LazyProcessArc,
    ctx: &CArc<c_void>,
) -> COption<LeafArcBox<'static>> {
    let file = FnFile::new(proc.clone(), |proc| {
        let proc = proc.proc().ok_or(ErrorKind::Uninitialized)?;
        let mut proc = proc.get();

        let maps = proc.mapped_mem_vec(-1);
        let mut modules = proc.module_list().map_err(|_| ErrorKind::Uninitialized)?;
        modules.sort_by_key(|m| m.base.to_umem());

        let mut ret = vec![];
        let mut cursor = std::io::Cursor::new(&mut ret);
        let mut minidump = Minidump::default();

        let mut module_list = ModuleListStream::default();

        for i in modules {
            module_list.add_module(MinidumpModule {
                base_of_image: i.base.to_umem() as u64,
                size_of_image: i.size as _,
                checksum: 0,
                time_date_stamp: 0,
                name: i.name.to_string(),
            });
        }

        minidump
            .directory
            .push(Box::new(SystemInfoStream::with_arch_and_version(
                9, 10, 0, 19041, //major, minor, build,
            )));
        minidump.directory.push(Box::new(module_list));

        let mut memory_list = Memory64ListStream::default();

        for CTup3(addr, size, _) in maps {
            let mut buf = vec![0; size as usize];
            proc.read_raw_into(addr, &mut buf)
                .data_part()
                .map_err(|_| ErrorKind::UnableToReadFile)?;
            memory_list.list.push(MemoryDescriptor {
                start_of_memory: addr.to_umem() as _,
                buf,
            });
        }

        minidump.directory.push(Box::new(memory_list));
        minidump
            .write_all(&mut cursor)
            .map_err(|_| ErrorKind::UnableToWriteFile)?;

        Ok(ret)
    });

    COption::Some(trait_obj!((file, ctx.clone()) as Leaf))
}
