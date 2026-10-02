use core::marker::PhantomData;

use crate::{
    prod_cons::{
        framed::{FramedConsumer, FramedProducer},
        stream::{StreamConsumer, StreamProducer},
    },
    traits::{
        coordination::Coord,
        notifier::Notifier,
        storage::{ConstStorage, Storage},
    },
};

#[cfg(feature = "alloc")]
use crate::traits::bbqhdl::BbqHandle;

/// A standard bbqueue
pub struct BBQueue<S, C, N> {
    pub(crate) sto: S,
    pub(crate) cor: C,
    pub(crate) not: N,
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    /// Create a new [`BBQueue`] with the given [`Storage`] impl
    ///
    /// This is a `const fn`, so a queue can be placed in a `static` with a
    /// [`Storage`] that is not [`ConstInit`](const_init::ConstInit) — for
    /// example one that carries a pointer to a buffer declared elsewhere.
    /// [`BBQueue::new`] covers the case where the storage can initialize
    /// itself, such as [`Inline<N>`](crate::traits::storage::Inline).
    pub const fn new_with_storage(sto: S) -> Self {
        Self {
            sto,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

/// A BBQueue wrapped in an Arc
#[cfg(feature = "alloc")]
pub struct ArcBBQueue<S, C, N>(pub(crate) alloc::sync::Arc<BBQueue<S, C, N>>);

#[cfg(feature = "alloc")]
impl<S: Storage, C: Coord, N: Notifier> ArcBBQueue<S, C, N> {
    /// Create a new [`BBQueue`] with the given [`Storage`] impl
    pub fn new_with_storage(sto: S) -> Self {
        Self(alloc::sync::Arc::new(BBQueue::new_with_storage(sto)))
    }
}

#[allow(clippy::new_without_default)]
impl<S: ConstStorage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    /// Create a new `BBQueue` in a const context
    pub const fn new() -> Self {
        Self {
            sto: S::INIT,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    /// Create a new [`FramedProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn framed_producer(&self) -> FramedProducer<&'_ Self> {
        FramedProducer {
            bbq: self,
            pd: PhantomData,
        }
    }

    /// Create a new [`FramedConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn framed_consumer(&self) -> FramedConsumer<&'_ Self> {
        FramedConsumer {
            bbq: self,
            pd: PhantomData,
        }
    }

    /// Create a new [`StreamProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn stream_producer(&self) -> StreamProducer<&'_ Self> {
        StreamProducer { bbq: self }
    }

    /// Create a new [`StreamConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn stream_consumer(&self) -> StreamConsumer<&'_ Self> {
        StreamConsumer { bbq: self }
    }

    /// Get the total capacity of the buffer, e.g. how much space is present in [`Storage`]
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        // SAFETY: capacity never changes, therefore reading the len is safe
        unsafe {
            self.sto.ptr_len().1
        }
    }

    /// Get access to the internal storage implementation details
    ///
    /// NOTE: Although this method is safe, use of the `Storage` methods are not.
    /// You should *never* attempt to access or modify the underlying data contained
    /// in a storage implementation while the bbqueue is live. That will IMMEDIATELY
    /// lead to undefined behavior.
    ///
    /// As far as I am aware, the only reasonable use for this is for cases where you
    /// have a custom `Storage` implementation that has unique teardown/drop in place
    /// requirements. Treat any uses of this function with *extreme* caution!
    #[inline(always)]
    pub fn storage(&self) -> &S {
        &self.sto
    }
}

#[cfg(feature = "alloc")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    /// Create a new [`FramedProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn framed_producer(&self) -> FramedProducer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        FramedProducer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    /// Create a new [`FramedConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn framed_consumer(&self) -> FramedConsumer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        FramedConsumer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    /// Create a new [`StreamProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn stream_producer(&self) -> StreamProducer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        StreamProducer {
            bbq: self.0.bbq_ref(),
        }
    }

    /// Create a new [`StreamConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn stream_consumer(&self) -> StreamConsumer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        StreamConsumer {
            bbq: self.0.bbq_ref(),
        }
    }

    /// Get the total capacity of the buffer, e.g. how much space is present in [`Storage`]
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        // SAFETY: capacity never changes, therefore reading the len is safe
        unsafe {
            self.0.sto.ptr_len().1
        }
    }

    /// Get access to the internal storage implementation details
    ///
    /// NOTE: Although this method is safe, use of the `Storage` methods are not.
    /// You should *never* attempt to access or modify the underlying data contained
    /// in a storage implementation while the bbqueue is live. That will IMMEDIATELY
    /// lead to undefined behavior.
    ///
    /// As far as I am aware, the only reasonable use for this is for cases where you
    /// have a custom `Storage` implementation that has unique teardown/drop in place
    /// requirements. Treat any uses of this function with *extreme* caution!
    #[inline(always)]
    pub fn storage(&self) -> &S {
        &self.0.sto
    }
}

