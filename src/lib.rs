#![no_std]

//! Library for broadcasting messages/events such as change notifications.
//!
//! # What this library does
//!
//! The niche which this library seeks to fill is: delivering precise change notifications
//! (e.g. “these particular elements of this collection have changed”) from a data source
//! to a set of listeners (observers) in such a way that there is
//!
//! * no unbounded buffering of messages (as an unbounded channel would have),
//! * no blocking/suspending (as a bounded channel would have),
//! * and yet also, no execution of further application logic while the message is being delivered
//!   (as plain event-listener registration would have).
//!
//! The tradeoff we make in order to achieve this is that message delivery does involve execution
//! of a *small* amount of code on behalf of each [`Listener`];
//! this code is responsible for deciding whether the message is of interest, and if so, storing it
//! for later reading.
//! For example, a listener might `match` a message enum, and in cases where it is of interest,
//! [set an `AtomicBool` to true](DirtyFlag). The final recipient would then use that boolean flag
//! to determine that it needs to re-read the data which the messages are about.
//! Thus, the message itself need not be stored until the final recipient gets to it,
//! and multiple messages are de-duplicated.
//!
//! In a loose sense, each listener is itself a sort of channel sender;
//! it is written when the message is sent, and to be useful, it must have shared state with
//! some receiving side which reads (and clears) that shared state.
//! Because of this, this library is not a good choice if you expect to have very many listeners
//! of the same character (e.g. many identical worker tasks updating their state); in those cases,
//! you would probably be better off using a conventional broadcast channel or watch channel.
//!
//! # Getting started
//!
//! To send messages, create a [`Notifier`], which manages a collection of [`Listener`]s.
//!
//! To receive messages, create a [`Listener`], then use the [`Listen`] trait to register it with
//! a [`Notifier`] or something which contains a [`Notifier`].
//! When possible, you should use existing [`Listener`] implementations such as [`DirtyFlag`] or
//! [`StoreLock`] which have been designed to be well-behaved, but it is also reasonable
//! to write your own implementation, as long as it obeys the documented requirements.
//!
//! # Features and platform requirements
//!
//! The minimum requirements for using this library are:
//!
//! * The `alloc` standard library crate, and a global allocator.
//! * Support for pointer-sized atomics (`cfg(target_has_atomic = "ptr")`).
//!
//! The following Cargo feature flags are defined:
//!
//! * `"std"`:
//!   Enable implementations of our traits for `std` types.
//!   Without this feature, only `core` and `alloc` types are used.
//! * `"sync"`:
//!   Adds [`Sync`] functionality for delivering messages across threads;
//!   in particular, the
#![cfg_attr(feature = "sync", doc = "   [`sync`]")]
#![cfg_attr(not(feature = "sync"), doc = "   `sync`")]
//!   module, and `Notifier: Sync` (when possible).

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]
#![warn(explicit_outlives_requirements)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]
#![warn(redundant_lifetimes)]
#![warn(trivial_casts)]
#![warn(trivial_numeric_casts)]
#![warn(unnameable_types)]
#![warn(unused_extern_crates)]
#![warn(unused_lifetimes)]
#![warn(unreachable_pub)]
#![warn(
    clippy::alloc_instead_of_core,
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc
)]
#![warn(clippy::assigning_clones)]
#![warn(clippy::doc_markdown)]
#![warn(clippy::exhaustive_enums)]
#![warn(clippy::exhaustive_structs)]
#![warn(clippy::inconsistent_struct_constructor)]
#![warn(clippy::large_futures)]
#![warn(clippy::large_stack_frames)]
#![warn(clippy::manual_let_else)]
#![warn(clippy::module_name_repetitions)]
#![warn(clippy::pedantic)]
#![warn(clippy::return_self_not_must_use)]
#![warn(clippy::should_panic_without_expect)]
#![warn(clippy::unnecessary_self_imports)]
#![warn(clippy::unnecessary_wraps)]
#![allow(clippy::bool_assert_comparison, reason = "less legible")]
#![allow(clippy::semicolon_if_nothing_returned, reason = "explicit delegation")]
#![allow(clippy::missing_panics_doc)] // TODO: fix this

// -------------------------------------------------------------------------------------------------

extern crate alloc;

#[cfg(any(feature = "std", feature = "sync", test))]
extern crate std;

// -------------------------------------------------------------------------------------------------

mod listener;
pub use listener::{IntoDynListener, Listen, Listener};

mod listeners;
pub use listeners::*;

mod maybe_sync;

mod notifier;
pub use notifier::{Buffer, Notifier, NotifierForwarder};

mod store;
pub use store::{Store, StoreLock, StoreLockListener};

mod util;
pub use util::*;

// -------------------------------------------------------------------------------------------------

/// Type aliases for use in applications where listeners are expected to implement [`Sync`].
#[cfg(feature = "sync")]
#[path = "sync_or_not/"]
#[allow(clippy::duplicate_mod)]
pub mod sync {
    #[cfg(doc)]
    use crate::unsync;
    use crate::Listener;

    /// Type-erased form of a [`Listener`] which accepts messages of type `M`.
    ///
    /// This type is [`Send`] and [`Sync`]. When that is not satisfiable, use
    /// [`unsync::DynListener`] instead.
    pub type DynListener<M> = alloc::sync::Arc<dyn Listener<M> + Send + Sync>;

    /// Message broadcaster.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires all its [`Listener`]s to be so.
    /// When this requirement is undesired, use [`unsync::Notifier`] instead.
    pub type Notifier<M> = crate::Notifier<M, DynListener<M>>;

    /// A [`Listener`] which forwards messages through a [`Notifier`] to its listeners.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires its [`Notifier`] be so.
    /// When this requirement is undesired, use [`unsync::NotifierForwarder`] instead.
    pub type NotifierForwarder<M> = crate::NotifierForwarder<M, DynListener<M>>;
}

/// Type aliases for use in applications where listeners are not expected to implement [`Sync`].
#[path = "sync_or_not/"]
#[allow(clippy::duplicate_mod)]
#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
pub mod unsync {
    #[cfg(all(doc, feature = "sync"))]
    use crate::sync;
    use crate::Listener;

    /// Type-erased form of a [`Listener`] which accepts messages of type `M`.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::DynListener`] instead.
    //---
    // TODO: try making this only Rc instead of Arc
    pub type DynListener<M> = alloc::sync::Arc<dyn Listener<M>>;

    /// Message broadcaster.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::Notifier`] instead.
    pub type Notifier<M> = crate::Notifier<M, DynListener<M>>;

    /// A [`Listener`] which forwards messages through a [`Notifier`] to its listeners.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::NotifierForwarder`] instead.
    pub type NotifierForwarder<M> = crate::NotifierForwarder<M, DynListener<M>>;
}

// mod sync_if_possible {
//     #[cfg(feature = "sync")]
//     pub(crate) use crate::sync::*;
//     #[cfg(not(feature = "sync"))]
//     pub(crate) use crate::unsync::*;
// }

// -------------------------------------------------------------------------------------------------
