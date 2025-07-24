use alloc::sync::Weak;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering::Relaxed};

#[cfg(doc)]
use alloc::sync::Arc;

use crate::maybe_sync::RwLock;
use crate::{IntoDynListener, Listen, Listener};

#[cfg(doc)]
use crate::{sync, Source};

// -------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
/// Delivers messages to a dynamic set of [`Listener`]s.
///
/// The `Notifier` is usually owned by some entity which emits messages when it changes.
/// [`Listener`]s may be added using the [`Listen`] implementation, and are removed when
/// they report themselves as dead (usually by means of checking a weak reference).
///
/// We recommend that you use the type aliases [`sync::Notifier`][crate::sync::Notifier]
/// or [`unsync::Notifier`][crate::unsync::Notifier], to avoid writing the type parameter
/// `L` outside of special cases.
///
/// # Generic parameters
///
/// * `M` is the type of the messages to be broadcast.
/// * `L` is the type of [`Listener`] accepted,
///   usually a trait object type such as [`sync::DynListener`].
pub struct Notifier<M, L> {
    state: RwLock<State<M, L>>,
}

enum State<M, L> {
    Open(RawNotifier<M, L>),
    /// The notifier is unable to send any more messages and does not accept listeners.
    Closed,
}

/// [`Notifier`] but without interior mutability.
///
/// Compared to [`Notifier`],
/// this type requires `&mut` access to add listeners, and therefore does not implement [`Listen`].
/// In exchange, it is always `Send + Sync` if the listener type is, even without `feature = "sync"`
/// and thus without a dependency on `std`.
///
/// We recommend that you use the type aliases [`sync::RawNotifier`][crate::sync::RawNotifier]
/// or [`unsync::RawNotifier`][crate::unsync::RawNotifier], to avoid writing the type parameter
/// `L` outside of special cases.
///
/// # Generic parameters
///
/// * `M` is the type of the messages to be broadcast.
/// * `L` is the type of [`Listener`] accepted,
///   usually a trait object type such as [`sync::DynListener`].
//---
// Design note: `State` and `close()` is not a part of `RawNotifier` because they would require
// `&mut self` to perform the closure, so there’s no benefit to making them internal.
pub struct RawNotifier<M, L> {
    listeners: Vec<NotifierEntry<L>>,
    _phantom: PhantomData<fn(&M)>,
}

pub(crate) struct NotifierEntry<L> {
    listener: L,
    /// True iff every call to `listener.receive()` has returned true.
    was_alive: AtomicBool,
}

// -------------------------------------------------------------------------------------------------
// Impls for `Notifier` and `RawNotifier` are interleaved because they are so similar.

impl<M, L> Notifier<M, L> {
    /// Constructs a new [`Notifier`] with no listeners.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(State::Open(RawNotifier::new())),
        }
    }

    /// Signals that no further messages will be delivered, by dropping all current and future
    /// listeners.
    ///
    /// After calling this, [`Notifier::notify()`] will panic if called.
    ///
    /// This method should be used when a `Notifier` has shared ownership and the remaining owners
    /// (e.g. a [`Source`]) cannot be used to send further messages.
    /// It is not necessary when the `Notifier` itself is dropped.
    pub fn close(&self) {
        *self.state.write() = State::Closed;
    }
}
impl<M, L> RawNotifier<M, L> {
    /// Constructs a new [`RawNotifier`] with no listeners.
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
            _phantom: PhantomData,
        }
    }

    fn count_approximate(&self) -> usize {
        self.listeners.len()
    }
}

