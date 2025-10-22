#![no_std]

//! Library for broadcasting messages/events such as change notifications.
//!
//! # What `nosy` does
//!
//! The niche which `nosy` seeks to fill is: delivering precise change notifications
//! (e.g. “these particular elements of this collection have changed”) from a data source
//! to a set of listeners (observers) in such a way that
//!
//! * there is no unbounded buffering of messages (as an unbounded channel would have),
//! * there is no blocking/suspending (as a bounded channel would have),
//! * there is no execution of further application logic while the message is being delivered
//!   (as plain event-listener registration would have), and
//! * the scheduling of the execution of said application logic is fully under application control
//!   (rather than implicitly executing some sort of work queue, as a “reactive” framework might).
//!
//! The tradeoff we make in order to achieve this is that message delivery does involve execution
//! of a *small* amount of code on behalf of each [`Listener`];
//! this code is responsible for deciding whether the message is of interest, and if so, storing it
//! or its implications for later reading.
//! (We could say that *the listeners are nosy*.)
//! For example, a listener might `match` a message enum, and in cases where it is of interest,
//! [set an `AtomicBool` to true](Flag). The final recipient would then use that boolean flag
//! to determine that it needs to re-read the data which the messages are about.
//! Thus, the message itself need not be stored until the final recipient gets to it,
//! and multiple messages are de-duplicated.
//!
//! In a loose sense, each listener is itself a sort of channel sender;
//! it is written when the message is sent, and to be useful, it must have shared state with
//! some receiving side which reads (and clears) that shared state.
//! Because of this, `nosy` is not a good choice if you expect to have very many listeners
//! of the same character (e.g. many identical worker tasks updating their state); in those cases,
//! you would probably be better off using a conventional broadcast channel or watch channel.
//! It is also not a good choice if it is critical that no third-party code executes on your thread
//! or while your function is running.
//!
//! # Getting started
//!
//! The types in this library are often generic over whether they are <code>[Send] + [Sync]</code>
//! and require the values and listeners they contain to be too. For convenience, a set of
//! less-generic type aliases and functions is available in the [`sync`] and [`unsync`] modules.
//! The following discussion links to the generic versions, but examples will use the non-generic
//! ones.
//!
//! * To send messages, create a [`Notifier`], which manages a collection of [`Listener`]s.
//!
//! * To receive messages, create a [`Listener`], then use the [`Listen`] trait to register it with
//!   a [`Notifier`] or something which contains a [`Notifier`].
//!   When possible, you should use existing [`Listener`] implementations such as [`Flag`] or
//!   [`StoreLock`] which have been designed to be well-behaved, but it is also reasonable
//!   to write your own implementation, as long as it obeys the documented requirements.
//!
//! * To share a value which changes over time and which can be retrieved at any time
//!   (rather than only a stream of messages), use [`Cell`], or implement [`Source`].
//!
//! # Features and platform requirements
//!
//! `nosy` is compatible with `no_std` platforms.
//! The minimum requirements for using `nosy` are the following.
//! (All [platforms which support `std`] meet these requirements, and many others do too.)
//!
//! * The `alloc` standard library crate, and a global allocator.
//! * Pointer-sized and `u8`-sized atomics (`cfg(target_has_atomic = "ptr")` and `cfg(target_has_atomic = "8")`).
//!
//! The following Cargo feature flags are defined:
//!
//! * `"async"`:
//!   Add functionality for `async` programming,
//!   currently consisting of the
#![cfg_attr(feature = "async", doc = "[`future`]")]
#![cfg_attr(not(feature = "async"), doc = "`future`")]
//! module.
//!
//! * `"std"`:
//!   Enable implementations of our traits for [`std`] types,
//!   rather than only [`core`] and [`alloc`] types.
//!
//! * `"std-sync"`:
//!   Makes use of [`std::sync`] to add [`Sync`] to [`Notifier`], [`StoreLock`], and [`Flatten`].
//!
//!   Adds the type aliases <code>nosy::sync::{[Cell], [CellWithLocal], [CellSource]}</code>,
//!   which depend on [`std::sync::Mutex`].
//!
//! * `"spin-sync"`:
//!   Makes use of spinlocks to add [`Sync`] to [`Notifier`], [`StoreLock`], and [`Flatten`].
//!   This is not recommended for general use and is primarily useful when [`Sync`] is required
//!   yet the platform is `no_std` and single-threaded.
//!   If both `"spin-sync"` and `"std-sync"` are enabled, `"std-sync"` takes priority.
//!
//! It is possible to use a subset of `nosy`’s functionality as <code>[Send] + [Sync]</code>
//! without enabling this feature and without depending on `std`.
//! In particular, you may wish to use [`sync::RawNotifier`]; the [`Flag`] and
#![cfg_attr(feature = "async", doc = "   [`future::WakeFlag`]")]
#![cfg_attr(not(feature = "async"), doc = "   `future::WakeFlag`")]
//! listeners are always `Send + Sync`; and [`Cell`] can have its internal interior mutability
//! swapped out if you implement the [`LoadStore`] trait.
//!
//! # Limitations
//!
//! Besides the logical consequences of the architecture, there are some costs due to implementation
//! restrictions that could have been avoided in a more narrowly scoped library,
//! and some missing features.
//!
//! * In some cases, [`Arc`](alloc::sync::Arc) is used where [`Rc`](alloc::rc::Rc) would do.
//!   This is done in order to avoid unnecessary incompatibilities between the [`sync`] and
//!   [`unsync`] styles of usage (e.g. [`Cell::as_source()`]’s return type does not vary).
//!
//! * Currently, almost all [`Listener`] invocations end up passing through double indirection.
//!   This is because by default, each [`Listener`] in a [`Notifier`] is stored as an
//!   `Arc<dyn Listener>`, and most listeners then contain a weak reference to their actual target.
//!   This could be fixed with “inline box” storage of listeners whose data size is small,
//!   but that is not provided as default functionality.
//!
//!   In specific applications, this can be avoided by creating [`Notifier`]s which store a single
//!   non-`dyn` listener type or enum of listener types. See [`FromListener`] for more information.
//!
//! * There is not yet any support for bringing your own `Mutex` type to enable
//!   [`Notifier`] and [`StoreLock`] to be <code>[Send] + [Sync]</code> without [`std`].
//!   If you need these features, you must use [`RawNotifier`] and implement the other components
//!   yourself.
//!
#![cfg_attr(not(feature = "std"), doc = " [`std`]: https://doc.rust-lang.org/std/")]
#![cfg_attr(
    not(feature = "std"),
    doc = " [`std::sync`]: https://doc.rust-lang.org/std/sync/"
)]
#![cfg_attr(
    not(feature = "std"),
    doc = " [`std::sync::Mutex`]: https://doc.rust-lang.org/std/sync/struct.Mutex.html"
)]
//! [platforms which support `std`]: https://doc.rust-lang.org/rustc/platform-support.html
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
#![warn(clippy::missing_panics_doc)]
#![warn(clippy::module_name_repetitions)]
#![warn(clippy::pedantic)]
#![warn(clippy::return_self_not_must_use)]
#![warn(clippy::should_panic_without_expect)]
#![warn(clippy::unnecessary_self_imports)]
#![warn(clippy::unnecessary_wraps)]
#![allow(clippy::bool_assert_comparison, reason = "less legible")]
#![allow(clippy::explicit_auto_deref)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::semicolon_if_nothing_returned, reason = "explicit delegation")]
#![cfg_attr(test, allow(clippy::arc_with_non_send_sync))]

