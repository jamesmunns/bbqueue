use super::Notifier;

pub struct Blocking;

// Blocking performs no notification
impl Notifier for Blocking {
    const INIT: Self = Blocking;
    fn wake_one_consumer(&self) {}
    fn wake_one_producer(&self) {}
}
