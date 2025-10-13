#![cfg_attr(not(feature = "std"), allow(rustdoc::broken_intra_doc_links))]

use core::cell::RefCell;
use core::fmt;

use crate::Listener;

#[cfg(doc)]
use crate::StoreLock;

// -------------------------------------------------------------------------------------------------

/// A data structure which records received messages for later processing.
///
/// Its role is similar to [`Listener`], except that it is permitted and expected to `&mut` mutate
/// itself, but **should not** communicate outside itself.
/// A [`Listener`] is a sort of channel by which to transmit messages;
/// a `Store` is a data structure which is a destination for messages.
///
/// If the data structure supports interior mutability, it should implement [`StoreRef`] instead
/// or in addition.
///
/// After implementing `Store`, wrap it in a [`StoreLock`] to make use of it as a [`Listener`],
/// or wrap it in a [`RefCell`] or a `Mutex` to make use of it as a [`StoreRef`].
///
/// Generally, a `Store` implementation will combine and de-duplicate messages in some
/// fashion. For example, if the incoming messages were notifications of modified regions of data
/// as rectangles, then one might define a `Store` owning an `Option<Rect>` that contains the union
/// of all the rectangle messages, as an adequate constant-space approximation of the whole.
///
/// # Generic parameters
///
/// * `M` is the type of message that can be received.
///
/// # Example
///
/// **Note:**: While this example is simple and illustrates the kind of thing that should be done
/// to receive messages, in this particular case, it would be more efficient to keep
/// [atomic][core::sync::atomic] types in `Todo` and implement the [`StoreRef`] trait instead.
/// This would avoid needing a mutex at all. You only actually need [`Store`] and [`StoreLock`]
/// when the data cannot be updated atomically,
/// or if the recipient needs a consistent view of multiple pieces of data.
// TODO: Add a better example.
///
/// ```rust
/// use nosy::{Listen as _, Listener as _, StoreLock};
/// use nosy::unsync::{Cell, Notifier};
///
/// /// Message type delivered from other sources.
/// /// (This enum might be aggregated from multiple `Source`s’ change notifications.)
/// #[derive(Debug)]
/// enum WindowChange {
///     Resized,
///     Contents,
/// }
///
/// /// Tracks what we need to do in response to the messages.
/// #[derive(Debug, Default, PartialEq)]
/// struct Todo {
///     resize: bool,
///     redraw: bool,
/// }
///
/// impl nosy::Store<WindowChange> for Todo {
///     fn receive(&mut self, messages: &[WindowChange]) {
///         for message in messages {
///             match message {
///                 WindowChange::Resized => {
///                     self.resize = true;
///                     self.redraw = true;
///                 }
///                 WindowChange::Contents => {
///                     self.redraw = true;
///                 }
///             }
///         }
///     }
/// }
///
/// // These would actually come from external data sources.
/// let window_size: Cell<[u32; 2]> = Cell::new([100, 100]);
/// let contents_notifier: Notifier<()> = Notifier::new();
///
/// // Create the store and attach its listener to the data sources.
/// let todo_store: StoreLock<Todo> = StoreLock::default();
/// window_size.listen(todo_store.listener().filter(|()| Some(WindowChange::Resized)));
/// contents_notifier.listen(todo_store.listener().filter(|()| Some(WindowChange::Contents)));
///
/// // Make a change and see it reflected in the store.
/// window_size.set([200, 120]);
/// assert_eq!(todo_store.take(), Todo { resize: true, redraw: true });
/// ```
pub trait Store<M> {
    /// Record the given series of messages.
    ///
    /// # Requirements on implementors
    ///
    // [Docs maintenance note: keep wording consistent with the below, `Listener`, and `StoreRef`.]
    ///
    /// * Messages are provided in a batch for efficiency of dispatch.
    ///   Each message in the provided slice should be processed exactly the same as if
    ///   it were the only message provided.
    ///   If the slice is empty, there should be no observable effect.
    ///
    /// * Do not panic under any possible incoming message stream,
    ///   in order to ensure the sender's other work is not interfered with.
    ///
    /// * Do not perform any blocking operation, such as acquiring locks,
    ///   for performance and to avoid deadlock with locks held by the sender.
    ///   (Normally, locking is to be provided separately, e.g. by [`StoreLock`].)
    ///
    /// * Do not access thread-local state, since this may be called from whichever thread(s)
    ///   the sender is using.
    ///
    /// # Advice for implementors
    ///
    /// Implementations should take care to be efficient, both in time taken and other
    /// costs such as working set size. This method is typically called with a mutex held and the
    /// original message sender blocking on it, so inefficiency here may have an effect on
    /// distant parts of the application.
    fn receive(&mut self, messages: &[M]);
}

