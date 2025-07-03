use core::ops::Deref;

use crate::queue::BBQueue;

use super::{coordination::Coord, notifier::Notifier, storage::Storage};

pub trait BbqHandle {
    type Target: Deref<Target = BBQueue<Self::Storage, Self::Coord, Self::Notifier>> + Clone;
    type Storage: Storage;
    type Coord: Coord;
    type Notifier: Notifier;
    fn bbq_ref(&self) -> Self::Target;
}

impl<S: Storage, C: Coord, N: Notifier> BbqHandle for &'_ BBQueue<S, C, N> {
    type Target = Self;
    type Storage = S;
    type Coord = C;
    type Notifier = N;

    #[inline(always)]
    fn bbq_ref(&self) -> Self::Target {
        *self
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> BbqHandle for std::sync::Arc<BBQueue<S, C, N>> {
    type Target = Self;
    type Storage = S;
    type Coord = C;
    type Notifier = N;

    #[inline(always)]
    fn bbq_ref(&self) -> Self::Target {
        self.clone()
    }
}
