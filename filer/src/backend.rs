use crate::error::*;
use crate::fs::*;
use crate::plugin_store::*;
use crate::types::*;
use abi_stable::StableAbi;
use cglue::prelude::v1::*;

pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;

use dashmap::DashMap;
use sharded_slab::Slab;

#[derive(StableAbi)]
#[repr(C)]
pub struct ListEntry {
    pub name: ReprCString,
    pub is_branch: bool,
}

pub struct LocalBackend<T> {
    entries: DashMap<String, T>,
    handle_objs: Slab<FileOpsObj<c_void>>,
}

impl<T> Default for LocalBackend<T> {
    fn default() -> Self {
        Self {
            entries: Default::default(),
            handle_objs: Slab::new(),
        }
    }
}

//impl<T: Branch> FileOps for LocalBackend<T> {
//}

impl<T> LocalBackend<T> {
    pub fn insert(&self, name: &str, entry: T) -> bool {
        if self.entries.contains_key(name) {
            false
        } else {
            self.entries.insert(name.into(), entry).is_none()
        }
    }

    fn push_obj(&self, obj: FileOpsObj<c_void>) -> usize {
        self.handle_objs.insert(obj).unwrap()
    }
}

impl<T: Branch> Backend for LocalBackend<T> {
    fn read(&self, handle: usize, data: CIterator<RWData>) -> Result<()> {
        match self.handle_objs.get(handle) {
            Some(f) => f.read(data),
            _ => Err(Error(ErrorOrigin::Backend, ErrorKind::NotFound)),
        }
    }

    fn write(&self, handle: usize, data: CIterator<ROData>) -> Result<()> {
        match self.handle_objs.get(handle) {
            Some(f) => f.write(data),
            _ => Err(Error(ErrorOrigin::Backend, ErrorKind::NotFound)),
        }
    }

    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()> {
        match self.handle_objs.get(handle) {
            Some(f) => f.rpc(input, output),
            _ => Err(Error(ErrorOrigin::Backend, ErrorKind::NotFound)),
        }
    }

    fn open(&self, path: &str, plugins: &CPluginStore) -> Result<usize> {
        let (branch, path) = path.split_once("/").unwrap_or((path, ""));
        match self
            .entries
            .get(branch)
            .and_then(|b| Some(b.get_entry(path, plugins)))
        {
            Some(Ok(DirEntry::Leaf(leaf))) => leaf.open().map(|o| self.push_obj(o)),
            Some(Ok(_)) => Err(Error(ErrorOrigin::Backend, ErrorKind::InvalidArgument)),
            Some(Err(e)) => Err(e),
            _ => Err(Error(ErrorOrigin::Backend, ErrorKind::NotFound)),
        }
    }

    fn list(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()> {
        if path.is_empty() {
            self.entries
                .iter()
                .map(|r| r.key().clone())
                .map(|n| ListEntry::new(n.into(), true))
                .feed_into_mut(out);

            Ok(())
        } else {
            let (branch, path) = path.split_once("/").unwrap_or((path, ""));
            match self.entries.get(branch) {
                Some(branch) => {
                    let cb = &mut |entry: BranchListEntry| {
                        out.call(ListEntry::new(
                            entry.name.into(),
                            matches!(entry.obj, DirEntry::Branch(_)),
                        ))
                    };

                    branch.list_recurse(path, plugins, &mut cb.into())
                }
                _ => Err(Error(ErrorOrigin::Backend, ErrorKind::NotFound)),
            }
        }
    }
}

#[cglue_trait]
#[int_result]
pub trait Backend {
    /// Perform read operation on the given handle
    fn read(&self, handle: usize, data: CIterator<RWData>) -> Result<()>;
    /// Perform write operation on the given handle.
    fn write(&self, handle: usize, data: CIterator<ROData>) -> Result<()>;
    /// Perform remote procedure call on the given handle.
    fn rpc(&self, handle: usize, input: &[u8], output: &mut [u8]) -> Result<()>;
    /// Open a leaf at the given path. The result is a handle.
    fn open(&self, path: &str, plugins: &CPluginStore) -> Result<usize>;
    /// List entries in the given path. It is a (name, is_branch) pair.
    fn list(
        &self,
        path: &str,
        plugins: &CPluginStore,
        out: &mut OpaqueCallback<ListEntry>,
    ) -> Result<()>;
}