// -------------------------------------------------------------------------------------------------

/// This is a poor implementation of [`Store`] because it allocates unboundedly.
/// It should be used only for tests of message processing.
impl<M: Clone + Send> Store<M> for alloc::vec::Vec<M> {
    fn receive(&mut self, messages: &[M]) {
        self.extend_from_slice(messages);
    }
}

impl<M: Clone + Send + Ord> Store<M> for alloc::collections::BTreeSet<M> {
    fn receive(&mut self, messages: &[M]) {
        self.extend(messages.iter().cloned());
    }
}
#[cfg(feature = "std")]
impl<M: Clone + Send + Eq + core::hash::Hash, S: core::hash::BuildHasher> Store<M>
    for std::collections::HashSet<M, S>
{
    fn receive(&mut self, messages: &[M]) {
        self.extend(messages.iter().cloned());
    }
}

// -------------------------------------------------------------------------------------------------

/// An interior-mutable data structure which records received messages for later processing.
///
/// [`StoreRef`] is implemented for [`RefCell`]s and [`std::sync::Mutex`]es that contain
/// [`Store`] implementations, and you can implement it yourself for other interior-mutable
/// data structures.
///
/// [`StoreRef`] is similar to [`Listener`], except that implementors are not required to be aware
/// when they no longer have a use for receiving messages — they are something that is to be shared,
/// rather than the handle to the shared thing.
/// Therefore, in order to make use of a [`StoreRef`] implementation, wrap it in a weak reference,
/// [`rc::Weak`][alloc::rc::Weak] or [`sync::Weak`][alloc::sync::Weak], which will automatically
/// implement [`Listener`].
///
/// # Generic parameters
///
/// * `M` is the type of message that can be received.
///
/// # Example
///
/// ```
/// use std::sync::{Arc, atomic::{AtomicBool, Ordering::Relaxed}};
/// use nosy::{Listen as _, IntoListener as _, Listener as _, StoreRef};
/// use nosy::unsync::{Cell, Notifier, DynListener};
///
/// /// Message type delivered from other sources.
/// /// (This enum might be aggregated from multiple `Source`s’ change notifications.)
/// #[derive(Debug)]
/// enum WindowChange {
///     Resized,
///     Contents,
/// }
///
/// /// Tracks what we need to do in response to the messages.
/// #[derive(Debug, Default)]
/// struct Todo {
///     // Note: In a serious application, it might be useful to represent these as bits in a
///     // single `AtomicU8`, to allow atomic read and clear of both flags at once.
///     resize: AtomicBool,
///     redraw: AtomicBool,
/// }
///
/// impl nosy::StoreRef<WindowChange> for Todo {
///     fn receive(&self, messages: &[WindowChange]) {
///         for message in messages {
///             match message {
///                 WindowChange::Resized => {
///                     self.resize.store(true, Relaxed);
///                     self.redraw.store(true, Relaxed);
///                 }
///                 WindowChange::Contents => {
///                     self.redraw.store(true, Relaxed);
///                 }
///             }
///         }
///     }
/// }
///
/// // These would actually come from external data sources.
/// let window_size: Cell<[u32; 2]> = Cell::new([100, 100]);
/// let contents_notifier: Notifier<()> = Notifier::new();
///
/// // Create the store and attach its listener to the data sources.
/// let todo_store: Arc<Todo> = Arc::default();
/// let listener: DynListener<WindowChange> = Arc::downgrade(&todo_store).into_listener();
/// window_size.listen(listener.clone().filter(|()| Some(WindowChange::Resized)));
/// contents_notifier.listen(listener.filter(|()| Some(WindowChange::Contents)));
///
/// // Make a change and see it reflected in the store.
/// window_size.set([200, 120]);
/// assert_eq!(todo_store.resize.fetch_and(false, Relaxed), true);
/// assert_eq!(todo_store.redraw.fetch_and(false, Relaxed), true);
/// ```
pub trait StoreRef<M> {
    /// Record the given series of messages.
    ///
    /// # Requirements on implementors
    ///
    // [Docs maintenance note: keep wording consistent with the below, `Store`, and `Listener`.]
    ///
    /// * Messages are provided in a batch for efficiency of dispatch.
    ///   Each message in the provided slice should be processed exactly the same as if
    ///   it were the only message provided.
    ///   If the slice is empty, there should be no observable effect.
    ///
    /// * Do not panic under any possible incoming message stream,
    ///   in order to ensure the sender's other work is not interfered with.
    ///   For example, if the operation accesses a poisoned mutex, it should do nothing or clear
    ///   the poison, rather than panicking.
    ///
    /// * Do not acquire any locks except ones which are used only for the state of the
    ///   listener itself.
    ///  
    /// * Do not perform any blocking operation except for such locks.
    ///
    /// * Do not access thread-local state, since this may be called from whichever thread(s)
    ///   the sender is using.
    ///
    /// # Advice for implementors
    ///
    /// Implementations should take care to be efficient, both in time taken and other
    /// costs such as working set size.
    /// Locking should be done with careful consideration of performance and avoiding deadlock with
    /// locks held by the sender.
    /// This method is called with the original message sender blocking on it, so inefficiency here
    /// may have an effect on distant parts of the application.
    fn receive(&self, messages: &[M]);
}

