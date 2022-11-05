use core::task::Waker;

/// A waker storage. Can be initialized without a waker, and a waker can be set on an eventual `poll` call.
/// The waker can be set and woken up.
#[derive(Debug)]
pub struct WakerStorage {
    waker: Option<Waker>,
}

impl WakerStorage {
    pub const fn new() -> Self {
        WakerStorage { waker: None }
    }

    /// Set the waker, will wake the previous one if one was already stored.
    pub fn set(&mut self, new: &Waker) {
        match self.waker.take() {
            Some(prev) => {
                // No need to clone if they wake the same task.
                if prev.will_wake(new) {
                    return;
                }
                // Wake the previous waker and replace it
                else {
                    prev.wake();
                    self.waker.replace(new.clone());
                }
            }
            None => {
                self.waker.replace(new.clone());
            }
        }
    }

    /// Wake the waker if one is available
    pub fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake()
        }
    }
}