// -------------------------------------------------------------------------------------------------

extern crate alloc;

#[cfg(any(feature = "std", test))]
extern crate std;

// -------------------------------------------------------------------------------------------------

mod cell;
pub use cell::{Cell, CellSource, CellWithLocal};

mod convert;
pub use convert::{FromListener, IntoListener};

mod filter;
pub use filter::Filter;

#[cfg(feature = "async")]
pub mod future;

mod gate;
pub use gate::{Gate, GateListener};

mod source;
pub use source::{Constant, Flatten, Map, Source};

mod listener;
pub use listener::{Listen, Listener};

mod simple_listeners;
pub use simple_listeners::{Flag, FlagListener, Log, LogListener, NullListener};

mod maybe_sync;

mod notifier;
pub use notifier::{Buffer, Notifier, NotifierForwarder, RawBuffer, RawNotifier};

mod store;
pub use store::{Store, StoreRef};

mod store_lock;
pub use store_lock::{StoreLock, StoreLockListener};

mod util;

mod load_store;
pub use load_store::LoadStore;

// -------------------------------------------------------------------------------------------------

/// Type aliases for use in applications where listeners are expected to implement [`Sync`].
///
/// Some of the items in this module are only available with the `"sync"` feature.
pub mod sync {
    use crate::{Listener, Source};
    use alloc::sync::Arc;
    use core::fmt;

