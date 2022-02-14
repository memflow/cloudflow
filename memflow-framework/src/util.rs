pub use cglue::slice::CSliceMut;

use filer::prelude::v1::*;
pub use memflow::mem::MemData;
use memflow::prelude::v1::*;

/// Splits the connector/os arguments into parts.
///
/// The parts provided are:
///
/// 1. Parent OS/Connector to chain with.
/// 2. Name of the plugin library.
/// 3. Arguments for the plugin.
///
pub fn split_args(input: &str) -> (Option<&str>, &str, &str) {
    let input = input.trim();

    let (chain_with, input) = if input.starts_with("-c ") {
        let input = input.strip_prefix("-c").unwrap().trim();

        input
            .split_once(" ")
            .map(|(a, b)| (Some(a), b))
            .unwrap_or((Some(input), ""))
    } else {
        (None, input)
    };

    let (name, args) = input.split_once(":").unwrap_or((input, ""));
    (chain_with, name, args)
}

pub fn memdata_map<A: Into<memflow::types::Address>, B>(
    iter: impl Iterator<Item = CTup2<A, B>>,
) -> impl Iterator<Item = MemData<memflow::types::Address, B>> {
    iter.map(|CTup2(a, b)| MemData(a.into(), b))
}

pub extern "C" fn self_as_leaf<T: Leaf + Into<LeafBaseBox<'static, T>> + Clone + 'static>(
    obj: &T,
) -> COption<LeafBox<'static>> {
    COption::Some(trait_obj!(obj.clone() as Leaf))
}
