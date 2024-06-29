use crate::traits::{
    coordination::Coord,
    notifier::Notifier,
    storage::{ConstStorage, Storage},
};

pub struct BBQueue<S, C, N> {
    pub(crate) sto: S,
    pub(crate) cor: C,
    pub(crate) not: N,
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub fn new_with_storage(sto: S) -> Self {
        Self {
            sto,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

#[cfg(feature = "std")]
pub struct ArcBBQueue<S, C, N>(pub(crate) std::sync::Arc<BBQueue<S, C, N>>);

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> ArcBBQueue<S, C, N> {
    pub fn new_with_storage(sto: S) -> Self {
        Self(std::sync::Arc::new(BBQueue::new_with_storage(sto)))
    }
}

#[allow(clippy::new_without_default)]
impl<S: ConstStorage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub const fn new() -> Self {
        Self {
            sto: S::INIT,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}