    #[cfg(doc)]
    use crate::{Listen, unsync};

    /// Type-erased form of a [`Listener`] which accepts messages of type `M`.
    ///
    /// This type is [`Send`] and [`Sync`]. When that is not satisfiable, use
    /// [`unsync::DynListener`] instead.
    pub type DynListener<M> = Arc<dyn Listener<M> + Send + Sync>;

    /// Type-erased form of a [`Source`] which provides a value of type `T`.
    ///
    /// This type is [`Send`] and [`Sync`]. When that is not satisfiable, use
    /// [`unsync::DynSource`] instead.
    pub type DynSource<T> =
        Arc<dyn Source<Value = T, Listener = DynListener<()>> + Send + Sync + 'static>;

    /// Message broadcaster.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires all its [`Listener`]s to be so.
    /// When this requirement is undesired, use [`unsync::Notifier`] instead.
    #[cfg(any(feature = "std-sync", feature = "spin-sync"))]
    pub type Notifier<M> = crate::Notifier<M, DynListener<M>>;

    /// Message broadcaster without interior mutability.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires all its [`Listener`]s to be so.
    /// When this requirement is undesired, use [`unsync::Notifier`] or [`unsync::RawNotifier`]
    /// instead.
    ///
    /// Its advantage over `Notifier` is that it is available even without any thread
    /// synchronization support. Its disadvantage is that it requires `&mut` access to use,
    /// and cannot implement [`Listen`].
    pub type RawNotifier<M> = crate::RawNotifier<M, DynListener<M>>;

    /// A [`Listener`] which forwards messages through a [`Notifier`] to its listeners.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires its [`Notifier`] be so.
    /// When this requirement is undesired, use [`unsync::NotifierForwarder`] instead.
    #[cfg(any(feature = "std-sync", feature = "spin-sync"))]
    pub type NotifierForwarder<M> = crate::NotifierForwarder<M, DynListener<M>>;

    /// A [`Source`] of a constant value.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires its [`Listener`]s be so.
    /// When this requirement is undesired, use [`unsync::Constant`] instead.
    pub type Constant<T> = crate::Constant<T, DynListener<()>>;

    /// Returns a [`DynSource`] with a constant value.
    ///
    /// This function behaves identically to `Arc::new(Constant::new(value))` followed by
    /// [unsized coercion](https://doc.rust-lang.org/reference/type-coercions.html#unsized-coercions).
    pub fn constant<T: Clone + Send + Sync + fmt::Debug + 'static>(value: T) -> DynSource<T> {
        Arc::new(Constant::new(value))
    }

    /// An interior-mutable container for a single value, which notifies when the value is changed.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires its [`Listener`]s be so.
    /// When this requirement is undesired, use [`unsync::Cell`] instead.
    ///
    /// This type uses a [`std::sync::Mutex`] to store its `T` value.
    /// When `T` can be stored in an [atomic type][core::sync::atomic], using
    /// [`nosy::Cell`][crate::Cell] with that atomic type will be more efficient.
    #[cfg(feature = "std-sync")]
    pub type Cell<T> = crate::Cell<std::sync::Mutex<T>, DynListener<()>>;

    /// Like [`Cell`], but allows borrowing the current value,
    /// at the cost of requiring `&mut` access to set it, and storing an extra clone.
    ///
    /// This type is [`Send`] and [`Sync`] and therefore requires its [`Listener`]s be so.
    /// When this requirement is undesired, use [`unsync::CellWithLocal`] instead.
    ///
    /// This type uses a [`std::sync::Mutex`] to store its `T` value.
    /// When `T` can be stored in an [atomic type][core::sync::atomic], using
    /// [`nosy::Cell`][crate::Cell] with that atomic type will be more efficient.
    #[cfg(feature = "std-sync")]
    pub type CellWithLocal<T> = crate::CellWithLocal<std::sync::Mutex<T>, DynListener<()>>;

    /// [`Cell::as_source()`] implementation.
    #[cfg(feature = "std-sync")]
    pub type CellSource<T> = Arc<crate::CellSource<std::sync::Mutex<T>, DynListener<()>>>;
}

