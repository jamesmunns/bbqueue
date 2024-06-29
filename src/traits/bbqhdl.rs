use core::ops::Deref;

use crate::queue::BBQueue;

use super::{coordination::Coord, notifier::Notifier, storage::Storage};

pub trait BbqHandle<S: Storage, C: Coord, N: Notifier> {
    type Target: Deref<Target = BBQueue<S, C, N>> + Clone;
    fn bbq_ref(&self) -> Self::Target;
}

impl<'a, S: Storage, C: Coord, N: Notifier> BbqHandle<S, C, N> for &'a BBQueue<S, C, N> {
    type Target = Self;

    #[inline(always)]
    fn bbq_ref(&self) -> Self::Target {
        *self
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> BbqHandle<S, C, N> for std::sync::Arc<BBQueue<S, C, N>> {
    type Target = std::sync::Arc<BBQueue<S, C, N>>;

    #[inline(always)]
    fn bbq_ref(&self) -> Self::Target {
        self.clone()
    }
}
