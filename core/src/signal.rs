// Copyright The Embassy Project (https://github.com/akiles/embassy). Licensed under the Apache 2.0
// license

use core::cell::UnsafeCell;
use core::mem;
use core::task::{Context, Poll, Waker};

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
fn with_critical_section<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    use std::sync::Once;
    static INIT: Once = Once::new();
    static mut BKL: Option<std::sync::Mutex<()>> = None;

    INIT.call_once(|| unsafe {
        BKL.replace(std::sync::Mutex::new(()));
    });

    let _guard = unsafe { BKL.as_ref().unwrap().lock() };
    f()
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
fn with_critical_section<F, R>(&self, f: F) -> R
where
    F: FnOnce() -> R,
{
    cortex_m::interrupt::free(|_| f())
}

pub struct Signal<T> {
    state: UnsafeCell<State<T>>,
}

enum State<T> {
    None,
    Waiting(Waker),
    Signaled(T),
}

unsafe impl<T: Sized> Send for Signal<T> {}
unsafe impl<T: Sized> Sync for Signal<T> {}

impl<T: Sized> Signal<T> {
    pub const fn new() -> Self {
        Self {
            state: UnsafeCell::new(State::None),
        }
    }

    #[allow(clippy::single_match)]
    pub fn signal(&self, val: T) {
        with_critical_section(|| unsafe {
            let state = &mut *self.state.get();
            match mem::replace(state, State::Signaled(val)) {
                State::Waiting(waker) => waker.wake(),
                _ => {}
            }
        })
    }

    pub fn poll_wait(&self, cx: &mut Context<'_>) -> Poll<T> {
        with_critical_section(|| unsafe {
            let state = &mut *self.state.get();
            match state {
                State::None => {
                    *state = State::Waiting(cx.waker().clone());
                    Poll::Pending
                }
                State::Waiting(w) if w.will_wake(cx.waker()) => Poll::Pending,
                State::Waiting(_) => Poll::Pending,
                State::Signaled(_) => match mem::replace(state, State::None) {
                    State::Signaled(res) => Poll::Ready(res),
                    _ => Poll::Pending,
                },
            }
        })
    }
}