impl<M, L: Listener<M>> Notifier<M, L> {
    /// Returns a [`Listener`] which forwards messages to the listeners registered with
    /// this `Notifier`, provided that it is owned by an [`Arc`].
    ///
    /// This may be used together with [`Listener::filter()`] to forward notifications
    /// of changes in dependencies. Using this operation means that the dependent does not
    /// need to fan out listener registrations to all of its current dependencies.
    ///
    /// ```
    /// use std::sync::Arc;
    #[cfg_attr(feature = "sync", doc = " use nosy::{Listen, sync::Notifier, Log};")]
    #[cfg_attr(
        not(feature = "sync"),
        doc = " use nosy::{Listen, unsync::Notifier, Log};"
    )]
    ///
    /// let notifier_1 = Notifier::new();
    /// let notifier_2 = Arc::new(Notifier::new());
    /// let mut log = Log::new();
    /// notifier_1.listen(Notifier::forwarder(Arc::downgrade(&notifier_2)));
    /// notifier_2.listen(log.listener());
    /// # assert_eq!(notifier_1.count(), 1);
    /// # assert_eq!(notifier_2.count(), 1);
    ///
    /// notifier_1.notify(&"a");
    /// assert_eq!(log.drain(), vec!["a"]);
    /// drop(notifier_2);
    /// notifier_1.notify(&"a");
    /// assert!(log.drain().is_empty());
    ///
    /// # assert_eq!(notifier_1.count(), 0);
    /// ```
    #[must_use]
    pub fn forwarder(this: Weak<Self>) -> NotifierForwarder<M, L> {
        NotifierForwarder(this)
    }

    /// Deliver a message to all [`Listener`]s.
    pub fn notify(&self, message: &M) {
        self.notify_many(core::slice::from_ref(message))
    }

    /// Deliver multiple messages to all [`Listener`]s.
    ///
    /// # Panics
    ///
    /// Panics if [`Notifier::close()`] was previously called.
    pub fn notify_many(&self, messages: &[M]) {
        match &*self.state.read() {
            State::Open(raw) => raw.notify_many(messages),
            State::Closed => panic!("cannot send messages after Notifier::close()"),
        }
    }

    /// Creates a [`Buffer`] which batches messages sent through it.
    /// This may be used as a more convenient interface to [`Notifier::notify_many()`],
    /// at the cost of delaying messages until the buffer is dropped.
    ///
    /// The buffer does not use any heap allocations and will collect up to `CAPACITY` messages
    /// per batch.
    ///
    /// # Example
    ///
    /// ```
    /// use nosy::{Listen as _, unsync::Notifier, Log};
    ///
    /// let notifier: Notifier<&str> = Notifier::new();
    /// let log: Log<&str> = Log::new();
    /// notifier.listen(log.listener());
    ///
    /// let mut buffer = notifier.buffer::<2>();
    ///
    /// // The buffer fills up and sends after two messages.
    /// buffer.push("hello");
    /// assert!(log.drain().is_empty());
    /// buffer.push("and");
    /// assert_eq!(log.drain(), vec!["hello", "and"]);
    ///
    /// // The buffer also sends when it is dropped.
    /// buffer.push("goodbye");
    /// drop(buffer);
    /// assert_eq!(log.drain(), vec!["goodbye"]);
    /// ```
    pub fn buffer<const CAPACITY: usize>(&self) -> Buffer<'_, M, L, CAPACITY> {
        Buffer::new(self)
    }

    /// Computes the exact count of listeners, including asking all current listeners
    /// if they are alive.
    ///
    /// This operation is intended for testing and diagnostic purposes.
    pub fn count(&self) -> usize {
        let mut state = self.state.write();
        state.drop_dead_listeners();
        state.count_approximate()
    }
}
impl<M, L: Listener<M>> RawNotifier<M, L> {
    /// Deliver a message to all [`Listener`]s.
    pub fn notify(&self, message: &M) {
        self.notify_many(core::slice::from_ref(message))
    }

    /// Deliver multiple messages to all [`Listener`]s.
    ///
    /// # Panics
    ///
    /// Panics if [`Notifier::close()`] was previously called.
    pub fn notify_many(&self, messages: &[M]) {
        for NotifierEntry {
            listener,
            was_alive,
        } in self.listeners.iter()
        {
            // Don't load was_alive before sending, because we assume the common case is that
            // a listener implements receive() cheaply when it is dead.
            let alive = listener.receive(messages);

            was_alive.fetch_and(alive, Relaxed);
        }
    }

