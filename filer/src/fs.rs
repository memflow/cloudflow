//! Internal FS representation. Does not cross backends.

use crate::error::*;
use crate::plugin_store::*;

use crate::types::*;
use abi_stable::StableAbi;
use cglue::prelude::v1::*;
use cglue::result::from_int_result_empty;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use core::num::NonZeroI32;

/// Safely wrap fallible functions to return a integer result value.
///
/// This is effectively needed when defining `extern "C"` functions that for file ops.
#[macro_export]
macro_rules! int_res_wrap {
    ($($expr:tt)*) => {
        let __wrapped_fn = || -> $crate::error::Result<()> { $($expr)* };
        __wrapped_fn().into_int_result()
    };
}

#[repr(C)]
#[derive(StableAbi)]
pub enum DirEntry {
    Branch(BranchBox<'static>),
    Leaf(LeafBox<'static>),
}

#[repr(C)]
#[derive(StableAbi)]
pub struct BranchListEntry {
    pub name: ReprCString,
    pub obj: DirEntry,
}

impl BranchListEntry {
    pub fn new(name: ReprCString, obj: DirEntry) -> Self {
        Self { name, obj }
    }
}

#[cglue_trait]
#[int_result]
pub trait Branch {
    fn get_entry(&self, path: &str, plugins: &CPluginStore) -> Result<DirEntry>;
    fn list(&self, plugins: &CPluginStore, out: &mut OpaqueCallback<BranchListEntry>)
        -> Result<()>;

    fn list_recurse(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<BranchListEntry>,
    ) -> Result<()> {
        if path.is_empty() {
            self.list(plugins, out)
        } else {
            match self.get_entry(path, plugins) {
                Ok(DirEntry::Branch(branch)) => branch.list(plugins, out),
                Ok(_) => Err(Error(ErrorOrigin::Branch, ErrorKind::InvalidPath)),
                Err(e) => Err(e),
            }
        }
    }
}

#[cglue_trait]
#[int_result]
pub trait Leaf {
    fn open(&self) -> Result<FileOpsObj<c_void>>;
}

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct FileOpsObj<T: 'static> {
    obj: CArcSome<T>,
    read: Option<for<'a> extern "C" fn(&T, data: VecOps<RWData>) -> i32>,
    write: Option<for<'a> extern "C" fn(&T, data: VecOps<ROData>) -> i32>,
    rpc: Option<
        for<'a> extern "C" fn(&'a T, input: CSliceRef<'a, u8>, output: CSliceMut<'a, u8>) -> i32,
    >,
}

impl<T> FileOpsObj<T> {
    pub fn new(
        obj: CArcSome<T>,
        read: Option<for<'a> extern "C" fn(&T, data: VecOps<RWData>) -> i32>,
        write: Option<for<'a> extern "C" fn(&T, data: VecOps<ROData>) -> i32>,
        rpc: Option<
            for<'a> extern "C" fn(
                &'a T,
                input: CSliceRef<'a, u8>,
                output: CSliceMut<'a, u8>,
            ) -> i32,
        >,
    ) -> FileOpsObj<c_void> {
        Self {
            obj,
            read,
            write,
            rpc,
        }
        .into_opaque()
    }

    pub fn read<'a>(&self, data: VecOps<RWData>) -> Result<()> {
        from_int_result_empty((self
            .read
            .ok_or(Error(ErrorOrigin::Read, ErrorKind::NotImplemented))?)(
            &self.obj, data
        ))
    }

    pub fn write<'a>(&self, data: VecOps<ROData>) -> Result<()> {
        from_int_result_empty((self
            .write
            .ok_or(Error(ErrorOrigin::Write, ErrorKind::NotImplemented))?)(
            &self.obj, data,
        ))
    }

    pub fn rpc(&self, input: &[u8], output: &mut [u8]) -> Result<()> {
        from_int_result_empty((self
            .rpc
            .ok_or(Error(ErrorOrigin::Rpc, ErrorKind::NotImplemented))?)(
            &self.obj,
            input.into(),
            output.into(),
        ))
    }
}

unsafe impl<T> cglue::trait_group::Opaquable for FileOpsObj<T> {
    type OpaqueTarget = FileOpsObj<c_void>;
}

#[derive(Clone)]
pub struct FnFile<C, D> {
    ctx: C,
    data: once_cell::sync::OnceCell<D>,
    func: fn(&C) -> Result<D>,
}

impl<C: Clone + 'static, D: AsRef<[u8]> + Clone + 'static> Leaf for FnFile<C, D> {
    fn open(&self) -> Result<FileOpsObj<c_void>> {
        Ok(FileOpsObj::new(
            self.clone().into(),
            Some(Self::read),
            None,
            None,
        ))
    }
}

impl<C, D: AsRef<[u8]>> FnFile<C, D> {
    pub fn new(ctx: C, func: fn(&C) -> Result<D>) -> Self {
        Self {
            ctx,
            data: Default::default(),
            func,
        }
    }

    extern "C" fn read<'a>(&self, mut data: VecOps<RWData<'a>>) -> i32 {
        int_res_wrap! {
            let file = self.data.get_or_try_init(|| (self.func)(&self.ctx))?;
            let file: &[u8] = file.as_ref();

            for CTup2(off, to) in data.inp {
                let maps = &file[std::cmp::min(off as usize, file.len())..];
                let min_len = std::cmp::min(maps.len(), to.len());
                let to: &'a mut [u8] = to.into();
                let (to, to_reject) = to.split_at_mut(min_len);
                to.copy_from_slice(&maps[..min_len]);

                let mut cont = false;

                if !to.is_empty() {
                    cont = opt_call(data.out.as_deref_mut(), CTup2(off, to.into()));
                }
                if !to_reject.is_empty() {
                    cont = opt_call(
                        data.out_fail.as_deref_mut(),
                        (
                            CTup2(off + min_len as u64, to_reject.into()),
                            Error(ErrorOrigin::Read, ErrorKind::OutOfBounds),
                        )
                            .into(),
                    ) || cont;
                }

                if !cont {
                    return Err(Error(ErrorOrigin::Read, ErrorKind::Unknown));
                }
            }

            Ok(())
        }
    }
}
