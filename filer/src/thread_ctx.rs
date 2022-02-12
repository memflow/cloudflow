use abi_stable::StableAbi;
use cglue::prelude::v1::*;
pub use cglue::slice::CSliceMut;
use cglue::trait_group::c_void;
use core::mem::MaybeUninit;

#[derive(StableAbi)]
#[repr(C)]
pub struct ThreadCtx<T: 'static> {
    orig: T,
    stack: CBox<'static, c_void>,
    stack_push: for<'a> extern "C" fn(&c_void, COption<T>),
    stack_pop: for<'a> extern "C" fn(&c_void, &mut MaybeUninit<COption<T>>) -> bool,
}

pub struct ThreadCtxHandle<'a, T: 'static> {
    value: MaybeUninit<T>,
    ctx: &'a ThreadCtx<T>,
}

impl<T> Drop for ThreadCtxHandle<'_, T> {
    fn drop(&mut self) {
        self.ctx.push(Some(unsafe { self.value.as_ptr().read() }))
    }
}

impl<T> core::ops::Deref for ThreadCtxHandle<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.value.as_ptr().as_ref().unwrap() }
    }
}

impl<T> core::ops::DerefMut for ThreadCtxHandle<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.value.as_mut_ptr().as_mut().unwrap() }
    }
}

impl<T> ThreadCtx<T> {
    pub fn new(orig: T, size: usize) -> Self {
        // SAFETY: All types in the opaque functions match!!! It is safe, but needs care!!!

        let stack = crossbeam_deque::Worker::<COption<T>>::new_lifo();

        for _ in 0..size {
            stack.push(COption::None);
        }

        let stack = CBox::from(stack).into_opaque();

        extern "C" fn stack_pop<T>(stack: &c_void, out: &mut MaybeUninit<COption<T>>) -> bool {
            match unsafe {
                (*(stack as *const _ as *const crossbeam_deque::Worker<COption<T>>)).pop()
            } {
                Some(t) => {
                    out.write(t);
                    true
                }
                None => false,
            }
        }

        extern "C" fn stack_push<T>(stack: &c_void, val: COption<T>) {
            unsafe {
                (*(stack as *const _ as *const crossbeam_deque::Worker<COption<T>>)).push(val)
            };
        }

        Self {
            orig,
            stack,
            stack_pop: stack_pop::<T>,
            stack_push: stack_push::<T>,
        }
    }

    fn push(&self, val: Option<T>) {
        (self.stack_push)(&*self.stack, val.into())
    }

    fn pop(&self) -> Option<Option<T>> {
        let mut out = MaybeUninit::uninit();
        if (self.stack_pop)(&*self.stack, &mut out) {
            Some(unsafe { out.assume_init() }.into())
        } else {
            None
        }
    }
}

impl<T: Clone> ThreadCtx<T> {
    pub fn get(&self) -> ThreadCtxHandle<T> {
        let v = loop {
            match self.pop() {
                Some(Some(v)) => break v,
                Some(None) => break self.orig.clone(),
                None => {}
            }
        };

        ThreadCtxHandle {
            value: MaybeUninit::new(v),
            ctx: self,
        }
    }
}
