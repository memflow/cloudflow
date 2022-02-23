use crate::error::*;

pub trait StrBuild<C>: Sized {
    fn build(input: &str, ctx: &C) -> Result<Self>;
}