    /// Creates a [`Buffer`] which batches messages sent through it.
    /// This may be used as a more convenient interface to [`RawNotifier::notify_many()`],
    /// at the cost of delaying messages until the buffer is dropped.
    ///
    /// The buffer does not use any heap allocations and will collect up to `CAPACITY` messages
    /// per batch.
    ///
    /// # Example
    ///
    /// ```
    /// use nosy::{Listen as _, unsync::RawNotifier, Log};
    ///
    /// let mut notifier: RawNotifier<&str> = RawNotifier::new();
    /// let log: Log<&str> = Log::new();
    /// notifier.listen(log.listener());
    ///
    /// let mut buffer = notifier.buffer::<2>();
    ///
    /// // The buffer fills up and sends after two messages.
    /// buffer.push("hello");
    /// assert!(log.drain().is_empty());
    /// buffer.push("and");
    /// assert_eq!(log.drain(), vec!["hello", "and"]);
    ///
    /// // The buffer also sends when it is dropped.
    /// buffer.push("goodbye");
    /// drop(buffer);
    /// assert_eq!(log.drain(), vec!["goodbye"]);
    /// ```
    pub fn buffer<const CAPACITY: usize>(&mut self) -> RawBuffer<'_, M, L, CAPACITY> {
        RawBuffer::new(self)
    }

    /// Add a listener which will receive future messages.
    pub fn listen<L2: IntoDynListener<M, L>>(&mut self, listener: L2) {
        if !listener.receive(&[]) {
            // skip adding it if it's already dead
            return;
        }

        self.drop_dead_if_full();

        self.listeners.push(NotifierEntry {
            listener: listener.into_dyn_listener(),
            was_alive: AtomicBool::new(true),
        });
    }

    /// Identical to [`Self::listen()`] except that it doesn't call `into_dyn_listener()`.
    fn listen_raw(&mut self, listener: L) {
        if !listener.receive(&[]) {
            // skip adding it if it's already dead
            return;
        }

        self.drop_dead_if_full();

        self.listeners.push(NotifierEntry {
            listener,
            was_alive: AtomicBool::new(true),
        });
    }

    // TODO: Add Buffer for RawNotifier

    /// Discard all dead listeners.
    ///
    /// This is done automatically as new listeners are added.
    /// Doing this explicitly may be useful to control the timing of deallocation or
    /// to get a more accurate count of alive listeners.
    //---
    // TODO: add doctest?
    #[mutants::skip] // there are many ways to subtly break this
    fn drop_dead_listeners(&mut self) {
        let listeners = &mut self.listeners;
        let mut i = 0;
        while i < listeners.len() {
            let entry = &listeners[i];
            // We must ask the listener, not just consult was_alive, in order to avoid
            // leaking memory if listen() is called repeatedly without any notify().
            // TODO: But we can skip it if the last operation was notify().
            if entry.was_alive.load(Relaxed) && entry.listener.receive(&[]) {
                i += 1;
            } else {
                listeners.swap_remove(i);
            }
        }
    }

    fn drop_dead_if_full(&mut self) {
        let full = self.listeners.len() >= self.listeners.capacity();
        if full {
            self.drop_dead_listeners();
        }
    }

    /// Computes the exact count of listeners, including asking all current listeners
    /// if they are alive.
    ///
    /// This operation is intended for testing and diagnostic purposes.
    #[must_use]
    pub fn count(&self) -> usize {
        self.listeners
            .iter()
            .filter(|entry| entry.was_alive.load(Relaxed) && entry.listener.receive(&[]))
            .count()
    }
}

impl<M, L: Listener<M>> Listen for Notifier<M, L> {
    type Msg = M;
    type Listener = L;

