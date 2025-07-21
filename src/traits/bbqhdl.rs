//! "Access" functionality
//!
//! This trait allows us to be generic over things like whether the
//! BBQueue is stored as a static (and we use a `&'static` reference
//! to it), or if the BBQueue is stored in an `Arc`, and we clone the
//! Arc when creating producers and consumers.
//!
//! While `storage` is where/how the *data* is stored, `bbqhdl` is where
//! the shared *header* is stored.
//!
//! The `BbqHandle` trait also serves to "bundle" all the other generics
//! into a single trait with associated types, meaning MOST of the time
//! you code only needs to be generic over `Q: BbqHandle`, and not all four
//! generic types. You can still set trait bounds in where clauses for things
//! like "has async notifications" by using additional `where`-clause like
//! `where Q::BbqHandle, Q::Coord: AsyncCoord`.

use core::{marker::PhantomData, ops::Deref};

use crate::{
    prod_cons::{
        framed::{FramedConsumer, FramedProducer, LenHeader},
        stream::{StreamConsumer, StreamProducer},
    },
    queue::BBQueue,
};

use super::{coordination::Coord, notifier::Notifier, storage::Storage};

/// The "Access" trait
pub trait BbqHandle: Sized {
    /// How we reference our BBQueue.
    type Target: Deref<Target = BBQueue<Self::Storage, Self::Coord, Self::Notifier>> + Clone;
    /// How the DATA of our BBQueue is stored
    type Storage: Storage;
    /// How the producers/consumers of this BBQueue coordinate
    type Coord: Coord;
    /// How we notify the producer/consumers of this BBQueue
    type Notifier: Notifier;

    // Obtain a reference to
    fn bbq_ref(&self) -> Self::Target;

    fn stream_producer(&self) -> StreamProducer<Self> {
        StreamProducer {
            bbq: self.bbq_ref(),
        }
    }

    fn stream_consumer(&self) -> StreamConsumer<Self> {
        StreamConsumer {
            bbq: self.bbq_ref(),
        }
    }

    fn framed_producer<H: LenHeader>(&self) -> FramedProducer<Self, H> {
        FramedProducer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }

    fn framed_consumer<H: LenHeader>(&self) -> FramedConsumer<Self, H> {
        FramedConsumer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }
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
