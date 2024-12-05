#![no_std]

//! Library for broadcasting of notifications of state changes, and other messages.
//!
//! To send notifications, create a [`Notifier`], which manages a collection of [`Listener`]s.
//! Each listener reports when it is no longer needed and may be discarded.
//!
//! When [`Notifier::notify()`] is called to send a message, it is synchronously delivered
//! to all listeners; therefore, listeners are obligated to avoid doing significant work or
//! cause cascading state changes.
//! The recommended pattern is to use listener implementors such as [`DirtyFlag`] or [`StoreLock`],
//! which aggregate incoming messages, to be read and cleared by a task running on a more
//! appropriate schedule.

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

#[cfg(any(feature = "std", test))]
extern crate std;

// -------------------------------------------------------------------------------------------------

mod cell;
pub use cell::*;

mod listener;
pub use listener::{DynListener, Listener};

mod listeners;
pub use listeners::*;

mod maybe_sync;

mod notifier;
pub use notifier::{Buffer, Notifier, NotifierForwarder};

mod store;
pub use store::{PoisonError, Store, StoreLock, StoreLockListener};

mod util;
pub use util::*;

// -------------------------------------------------------------------------------------------------

/// Ability to subscribe to a source of messages, causing a [`Listener`] to receive them
/// as long as it wishes to.
pub trait Listen {
    /// The type of message which may be obtained from this source.
    ///
    /// Most message types should satisfy `Copy + Send + Sync + 'static`, but this is not required.
    type Msg;

    /// Subscribe the given [`Listener`] to this source of messages.
    ///
    /// Note that listeners are removed only via their returning [`false`] from
    /// [`Listener::receive()`]; there is no operation to remove a listener,
    /// nor are subscriptions deduplicated.
    fn listen<L: Listener<Self::Msg> + 'static>(&self, listener: L);
}

impl<T: Listen> Listen for &T {
    type Msg = T::Msg;

    fn listen<L: Listener<Self::Msg> + 'static>(&self, listener: L) {
        (**self).listen(listener)
    }
}
impl<T: Listen> Listen for alloc::sync::Arc<T> {
    type Msg = T::Msg;

    fn listen<L: Listener<Self::Msg> + 'static>(&self, listener: L) {
        (**self).listen(listener)
    }
}
