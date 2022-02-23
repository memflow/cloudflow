use crate::error::Error;
use abi_stable::StableAbi;
use cglue::prelude::v1::*;
use cglue::trait_group::c_void;
use core::num::NonZeroI32;
use sharded_slab::{Entry, Slab};

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

pub trait SetContext {
    fn set_context(&mut self, ctx: &CArc<c_void>);
}

use core::sync::atomic::{AtomicUsize, Ordering};

pub struct RcSlabEntry<'a, T> {
    entry: Entry<'a, (AtomicUsize, T)>,
}

impl<'a, T> From<Entry<'a, (AtomicUsize, T)>> for RcSlabEntry<'a, T> {
    fn from(entry: Entry<'a, (AtomicUsize, T)>) -> Self {
        Self { entry }
    }
}

impl<T> core::ops::Deref for RcSlabEntry<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.entry.1
    }
}

pub struct RcSlab<T> {
    slab: Slab<(AtomicUsize, T)>,
}

impl<T> Default for RcSlab<T> {
    fn default() -> Self {
        Self {
            slab: Default::default(),
        }
    }
}

impl<T> RcSlab<T> {
    pub fn insert(&self, val: T) -> Option<usize> {
        self.slab.insert((1.into(), val))
    }

    pub fn get(&self, idx: usize) -> Option<RcSlabEntry<T>> {
        self.slab.get(idx).map(<_>::into)
    }

    pub fn dec_rc(&self, idx: usize) -> Option<Option<RcSlabEntry<T>>> {
        self.slab.get(idx).map(|entry| {
            if entry.0.fetch_sub(1, Ordering::Relaxed) == 1 {
                self.slab.remove(idx);
                Some(entry.into())
            } else {
                None
            }
        })
    }

    pub fn inc_rc(&self, idx: usize) -> Option<()> {
        self.slab.get(idx).and_then(|entry| {
            let mut prev = entry.0.load(Ordering::Relaxed);
            while prev != 0 {
                match entry.0.compare_exchange_weak(
                    prev,
                    prev + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(p) => {
                        prev = p;
                        break;
                    }
                    Err(p) => prev = p,
                }
            }

            if prev == 0 {
                None
            } else {
                Some(())
            }
        })
    }
}