    fn listen_raw(&self, listener: L) {
        match *self.state.write() {
            State::Open(ref mut raw_notifier) => raw_notifier.listen_raw(listener),
            State::Closed => {}
        }
    }

    // By adding this implementation instead of taking the default, we can defer
    // `into_dyn_listener()` until we've done the early exit test.
    fn listen<L2: IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L2) {
        match *self.state.write() {
            State::Open(ref mut raw_notifier) => raw_notifier.listen(listener),
            State::Closed => {}
        }
    }
}

impl<M, L: Listener<M>> Default for Notifier<M, L> {
    fn default() -> Self {
        Self::new()
    }
}
impl<M, L: Listener<M>> Default for RawNotifier<M, L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, L> fmt::Debug for Notifier<M, L> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.state.try_read().as_deref() {
            // not using fmt.debug_tuple() so this is never printed on multiple lines
            Ok(State::Open(raw_notifier)) => {
                write!(fmt, "Notifier({})", raw_notifier.count_approximate())
            }
            Ok(State::Closed) => write!(fmt, "Notifier(closed)"),
            Err(_) => write!(fmt, "Notifier(?)"),
        }
    }
}
impl<M, L> fmt::Debug for RawNotifier<M, L> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        // not using fmt.debug_tuple() so this is never printed on multiple lines
        write!(fmt, "RawNotifier({})", self.count_approximate())
    }
}

impl<M, L> State<M, L> {
    fn count_approximate(&self) -> usize {
        match self {
            State::Open(listeners) => listeners.count_approximate(),
            State::Closed => 0,
        }
    }
}

impl<M, L: Listener<M>> State<M, L> {
    /// Discard all dead listeners.
    fn drop_dead_listeners(&mut self) {
        match self {
            State::Closed => {}
            State::Open(raw_notifier) => raw_notifier.drop_dead_listeners(),
        }
    }
}

// -------------------------------------------------------------------------------------------------

/// Buffers messages that are to be sent through a [`Notifier`], for efficiency.
///
/// Messages may be added to the buffer, and when the buffer contains
/// `CAPACITY` messages or when it is dropped,
/// they are all sent through [`Notifier::notify_many()`] at once.
/// This is intended to increase performance by invoking each listener once per batch
/// instead of once per message.
///
/// Create a [`Buffer`] by calling [`Notifier::buffer()`].
///
/// # Generic parameters
///
/// * `'notifier` is the lifetime of the borrow of the [`Notifier`]
///   which this sends messages to.
/// * `M` is the type of message accepted by the [`Notifier`].
/// * `L` is the type of [`Listener`] accepted by the [`Notifier`].
/// * `CAPACITY` is the maximum number of messages in one batch.
///   The buffer memory is allocated in-line in the [`Buffer`] value, so
///   `CAPACITY` should be chosen with consideration for stack memory usage.
#[derive(Debug)]
pub struct Buffer<'notifier, M, L, const CAPACITY: usize>
where
    L: Listener<M>,
{
    pub(crate) buffer: arrayvec::ArrayVec<M, CAPACITY>,
    pub(crate) notifier: &'notifier Notifier<M, L>,
}
/// Buffers messages that are to be sent through a [`RawNotifier`], for efficiency.
///
/// Messages may be added to the buffer, and when the buffer contains
/// `CAPACITY` messages or when it is dropped,
/// they are all sent through [`RawNotifier::notify_many()`] at once.
/// This is intended to increase performance by invoking each listener once per batch
/// instead of once per message.
///
/// Create a [`Buffer`] by calling [`RawNotifier::buffer()`].
///
/// # Generic parameters
///
/// * `'notifier` is the lifetime of the borrow of the [`Notifier`]
///   which this sends messages to.
/// * `M` is the type of message accepted by the [`Notifier`].
/// * `L` is the type of [`Listener`] accepted by the [`Notifier`].
/// * `CAPACITY` is the maximum number of messages in one batch.
///   The buffer memory is allocated in-line in the [`Buffer`] value, so
///   `CAPACITY` should be chosen with consideration for stack memory usage.
#[derive(Debug)]
pub struct RawBuffer<'notifier, M, L, const CAPACITY: usize>
where
    L: Listener<M>,
{
    pub(crate) buffer: arrayvec::ArrayVec<M, CAPACITY>,
    pub(crate) notifier: &'notifier mut RawNotifier<M, L>,
}

