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
//!   in particular, the [`sync`] module, and `Notifier: Sync` (when possible).

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
pub use store::{PoisonError, Store, StoreLock, StoreLockListener};

mod util;
pub use util::*;

// -------------------------------------------------------------------------------------------------

/// Type aliases for use when listeners are expected to implement [`Sync`].
#[cfg(feature = "sync")]
#[path = "sync_or_not/"]
#[allow(clippy::duplicate_mod)]
pub mod sync {
    #[cfg(doc)]
    use crate::unsync;

    /// Type-erased form of a [`Listener`] which accepts messages of type `M`.
    ///
    /// This type is [`Send`] and [`Sync`]. When that is not satisfiable, use
    /// [`unsync::DynListener`] instead.
    pub type DynListener<M> = alloc::sync::Arc<dyn crate::Listener<M> + Send + Sync>;

    /// Message broadcaster.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires all its [`Listener`]s to be so.
    /// When this requirement is undesired, use [`unsync::Notifier`] instead.
    pub type Notifier<M> = crate::Notifier<M, DynListener<M>>;
}

/// Type aliases for use when listeners are not expected to implement [`Sync`].
#[path = "sync_or_not/"]
#[allow(clippy::duplicate_mod)]
#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
pub mod unsync {
    #[cfg(all(doc, feature = "sync"))]
    use crate::sync;

    /// Type-erased form of a [`Listener`] which accepts messages of type `M`.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::DynListener`] instead.
    //---
    // TODO: try making this only Rc instead of Arc
    pub type DynListener<M> = alloc::sync::Arc<dyn crate::Listener<M>>;

    /// Message broadcaster.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::Notifier`] instead.
    pub type Notifier<M> = crate::Notifier<M, DynListener<M>>;
}

// mod sync_if_possible {
//     #[cfg(feature = "sync")]
//     pub(crate) use crate::sync::*;
//     #[cfg(not(feature = "sync"))]
//     pub(crate) use crate::unsync::*;
// }

// -------------------------------------------------------------------------------------------------