// -------------------------------------------------------------------------------------------------

/// A weak reference to a [`StoreRef`] acts as a [`Listener`].
impl<T: ?Sized + StoreRef<M> + fmt::Debug, M> Listener<M> for alloc::rc::Weak<T> {
    fn receive(&self, messages: &[M]) -> bool {
        if messages.is_empty() {
            // Check aliveness but skip changing the strong count.
            self.strong_count() > 0
        } else if let Some(referent) = self.upgrade() {
            referent.receive(messages);
            true
        } else {
            false
        }
    }
}

/// A weak reference to a [`StoreRef`] acts as a [`Listener`].
impl<T: ?Sized + StoreRef<M> + fmt::Debug, M> Listener<M> for alloc::sync::Weak<T> {
    fn receive(&self, messages: &[M]) -> bool {
        if messages.is_empty() {
            // Check aliveness but skip changing the strong count.
            self.strong_count() > 0
        } else if let Some(referent) = self.upgrade() {
            referent.receive(messages);
            true
        } else {
            false
        }
    }
}

/// A [`RefCell`] containing a [`Store`] acts as a [`StoreRef`].
impl<T: ?Sized + Store<M> + fmt::Debug, M> StoreRef<M> for RefCell<T> {
    fn receive(&self, messages: &[M]) {
        self.borrow_mut().receive(messages)
    }
}

/// A [`Mutex`][std::sync::Mutex] containing a [`Store`] acts as a [`StoreRef`].
///
/// This implementation is only available with `feature = "std"`.
///
/// The mutex’s [poison][std::sync::Mutex#poisoning] state is ignored and messages will be
/// delivered anyway. We consider it the responsibility of the party *reading* from the mutex to
/// deal with the implications of a previous panic having occurred.
#[cfg(feature = "std")]
impl<T: ?Sized + Store<M> + fmt::Debug, M> StoreRef<M> for std::sync::Mutex<T> {
    fn receive(&self, messages: &[M]) {
        self.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .receive(messages)
    }
}
