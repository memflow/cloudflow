use crate::error::*;
use crate::fs::*;
use crate::plugin_store::*;
use crate::str_build::*;
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

fn map_exists<T>(entries: &DashMap<String, T>, name: &str) -> bool {
    ["new", "rm"].contains(&name) || entries.contains_key(name)
}

fn map_insert<T>(entries: &DashMap<String, T>, name: &str, entry: T) -> bool {
    if name.contains("/") || map_exists(entries, name) {
        false
    } else {
        entries.insert(name.into(), entry).is_none()
    }
}

fn map_checked_insert<T>(entries: &DashMap<String, T>, name: &str, entry: T) -> Result<()> {
    if !map_insert(entries, name, entry) {
        Err(Error(ErrorOrigin::Backend, ErrorKind::InvalidArgument))
    } else {
        Ok(())
    }
}

struct NewHandler<T: 'static, C: 'static>(
    CArcSome<DashMap<String, T>>,
    CArc<C>,
    fn(&str, &CArc<C>) -> Result<T>,
);

impl<T, C> NewHandler<T, C> {
    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        for CTup2(_, d) in data {
            if let Err(e) = std::str::from_utf8(&d)
                .map_err(|_| Error(ErrorOrigin::Backend, ErrorKind::InvalidArgument))
                .map(|a| a.split_once(" ").unwrap_or((a, "")))
                .and_then(|(n, a)| (self.2)(a, &self.1).map(|o| (n, o)))
                .and_then(|(n, o)| map_checked_insert(&*self.0, n, o))
            {
                return e.into_int_err().into();
            }
        }
        0
    }
}

struct RmHandler<T: 'static>(CArcSome<DashMap<String, T>>);

impl<T> RmHandler<T> {
    extern "C" fn write(&self, data: CIterator<ROData>) -> i32 {
        for CTup2(_, d) in data {
            if let Err(e) = std::str::from_utf8(&d)
                .map_err(|_| Error(ErrorOrigin::Backend, ErrorKind::InvalidArgument))
                .map(|n| self.0.remove(n))
            {
                return e.into_int_err().into();
            }
        }
        0
    }
}

pub struct LocalBackend<T: 'static, C: 'static = ()> {
    entries: CArcSome<DashMap<String, T>>,
    context: CArc<C>,
    handle_objs: Slab<FileOpsObj<c_void>>,
    new_handle: Result<usize>,
    rm_handle: usize,
}

impl<T, C> Default for LocalBackend<T, C> {
    fn default() -> Self {
        let entries: DashMap<String, T> = Default::default();
        let entries: CArcSome<DashMap<String, T>> = entries.into();

        let handle_objs = Slab::new();
        let rm_obj = RmHandler(entries.clone());
        let rm_obj = FileOpsObj::new(rm_obj.into(), None, Some(RmHandler::write), None);
        let rm_handle = handle_objs.insert(rm_obj).unwrap();

        Self {
            entries,
            handle_objs,
            context: CArc::default(),
            new_handle: Err(Error(ErrorOrigin::Backend, ErrorKind::NotSupported)),
            rm_handle,
        }
    }
}

impl<T: StrBuild<CArc<C>>, C> LocalBackend<T, C> {
    pub fn with_new(mut self) -> Self {
        let new_obj = NewHandler(self.entries.clone(), self.context.clone(), T::build);
        let new_obj = FileOpsObj::new(new_obj.into(), None, Some(NewHandler::write), None);
        let new_handle = self
            .handle_objs
            .insert(new_obj)
            .ok_or(Error(ErrorOrigin::Backend, ErrorKind::Unknown));
        self.new_handle = new_handle;
        self
    }
}
//impl<T: Branch> FileOps for LocalBackend<T> {
//}

impl<T, C> LocalBackend<T, C> {
    pub fn with_context_arc<NC>(self, context: CArc<NC>) -> LocalBackend<T, NC> {
        let Self {
            entries,
            handle_objs,
            new_handle,
            rm_handle,
            ..
        } = self;

        LocalBackend {
            entries,
            handle_objs,
            context,
            new_handle,
            rm_handle,
        }
    }

    pub fn with_context<NC>(self, ctx: NC) -> LocalBackend<T, NC> {
        self.with_context_arc::<NC>(ctx.into())
    }

    pub fn insert(&self, name: &str, entry: T) -> bool {
        map_insert(&*self.entries, name, entry)
    }

    pub fn checked_insert(&self, name: &str, entry: T) -> Result<()> {
        map_checked_insert(&*self.entries, name, entry)
    }

    fn push_obj(&self, obj: FileOpsObj<c_void>) -> usize {
        self.handle_objs.insert(obj).unwrap()
    }
}

impl<T: Branch, C> Backend for LocalBackend<T, C> {
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

        if path.is_empty() && branch == "new" {
            self.new_handle
        } else if path.is_empty() && branch == "rm" {
            Ok(self.rm_handle)
        } else {
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

            let _ = out.call(ListEntry::new("rm".into(), false));

            if self.new_handle.is_ok() {
                let _ = out.call(ListEntry::new("new".into(), false));
            }

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