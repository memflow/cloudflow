pub use cglue::slice::CSliceMut;

use filer::prelude::v1::*;
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

pub fn memdata_map<B, F: FnOnce(MemOps<CTup3<Address, Address, B>, CTup2<Address, B>>) -> O, O>(
    VecOps { inp, out, out_fail }: VecOps<CTup2<Size, B>>,
    func: F,
) -> O {
    let inp = inp.map(|CTup2(a, b)| CTup3(a.into(), a.into(), b));
    let mut out =
        out.map(|c| |CTup2(a, b): CTup2<Address, B>| c.call(CTup2(a.to_umem() as Size, b)));
    let mut out = out.as_mut().map(<_>::into);
    let mut out_fail = out_fail.map(|c| {
        |CTup2(a, b): CTup2<Address, B>| {
            c.call(
                (
                    CTup2(a.to_umem() as Size, b),
                    filer::error::Error(
                        filer::error::ErrorOrigin::Other,
                        filer::error::ErrorKind::Unknown,
                    ),
                )
                    .into(),
            )
        }
    });
    let mut out_fail = out_fail.as_mut().map(<_>::into);
    MemOps::with_raw(inp, out.as_mut(), out_fail.as_mut(), func)
}
