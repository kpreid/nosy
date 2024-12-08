#![allow(
    clippy::module_name_repetitions,
    reason = "false positive; TODO: remove after Rust 1.84 is released"
)]

use alloc::sync::Weak;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering::Relaxed};

#[cfg(doc)]
use alloc::sync::Arc;

use crate::maybe_sync::RwLock;
use crate::{IntoDynListener, Listen, Listener};

// #[cfg(doc)]
// use crate::ListenableCell;

// -------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
/// Message broadcaster.
///
/// A `Notifier<M, L>` delivers messages of type `M` to a dynamic set of [`Listener`]s
/// of type `L`. `L` is usually a trait object type such as `Arc<dyn Listener + Send + Sync>`.
///
/// The `Notifier` is usually owned by some entity which emits messages when it changes.
/// [`Listener`]s may be added using the [`Listen`] implementation, and are removed when
/// they report themselves as dead (usually by means of checking a weak reference).
///
/// We recommend that you use the type aliases [`sync::Notifier`](crate::sync::Notifier)
/// or [`unsync::Notifier`](crate::unsync::Notifier), to avoid writing the type parameter
/// `L` outside of special cases.
pub struct Notifier<M, L> {
    listeners: RwLock<Vec<NotifierEntry<L>>>,
    _phantom: PhantomData<fn(&M)>,
}

pub(crate) struct NotifierEntry<L> {
    listener: L,
    /// True iff every call to `listener.receive()` has returned true.
    was_alive: AtomicBool,
}

impl<M, L: Listener<M>> Notifier<M, L> {
    /// Constructs a new [`Notifier`] with no listeners.
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: RwLock::default(),
            _phantom: PhantomData,
        }
    }

    /// Returns a [`Listener`] which forwards messages to the listeners registered with
    /// this `Notifier`, provided that it is owned by an [`Arc`].
    ///
    /// This may be used together with [`Listener::filter()`] to forward notifications
    /// of changes in dependencies. Using this operation means that the dependent does not
    /// need to fan out listener registrations to all of its current dependencies.
    ///
    /// ```
    /// use std::sync::Arc;
    #[cfg_attr(feature = "sync", doc = " use synch::{Listen, sync::Notifier, Sink};")]
    #[cfg_attr(
        not(feature = "sync"),
        doc = " use synch::{Listen, unsync::Notifier, Sink};"
    )]
    ///
    /// let notifier_1 = Notifier::new();
    /// let notifier_2 = Arc::new(Notifier::new());
    /// let mut sink = Sink::new();
    /// notifier_1.listen(Notifier::forwarder(Arc::downgrade(&notifier_2)));
    /// notifier_2.listen(sink.listener());
    /// # assert_eq!(notifier_1.count(), 1);
    /// # assert_eq!(notifier_2.count(), 1);
    ///
    /// notifier_1.notify(&"a");
    /// assert_eq!(sink.drain(), vec!["a"]);
    /// drop(notifier_2);
    /// notifier_1.notify(&"a");
    /// assert!(sink.drain().is_empty());
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
    pub fn notify_many(&self, messages: &[M]) {
        for NotifierEntry {
            listener,
            was_alive,
        } in self.listeners.read().iter()
        {
            // Don't load was_alive before sending, because we assume the common case is that
            // a listener implements receive() cheaply when it is dead.
            let alive = listener.receive(messages);

            was_alive.fetch_and(alive, Relaxed);
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
    /// use synch::{Listen as _, unsync::Notifier, Sink};
    ///
    /// let notifier: Notifier<&str> = Notifier::new();
    /// let sink: Sink<&str> = Sink::new();
    /// notifier.listen(sink.listener());
    ///
    /// let mut buffer = notifier.buffer::<2>();
    ///
    /// // The buffer fills up and sends after two messages.
    /// buffer.push("hello");
    /// assert!(sink.drain().is_empty());
    /// buffer.push("and");
    /// assert_eq!(sink.drain(), vec!["hello", "and"]);
    ///
    /// // The buffer also sends when it is dropped.
    /// buffer.push("goodbye");
    /// drop(buffer);
    /// assert_eq!(sink.drain(), vec!["goodbye"]);
    /// ```
    pub fn buffer<const CAPACITY: usize>(&self) -> Buffer<'_, M, L, CAPACITY> {
        Buffer::new(self)
    }

    /// Computes the exact count of listeners, including asking all current listeners
    /// if they are alive.
    ///
    /// This operation is intended for testing and diagnostic purposes.
    pub fn count(&self) -> usize {
        let mut listeners = self.listeners.write();
        Self::cleanup(&mut listeners);
        listeners.len()
    }

    /// Discard all dead weak pointers in `listeners`.
    #[mutants::skip] // there are many ways to subtly break this
    fn cleanup(listeners: &mut Vec<NotifierEntry<L>>) {
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
}

impl<M, L: Listener<M>> Listen for Notifier<M, L> {
    type Msg = M;
    type Listener = L;

    fn listen<L2: IntoDynListener<M, L>>(&self, listener: L2) {
        if !listener.receive(&[]) {
            // skip adding it if it's already dead
            return;
        }
        let mut listeners = self.listeners.write();
        // TODO: consider amortization by not doing cleanup every time
        Self::cleanup(&mut listeners);
        listeners.push(NotifierEntry {
            listener: listener.into_dyn_listener(),
            was_alive: AtomicBool::new(true),
        });
    }
}

impl<M, L: Listener<M>> Default for Notifier<M, L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, L> fmt::Debug for Notifier<M, L> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        // not using fmt.debug_tuple() so this is never printed on multiple lines
        if let Ok(listeners) = self.listeners.try_read() {
            write!(fmt, "Notifier({})", listeners.len())
        } else {
            write!(fmt, "Notifier(?)")
        }
    }
}

