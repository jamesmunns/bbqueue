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

use crate::{
    queue::{ArcBBQueue, BBQueue},
    traits::{
        coordination::{cas::AtomicCoord, CsCoord},
        notifier::{Blocking, MaiNotSpsc},
        storage::{BoxedSlice, Inline},
    },
};


/// Inline Storage, Critical Section, Blocking, Borrowed
pub type Jerk<const N: usize> = BBQueue<Inline<N>, CsCoord, Blocking>;

/// Inline Storage, Critical Section, Async, Borrowed
pub type Memphis<const N: usize, A = MaiNotSpsc> = BBQueue<Inline<N>, CsCoord, A>;

/// Inline Storage, Atomics, Blocking, Borrowed
#[cfg(feature = "cas-atomics")]
pub type Churrasco<const N: usize> = BBQueue<Inline<N>, AtomicCoord, Blocking>;

/// Inline Storage, Atomics, Async, Borrowed
#[cfg(feature = "cas-atomics")]
pub type Texas<const N: usize, A = MaiNotSpsc> = BBQueue<Inline<N>, AtomicCoord, A>;


/// Heap Buffer, Critical Section, Blocking, Borrowed
#[cfg(feature = "std")]
pub type Braai = BBQueue<BoxedSlice, CsCoord, Blocking>;

/// Heap Buffer, Critical Section, Async, Borrowed
#[cfg(feature = "std")]
pub type SiuMei<A = MaiNotSpsc> = BBQueue<BoxedSlice, CsCoord, A>;

/// Heap Buffer, Atomics, Blocking, Borrowed
#[cfg(all(feature = "std", feature = "cas-atomics"))]
pub type YakiNiku = BBQueue<BoxedSlice, AtomicCoord, Blocking>;

/// Heap Buffer, Atomics, Async, Borrowed
#[cfg(all(feature = "std", feature = "cas-atomics"))]
pub type Tandoori<A = MaiNotSpsc> = BBQueue<BoxedSlice, AtomicCoord, A>;


/// Inline Storage, Critical Section, Blocking, Arc
#[cfg(feature = "std")]
pub type Asado<const N: usize> = ArcBBQueue<Inline<N>, CsCoord, Blocking>;

/// Inline Storage, Critical Section, Async, Arc
#[cfg(feature = "std")]
pub type Carolina<const N: usize, A = MaiNotSpsc> = ArcBBQueue<Inline<N>, CsCoord, A>;

/// Inline Storage, Atomics, Blocking, Arc
#[cfg(all(feature = "std", feature = "cas-atomics"))]
pub type Barbacoa<const N: usize> = ArcBBQueue<Inline<N>, AtomicCoord, Blocking>;

/// Inline Storage, Atomics, Async, Arc
#[cfg(all(feature = "std", feature = "cas-atomics"))]
pub type KansasCity<const N: usize, A = MaiNotSpsc> = ArcBBQueue<Inline<N>, AtomicCoord, A>;


/// Heap Buffer, Critical Section, Blocking, Arc
#[cfg(feature = "std")]
pub type Kebab = ArcBBQueue<BoxedSlice, CsCoord, Blocking>;

/// Heap Buffer, Critical Section, Async, Arc
#[cfg(feature = "std")]
pub type Satay<A = MaiNotSpsc> = ArcBBQueue<BoxedSlice, CsCoord, A>;

/// Heap Buffer, Atomics, Blocking, Arc
#[cfg(all(feature = "std", feature = "cas-atomics"))]
pub type GogiGui = ArcBBQueue<BoxedSlice, AtomicCoord, Blocking>;

/// Heap Buffer, Atomics, Async, Arc
#[cfg(all(feature = "std", feature = "cas-atomics"))]
pub type Lechon<A = MaiNotSpsc> = ArcBBQueue<BoxedSlice, AtomicCoord, A>;