#[cfg(test)]
mod test {
    use core::{cell::UnsafeCell, ptr::NonNull};

    use crate::traits::{
        coordination::cas::AtomicCoord, notifier::polling::Polling, storage::Inline,
    };

    use super::*;

    type Queue = BBQueue<Inline<4096>, AtomicCoord, Polling>;
    static QUEUE: Queue = BBQueue::new();
    static PRODUCER: FramedProducer<&'static Queue, u16> = QUEUE.framed_producer();
    static CONSUMER: FramedConsumer<&'static Queue, u16> = QUEUE.framed_consumer();

    #[test]
    fn handles() {
        let mut wgr = PRODUCER.grant(16).unwrap();
        wgr.iter_mut().for_each(|w| *w = 123);
        wgr.commit(16);

        let rgr = CONSUMER.read().unwrap();
        assert_eq!(rgr.len(), 16);
        for b in rgr.iter() {
            assert_eq!(*b, 123);
        }
        rgr.release();
    }

    // What follows is one use case: holding several queues of *different*
    // sizes in one container.
    //
    // With `BBQueue::new` the capacity is part of the queue's type, because
    // the only storage that can initialize itself is `Inline<N>`. So
    // `BBQueue<Inline<64>, ..>` and `BBQueue<Inline<4096>, ..>` are unrelated
    // types: they cannot share an array, a slice or a struct field, and code
    // that dispatches between them has to go through `dyn` or a hand-written
    // function pointer — an indirect call on every operation, and no inlining
    // of the queue's body into the caller.
    //
    // Keeping the capacity in a `Storage` that points at a buffer declared
    // elsewhere erases it from the queue's type, which is what makes one
    // container possible. Such a storage cannot implement `ConstInit`, since
    // no single constant could point at every buffer, so `BBQueue::new` is out
    // of reach and `new_with_storage` is the only way in — hence this patch.

    /// A [`Storage`] whose capacity is a field rather than a type parameter.
    struct PtrStorage {
        ptr: NonNull<u8>,
        len: usize,
    }

    // SAFETY: the pointee is a `static` buffer that only the owning queue
    // touches; the queue itself provides the synchronization.
    unsafe impl Sync for PtrStorage {}
    unsafe impl Send for PtrStorage {}

    impl Storage for PtrStorage {
        unsafe fn ptr_len(&self) -> (NonNull<u8>, usize) {
            (self.ptr, self.len)
        }
    }

    /// Backing memory for one queue.
    ///
    /// The size lives in *this* type, where it is still known at compile time
    /// but does not reach the queue's type.
    #[repr(align(4))]
    struct Buffer<const N: usize>(UnsafeCell<[u8; N]>);

    // SAFETY: the buffer is only ever reached through the queue that owns it.
    unsafe impl<const N: usize> Sync for Buffer<N> {}

    impl<const N: usize> Buffer<N> {
        const fn new() -> Self {
            Self(UnsafeCell::new([0; N]))
        }

        const fn storage(&'static self) -> PtrStorage {
            PtrStorage {
                // SAFETY: the address of a `static` is never null and stays
                // valid for the whole program.
                ptr: unsafe { NonNull::new_unchecked(self.0.get().cast()) },
                len: N,
            }
        }
    }

    /// One type for every capacity — that is the whole point.
    type ErasedQueue = BBQueue<PtrStorage, AtomicCoord, Polling>;

    static SMALL_BUF: Buffer<64> = Buffer::new();
    static LARGE_BUF: Buffer<4096> = Buffer::new();

    static SMALL: ErasedQueue = BBQueue::new_with_storage(SMALL_BUF.storage());
    static LARGE: ErasedQueue = BBQueue::new_with_storage(LARGE_BUF.storage());

    /// Two queues of different sizes in one container, reached by plain
    /// reference: no `dyn`, no function pointer, and the compiler can still
    /// see the whole body at each call site.
    ///
    /// This array is what does not compile without the `const` on
    /// [`BBQueue::new_with_storage`].
    static TABLE: [&ErasedQueue; 2] = [&SMALL, &LARGE];

    #[test]
    fn storage_without_const_init_reaches_a_static() {
        assert_eq!(TABLE[0].capacity(), 64);
        assert_eq!(TABLE[1].capacity(), 4096);

        for (index, queue) in TABLE.iter().enumerate() {
            let producer: FramedProducer<&ErasedQueue, u16> = queue.framed_producer();
            let consumer: FramedConsumer<&ErasedQueue, u16> = queue.framed_consumer();

            let mut wgr = producer.grant(8).unwrap();
            wgr.iter_mut().for_each(|w| *w = index as u8);
            wgr.commit(8);

            let rgr = consumer.read().unwrap();
            assert_eq!(rgr.len(), 8);
            assert!(rgr.iter().all(|b| *b == index as u8));
            rgr.release();
        }
    }
}