impl<'notifier, M, L, const CAPACITY: usize> Buffer<'notifier, M, L, CAPACITY>
where
    L: Listener<M>,
{
    pub(crate) fn new(notifier: &'notifier Notifier<M, L>) -> Self {
        Self {
            buffer: arrayvec::ArrayVec::new(),
            notifier,
        }
    }

    /// Store a message in this buffer, to be delivered later as if by [`Notifier::notify()`].
    ///
    /// If the buffer becomes full when this message is added, then the messages in the buffer will
    /// be delivered before `push()` returns.
    /// Otherwise, they will be delivered when the [`Buffer`] is dropped.
    pub fn push(&mut self, message: M) {
        // We don't need to check for fullness before pushing, because we always flush immediately
        // if full.
        self.buffer.push(message);
        if self.buffer.is_full() {
            self.flush();
        }
    }

    #[cold]
    pub(crate) fn flush(&mut self) {
        self.notifier.notify_many(&self.buffer);
        self.buffer.clear();
    }
}
impl<'notifier, M, L, const CAPACITY: usize> RawBuffer<'notifier, M, L, CAPACITY>
where
    L: Listener<M>,
{
    pub(crate) fn new(notifier: &'notifier mut RawNotifier<M, L>) -> Self {
        Self {
            buffer: arrayvec::ArrayVec::new(),
            notifier,
        }
    }

    /// Store a message in this buffer, to be delivered later as if by [`RawNotifier::notify()`].
    ///
    /// If the buffer becomes full when this message is added, then the messages in the buffer will
    /// be delivered before `push()` returns.
    /// Otherwise, they will be delivered when the [`Buffer`] is dropped.
    pub fn push(&mut self, message: M) {
        // We don't need to check for fullness before pushing, because we always flush immediately
        // if full.
        self.buffer.push(message);
        if self.buffer.is_full() {
            self.flush();
        }
    }

    #[cold]
    pub(crate) fn flush(&mut self) {
        self.notifier.notify_many(&self.buffer);
        self.buffer.clear();
    }
}

impl<M, L, const CAPACITY: usize> Drop for Buffer<'_, M, L, CAPACITY>
where
    L: Listener<M>,
{
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            self.flush();
        }
    }
}
impl<M, L, const CAPACITY: usize> Drop for RawBuffer<'_, M, L, CAPACITY>
where
    L: Listener<M>,
{
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            self.flush();
        }
    }
}

// -------------------------------------------------------------------------------------------------

/// A [`Listener`] which forwards messages through a [`Notifier`] to its listeners.
/// Constructed by [`Notifier::forwarder()`].
///
/// # Generic parameters
///
/// * `M` is the type of the messages to be broadcast.
/// * `L` is the type of [`Listener`] accepted by the [`Notifier`].
pub struct NotifierForwarder<M, L>(pub(super) Weak<Notifier<M, L>>);

impl<M, L> fmt::Debug for NotifierForwarder<M, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotifierForwarder")
            .field("alive(shallow)", &(self.0.strong_count() > 0))
            .finish_non_exhaustive()
    }
}

impl<M, L: Listener<M>> Listener<M> for NotifierForwarder<M, L> {
    fn receive(&self, messages: &[M]) -> bool {
        if let Some(notifier) = self.0.upgrade() {
            notifier.notify_many(messages);
            true
        } else {
            false
        }
    }
}

impl<M, L> Clone for NotifierForwarder<M, L> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
