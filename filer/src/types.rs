use abi_stable::StableAbi;
use cglue::prelude::v1::*;

#[derive(StableAbi, Debug, Clone, Copy)]
#[repr(C)]
pub struct CTup2<A, B>(pub A, pub B);

impl<A, B> From<(A, B)> for CTup2<A, B> {
    fn from((a, b): (A, B)) -> Self {
        Self(a, b)
    }
}

impl<A, B> From<CTup2<A, B>> for (A, B) {
    fn from(CTup2(a, b): CTup2<A, B>) -> Self {
        (a, b)
    }
}

pub type Size = u64;

pub type RWData<'a> = CTup2<Size, CSliceMut<'a, u8>>;
pub type ROData<'a> = CTup2<Size, CSliceRef<'a, u8>>;

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