// -------------------------------------------------------------------------------------------------

/// Buffers messages that are to be sent through a [`Notifier`], for efficiency.
///
/// Messages of type `M` may be added to the buffer, and when the buffer contains
/// `CAPACITY` messages or when it is dropped,
/// they are all sent through [`Notifier::notify_many()`] at once.
/// This is intended to increase performance by invoking each listener once per batch
/// instead of once per message.
#[derive(Debug)]
pub struct Buffer<'notifier, M, L, const CAPACITY: usize>
where
    L: Listener<M>,
{
    pub(crate) buffer: arrayvec::ArrayVec<M, CAPACITY>,
    pub(crate) notifier: &'notifier Notifier<M, L>,
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

impl<M, L, const CAPACITY: usize> Drop for Buffer<'_, M, L, CAPACITY>
where
    L: Listener<M>,
{
    fn drop(&mut self) {
        // TODO: Should we discard messages if panicking?
        // Currently leaning no, because we've specified that listeners should not panic even under
        // error conditions such as poisoned mutexes.
        if !self.buffer.is_empty() {
            self.flush();
        }
    }
}

// -------------------------------------------------------------------------------------------------

/// A [`Listener`] which forwards messages through a [`Notifier`] to its listeners.
/// Constructed by [`Notifier::forwarder()`].
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

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sink;
    use alloc::sync::Arc;
    use alloc::{format, vec};

    #[test]
    fn notifier_basics_and_debug() {
        let cn: crate::unsync::Notifier<u8> = Notifier::new();
        assert_eq!(format!("{cn:?}"), "Notifier(0)");
        cn.notify(&0);
        assert_eq!(format!("{cn:?}"), "Notifier(0)");
        let sink = Sink::new();
        cn.listen(sink.listener());
        assert_eq!(format!("{cn:?}"), "Notifier(1)");
        // type annotation to prevent spurious inference failures in the presence
        // of other compiler errors
        assert_eq!(sink.drain(), Vec::<u8>::new());
        cn.notify(&1);
        cn.notify(&2);
        assert_eq!(sink.drain(), vec![1, 2]);
        assert_eq!(format!("{cn:?}"), "Notifier(1)");
    }

    // Test for NotifierForwarder functionality exists as a doc-test.

    #[test]
    fn notifier_forwarder_debug() {
        let n: Arc<crate::unsync::Notifier<u8>> = Arc::new(Notifier::new());
        let nf = Notifier::forwarder(Arc::downgrade(&n));
        assert_eq!(
            format!("{nf:?}"),
            "NotifierForwarder { alive(shallow): true, .. }"
        );
        drop(n);
        assert_eq!(
            format!("{nf:?}"),
            "NotifierForwarder { alive(shallow): false, .. }"
        );
    }
}
