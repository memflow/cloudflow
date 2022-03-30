use crate::error::*;
use crate::fs::*;
use crate::plugin_store::*;

use abi_stable::StableAbi;
use cglue::arc::CArc;
use cglue::trait_group::c_void;
use cglue::trait_obj;

pub use cglue::slice::CSliceMut;

use std::collections::HashMap;

pub fn split_path(path: &str) -> (&str, Option<&str>) {
    path.split_once('/')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((path, None))
}

pub fn map_entry<T: Branch + StableAbi>(
    branch: &T,
    entry: &Mapping<T>,
    remote: Option<&str>,
    plugins: &CPluginStore,
) -> Result<DirEntry> {
    match (remote, entry) {
        (Some(path), Mapping::Branch(map, ctx)) => map(branch, ctx)
            .as_ref()
            .ok_or::<ErrorKind>(ErrorKind::NotFound)?
            .get_entry(path, plugins),
        (None, Mapping::Branch(map, ctx)) => Ok(DirEntry::Branch(
            Option::from(map(branch, ctx)).ok_or(ErrorKind::NotFound)?,
        )),
        (None, Mapping::Leaf(map, ctx)) => Ok(DirEntry::Leaf(
            Option::from(map(branch, ctx)).ok_or(ErrorKind::NotFound)?,
        )),
        _ => Err(ErrorKind::NotFound.into()),
    }
}

pub fn forward_entry(
    branch: impl Branch + 'static,
    ctx: CArc<c_void>,
    path: Option<&str>,
    plugins: &CPluginStore,
) -> Result<DirEntry> {
    if let Some(path) = path {
        branch.get_entry(path, plugins)
    } else {
        Ok(DirEntry::Branch(trait_obj!((branch, ctx) as Branch)))
    }
}

pub fn get_entry<T: Branch + StableAbi>(
    branch: &T,
    path: &str,
    plugins: &CPluginStore,
) -> Result<DirEntry> {
    let (local, remote) = split_path(path);

    let entry = plugins
        .lookup_entry::<T>(local)
        .ok_or(ErrorKind::NotFound)?;

    map_entry(branch, &entry, remote, plugins)
}

pub fn list<T: Branch + StableAbi>(
    branch: &T,
    plugins: &CPluginStore,
) -> Result<HashMap<String, DirEntry>> {
    let mut ret = vec![];

    plugins.entry_list::<T>(
        (&mut |(name, entry): (&str, &Mapping<T>)| {
            ret.push(
                map_entry(branch, entry, None, plugins).map(|entry| (name.to_string(), entry)),
            );
            true
        })
            .into(),
    );

    Ok(ret.into_iter().filter_map(Result::ok).collect())
}