/// Type aliases for use in applications where listeners are not expected to implement [`Sync`].
#[cfg_attr(not(feature = "std-sync"), allow(rustdoc::broken_intra_doc_links))]
pub mod unsync {
    use crate::{Listener, Source};
    use alloc::rc::Rc;
    use alloc::sync::Arc;
    use core::fmt;

    #[cfg(doc)]
    use crate::sync;

    /// Type-erased form of a [`Listener`] which accepts messages of type `M`.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::DynListener`] instead.
    //---
    pub type DynListener<M> = Rc<dyn Listener<M>>;

    /// Type-erased form of a [`Source`] which provides a value of type `T`.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::DynSource`] instead.
    ///
    /// # Design note: Why `Arc`?
    ///
    /// You may wonder why this type uses [`Arc`] instead of [`Rc`], even though the contents
    /// will never be shareable with another thread.
    /// The answer is that, by using the same pointer type as [`sync::DynSource`],
    /// it allows implementors of [`Source`] such as [`Cell`] to use [`Arc`] as their shared
    /// pointer type and thereby produce a `DynSource` of either `sync` or `unsync` flavor
    /// without introducing any additional indirection in either case.
    /// (I tried adding a trait which would allow picking either [`Arc`] or [`Rc`] consistent with
    /// the [`Listener`] type, and got exciting trait solver failures instead of a working library.)
    pub type DynSource<T> = Arc<dyn Source<Value = T, Listener = DynListener<()>> + 'static>;

    /// Message broadcaster.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::Notifier`] instead.
    pub type Notifier<M> = crate::Notifier<M, DynListener<M>>;

    /// Message broadcaster without interior mutability.
    ///
    /// This type has little advantage over [`Notifier`]; use it only when you require a drop-in
    /// replacement for [`sync::RawNotifier`] without requiring the listeners to be `Send + Sync`.
    pub type RawNotifier<M> = crate::RawNotifier<M, DynListener<M>>;

    /// A [`Listener`] which forwards messages through a [`Notifier`] to its listeners.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::NotifierForwarder`] instead.
    pub type NotifierForwarder<M> = crate::NotifierForwarder<M, DynListener<M>>;

    /// A [`Source`] of a constant value.
    ///
    /// This type is not [`Send`] or [`Sync`]. When that is needed, use
    /// [`sync::Constant`] instead.
    pub type Constant<T> = crate::Constant<T, DynListener<()>>;

    /// Returns a [`DynSource`] with a constant value.
    ///
    /// This function behaves identically to `Arc::new(Constant::new(value))` followed by
    /// [unsized coercion](https://doc.rust-lang.org/reference/type-coercions.html#unsized-coercions).
    pub fn constant<T: Clone + fmt::Debug + 'static>(value: T) -> DynSource<T> {
        Arc::new(Constant::new(value))
    }

    /// An interior-mutable container for a single value, which notifies when the value is changed.
    ///
    /// This type is not [`Send`] or [`Sync`].
    /// When that is needed, use [`sync::Cell`] instead.
    ///
    /// This type uses a [`core::cell::RefCell`] to store its `T` value.
    /// When `T` implements [`Copy`], using
    /// [`nosy::Cell`][crate::Cell] containing [`core::cell::Cell`] will be more efficient.
    pub type Cell<T> = crate::Cell<core::cell::RefCell<T>, DynListener<()>>;

    /// Like [`Cell`], but allows borrowing the current value,
    /// at the cost of requiring `&mut` access to set it, and storing an extra clone.
    ///
    /// This type is not [`Send`] or [`Sync`].
    /// When that is needed, use [`sync::CellWithLocal`] instead.
    ///
    /// This type uses a [`core::cell::RefCell`] to store its `T` value.
    /// When `T` implements [`Copy`], using
    /// [`nosy::Cell`][crate::Cell] containing [`core::cell::Cell`] will be more efficient.
    pub type CellWithLocal<T> = crate::CellWithLocal<core::cell::RefCell<T>, DynListener<()>>;

    /// [`Cell::as_source()`] implementation.
    pub type CellSource<T> = Arc<crate::CellSource<core::cell::RefCell<T>, DynListener<()>>>;
}
