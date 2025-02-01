#[cfg(feature = "maitake-sync-0_2")]
pub mod maitake;

pub mod blocking;

pub trait Notifier {
    const INIT: Self;

    fn wake_one_consumer(&self);
    fn wake_one_producer(&self);
}

#[allow(async_fn_in_trait)]
pub trait AsyncNotifier: Notifier {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, f: F) -> T;
    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, f: F) -> T;
}
