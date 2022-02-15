//! Internal FS representation. Does not cross backends.

use crate::error::*;
use crate::plugin_store::*;

use crate::types::*;
use abi_stable::StableAbi;
use cglue::prelude::v1::*;
use cglue::result::from_int_result_empty;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;

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
    read: Option<for<'a> extern "C" fn(&'a T, data: CIterator<RWData>) -> i32>,
    write: Option<for<'a> extern "C" fn(&'a T, data: CIterator<ROData>) -> i32>,
    rpc: Option<
        for<'a> extern "C" fn(&'a T, input: CSliceRef<'a, u8>, output: CSliceMut<'a, u8>) -> i32,
    >,
}

impl<T> FileOpsObj<T> {
    pub fn new(
        obj: CArcSome<T>,
        read: Option<for<'a> extern "C" fn(&'a T, data: CIterator<RWData>) -> i32>,
        write: Option<for<'a> extern "C" fn(&'a T, data: CIterator<ROData>) -> i32>,
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

    pub fn read(&self, data: CIterator<RWData>) -> Result<()> {
        from_int_result_empty((self
            .read
            .ok_or(Error(ErrorOrigin::Read, ErrorKind::NotImplemented))?)(
            &self.obj, data
        ))
    }

    pub fn write(&self, data: CIterator<ROData>) -> Result<()> {
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
