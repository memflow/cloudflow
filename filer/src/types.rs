use crate::error::Error;
use abi_stable::StableAbi;
use cglue::prelude::v1::*;
use core::num::NonZeroI32;

pub type Size = u64;

pub type RWData<'a> = CTup2<Size, CSliceMut<'a, u8>>;
pub type ROData<'a> = CTup2<Size, CSliceRef<'a, u8>>;

#[repr(C)]
#[derive(StableAbi)]
pub struct FailData<T>(T, NonZeroI32);

impl<T> From<(T, Error)> for FailData<T> {
    fn from((d, e): (T, Error)) -> Self {
        Self(d, e.into_int_err())
    }
}

impl<T> From<FailData<T>> for (T, Error) {
    fn from(FailData(d, e): FailData<T>) -> Self {
        (d, Error::from_int_err(e))
    }
}

pub type RWFailData<'a> = FailData<RWData<'a>>;
pub type ROFailData<'a> = FailData<ROData<'a>>;

/*pub type RWCallback<'a, 'b> = OpaqueCallback<'a, RWData<'b>>;
pub type ROCallback<'a, 'b> = OpaqueCallback<'a, ROData<'b>>;
pub type RWFailCallback<'a, 'b> = OpaqueCallback<'a, RWFailData<'b>>;
pub type ROFailCallback<'a, 'b> = OpaqueCallback<'a, ROFailData<'b>>;*/

#[repr(C)]
#[derive(StableAbi)]
pub struct VecOps<'a, T: 'a> {
    pub inp: CIterator<'a, T>,
    pub out: Option<&'a mut OpaqueCallback<'a, T>>,
    pub out_fail: Option<&'a mut OpaqueCallback<'a, FailData<T>>>,
}

impl<'a, T, I: Into<CIterator<'a, T>>> From<I> for VecOps<'a, T> {
    fn from(inp: I) -> Self {
        Self {
            inp: inp.into(),
            out: None,
            out_fail: None,
        }
    }
}

pub fn opt_call<T>(cb: Option<&mut OpaqueCallback<T>>, data: T) -> bool {
    cb.map(|cb| cb.call(data)).unwrap_or(true)
}

pub trait ArcType: Sized + 'static {
    type ArcSelf: Into<CArcSome<Self>>;

    fn arc_up(self) -> CArcSome<Self> {
        self.into()
    }

    fn self_arc_up(self) -> Self::ArcSelf
    where
        CArcSome<Self>: Into<Self::ArcSelf>,
    {
        CArcSome::from(self).into()
    }

    fn into_arc(arc: Self::ArcSelf) -> CArcSome<Self> {
        arc.into()
    }

    fn from_arc(arc: CArcSome<Self>) -> Self::ArcSelf
    where
        CArcSome<Self>: Into<Self::ArcSelf>,
    {
        arc.into()
    }
}

#[derive(StableAbi, Default, Clone, Copy, Debug)]
#[repr(C)]
pub struct NodeMetadata {
    pub is_branch: bool,
    pub has_read: bool,
    pub has_write: bool,
    pub has_rpc: bool,
    pub size: Size,
}

impl NodeMetadata {
    pub fn branch() -> Self {
        Self {
            is_branch: true,
            ..Default::default()
        }
    }
}
