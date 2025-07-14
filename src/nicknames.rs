//! BBQueue Styles: Sixteen great flavors!
//!
//! | Storage | Coordination     | Notifier | Arc?   | Nickname     | Source        |
//! | :---    | :---             | :---     | :---   | :---         | :---          |
//! | Inline  | Critical Section | Blocking | No     | Jerk         | Jamaica       |
//! | Inline  | Critical Section | Blocking | Yes    | Asado        | Argentina     |
//! | Inline  | Critical Section | Async    | No     | Memphis      | USA           |
//! | Inline  | Critical Section | Async    | Yes    | Carolina     | USA           |
//! | Inline  | Atomic           | Blocking | No     | Churrasco    | Brazil        |
//! | Inline  | Atomic           | Blocking | Yes    | Barbacoa     | Mexico        |
//! | Inline  | Atomic           | Async    | No     | Texas        | USA           |
//! | Inline  | Atomic           | Async    | Yes    | KansasCity   | USA           |
//! | Heap    | Critical Section | Blocking | No     | Braai        | South Africa  |
//! | Heap    | Critical Section | Blocking | Yes    | Kebab        | Türkiye       |
//! | Heap    | Critical Section | Async    | No     | SiuMei       | Hong Kong     |
//! | Heap    | Critical Section | Async    | Yes    | Satay        | SE Asia       |
//! | Heap    | Atomic           | Blocking | No     | YakiNiku     | Japan         |
//! | Heap    | Atomic           | Blocking | Yes    | GogiGui      | South Korea   |
//! | Heap    | Atomic           | Async    | No     | Tandoori     | India         |
//! | Heap    | Atomic           | Async    | Yes    | Lechon       | Philippines   |

#![allow(unused_imports)]

#[cfg(feature = "std")]
use crate::queue::ArcBBQueue;
#[cfg(target_has_atomic = "ptr")]
use crate::traits::coordination::cas::AtomicCoord;
#[cfg(feature = "critical-section")]
use crate::traits::coordination::cs::CsCoord;
#[cfg(feature = "std")]
use crate::traits::storage::BoxedSlice;
use crate::{
    queue::BBQueue,
    traits::{notifier::blocking::Blocking, storage::Inline},
};

/// Inline Storage, Critical Section, Blocking, Borrowed
#[cfg(feature = "critical-section")]
pub type Jerk<const N: usize> = BBQueue<Inline<N>, CsCoord, Blocking>;

/// Inline Storage, Critical Section, Async, Borrowed
#[cfg(feature = "critical-section")]
pub type Memphis<const N: usize, A> = BBQueue<Inline<N>, CsCoord, A>;

/// Inline Storage, Atomics, Blocking, Borrowed
#[cfg(target_has_atomic = "ptr")]
pub type Churrasco<const N: usize> = BBQueue<Inline<N>, AtomicCoord, Blocking>;

/// Inline Storage, Atomics, Async, Borrowed
#[cfg(target_has_atomic = "ptr")]
pub type Texas<const N: usize, A> = BBQueue<Inline<N>, AtomicCoord, A>;

/// Heap Buffer, Critical Section, Blocking, Borrowed
#[cfg(all(feature = "std", feature = "critical-section"))]
pub type Braai = BBQueue<BoxedSlice, CsCoord, Blocking>;

/// Heap Buffer, Critical Section, Async, Borrowed
#[cfg(all(feature = "std", feature = "critical-section"))]
pub type SiuMei<A> = BBQueue<BoxedSlice, CsCoord, A>;

/// Heap Buffer, Atomics, Blocking, Borrowed
#[cfg(all(feature = "std", target_has_atomic = "ptr"))]
pub type YakiNiku = BBQueue<BoxedSlice, AtomicCoord, Blocking>;

/// Heap Buffer, Atomics, Async, Borrowed
#[cfg(all(feature = "std", target_has_atomic = "ptr"))]
pub type Tandoori<A> = BBQueue<BoxedSlice, AtomicCoord, A>;

/// Inline Storage, Critical Section, Blocking, Arc
#[cfg(all(feature = "std", feature = "critical-section"))]
pub type Asado<const N: usize> = ArcBBQueue<Inline<N>, CsCoord, Blocking>;

/// Inline Storage, Critical Section, Async, Arc
#[cfg(all(feature = "std", feature = "critical-section"))]
pub type Carolina<const N: usize, A> = ArcBBQueue<Inline<N>, CsCoord, A>;

/// Inline Storage, Atomics, Blocking, Arc
#[cfg(all(feature = "std", target_has_atomic = "ptr"))]
pub type Barbacoa<const N: usize> = ArcBBQueue<Inline<N>, AtomicCoord, Blocking>;

/// Inline Storage, Atomics, Async, Arc
#[cfg(all(feature = "std", target_has_atomic = "ptr"))]
pub type KansasCity<const N: usize, A> = ArcBBQueue<Inline<N>, AtomicCoord, A>;

/// Heap Buffer, Critical Section, Blocking, Arc
#[cfg(all(feature = "std", feature = "critical-section"))]
pub type Kebab = ArcBBQueue<BoxedSlice, CsCoord, Blocking>;

/// Heap Buffer, Critical Section, Async, Arc
#[cfg(all(feature = "std", feature = "critical-section"))]
pub type Satay<A> = ArcBBQueue<BoxedSlice, CsCoord, A>;

/// Heap Buffer, Atomics, Blocking, Arc
#[cfg(all(feature = "std", target_has_atomic = "ptr"))]
pub type GogiGui = ArcBBQueue<BoxedSlice, AtomicCoord, Blocking>;

/// Heap Buffer, Atomics, Async, Arc
#[cfg(all(feature = "std", target_has_atomic = "ptr"))]
pub type Lechon<A> = ArcBBQueue<BoxedSlice, AtomicCoord, A>;
