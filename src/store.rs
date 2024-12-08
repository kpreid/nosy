#![allow(
    clippy::module_name_repetitions,
    reason = "false positive; TODO: remove after Rust 1.84 is released"
)]

use core::fmt;

use manyfmt::formats::Unquote;
use manyfmt::Refmt as _;

use crate::maybe_sync::{self, MaRc, MaWeak};
use crate::Listener;

/// A data structure which records received messages for later processing.
///
/// Its role is similar to [`Listener`], except that it is permitted and expected to `&mut` mutate
/// itself, but **should not** communicate outside itself.
/// A [`Listener`] is a sort of channel by which to transmit messages;
/// a `Store` is a data structure which is a destination for messages.
///
/// After implementing `Store`, wrap it in a [`StoreLock`] to make use of it.
///
/// Generally, a `Store` implementation will combine and de-duplicate messages in some
/// fashion. For example, if the incoming messages were notifications of modified regions of data
/// as rectangles, then one might define a `Store` owning an `Option<Rect>` that contains the union
/// of all the rectangle messages, as an adequate constant-space approximation of the whole.
///
/// The type parameter `M` is the type of messages to be received.
///
/// TODO: give example
pub trait Store<M> {
    /// Record the given series of messages.
    ///
    /// # Requirements on implementors
    ///
    /// * Messages are provided in a batch for efficiency of dispatch.
    ///   Each message in the provided slice should be processed exactly the same as if
    ///   it were the only message provided.
    ///   If the slice is empty, there should be no observable effect.
    ///
    /// * Do not panic under any possible incoming message stream,
    ///   in order to ensure the sender's other work is not interfered with.
    ///
    /// * Do not acquire any locks, for performance and to avoid
    ///   deadlock with locks held by the sender.
    ///   (Normally, locking is to be provided separately, e.g. by [`StoreLock`].)
    ///  
    /// * Do not perform any blocking operation except for such locks.
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

/// Records messages delivered via [`Listener`] into a value of type `T` which implements
/// [`Store`].
///
/// This value is referred to as the “state”, and it is kept inside a mutex.
#[derive(Default)]
pub struct StoreLock<T: ?Sized>(MaRc<maybe_sync::Mutex<T>>);

/// [`StoreLock::listener()`] implementation.
///
/// You should not usually need to use this type explicitly.
pub struct StoreLockListener<T: ?Sized>(MaWeak<maybe_sync::Mutex<T>>);

impl<T: ?Sized + fmt::Debug> fmt::Debug for StoreLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(mutex) = self;

        // It is acceptable to lock the mutex for the same reasons it’s acceptable to lock it
        // during `Listener::receive()`: because it should only be held for short periods
        // and without taking any other locks.
        f.debug_tuple("StoreLock").field(&&*mutex.lock()).finish()
    }
}

impl<T: ?Sized> fmt::Debug for StoreLockListener<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreLockListener")
            // The type name of T may give a useful clue about who this listener is for,
            // without being too verbose or nondeterministic by printing the whole current state.
            .field("type", &core::any::type_name::<T>().refmt(&Unquote))
            // not useful to print weak_target unless we were to upgrade and lock it
            .field("alive", &(self.0.strong_count() > 0))
            .finish()
    }
}

impl<T> StoreLock<T> {
    /// Construct a new [`StoreLock`] with the given initial state.
    pub fn new(initial_state: T) -> Self {
        Self(MaRc::new(maybe_sync::Mutex::new(initial_state)))
    }
}

impl<T: ?Sized> StoreLock<T> {
    /// Returns a [`Listener`] which delivers messages to this.
    #[must_use]
    pub fn listener(&self) -> StoreLockListener<T> {
        StoreLockListener(MaRc::downgrade(&self.0))
    }

    /// Locks and returns access to the state.
    ///
    /// Callers should be careful to hold the lock for a very short time (e.g. only to copy or
    /// [take](core::mem::take) the data) or to do so only while messages will not be arriving.
    ///
    /// # Panics
    ///
    /// If it is called while the same thread has already acquired the lock, it may panic or hang,
    /// depending on the mutex implementation in use.
    #[must_use]
    pub fn lock(&self) -> impl core::ops::DerefMut<Target = T> + use<'_, T> {
        self.0.lock()
    }

    /// Delivers messages like `self.listener().receive(messages)`,
    /// but without creating a temporary listener.
    pub fn receive<M>(&self, messages: &[M])
    where
        T: Store<M>,
    {
        receive_bare_mutex(&self.0, messages);
    }
}

impl<M, T: ?Sized + Store<M> + Send> Listener<M> for StoreLockListener<T> {
    fn receive(&self, messages: &[M]) -> bool {
        let Some(strong) = self.0.upgrade() else {
            return false;
        };
        if messages.is_empty() {
            // skip acquiring lock
            return true;
        }
        receive_bare_mutex(&*strong, messages)
    }
}

fn receive_bare_mutex<M, T: ?Sized + Store<M>>(
    mutex: &maybe_sync::Mutex<T>,
    messages: &[M],
) -> bool {
    mutex.lock().receive(messages);
    true
}

// TODO: Provide an alternative to `StoreLock` which doesn't hand out access to the mutex
// but only swaps.

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use core::mem;

    #[test]
    fn store_lock_debug() {
        let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
        assert_eq!(format!("{sl:?}"), "StoreLock([\"initial\"])");
    }

    #[test]
    fn store_lock_listener_debug() {
        let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
        let listener = sl.listener();
        assert_eq!(
            format!("{listener:?}"),
            "StoreLockListener { type: alloc::vec::Vec<&str>, alive: true }"
        );
        drop(sl);
        assert_eq!(
            format!("{listener:?}"),
            "StoreLockListener { type: alloc::vec::Vec<&str>, alive: false }"
        );
    }

    #[test]
    fn store_lock_basics() {
        let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
        let listener = sl.listener();

        // Receive one message and see it added to the initial state
        assert_eq!(listener.receive(&["foo"]), true);
        assert_eq!(mem::take(&mut *sl.lock()), vec!["initial", "foo"]);

        // Receive multiple messages in multiple batches
        assert_eq!(listener.receive(&["bar", "baz"]), true);
        assert_eq!(listener.receive(&["separate"]), true);
        assert_eq!(mem::take(&mut *sl.lock()), vec!["bar", "baz", "separate"]);

        // Receive after drop
        drop(sl);
        assert_eq!(listener.receive(&["too late"]), false);
    }

    #[test]
    fn store_lock_receive_inherent() {
        let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
        sl.receive(&["from inherent receive"]);

        assert_eq!(
            mem::take(&mut *sl.lock()),
            vec!["initial", "from inherent receive"]
        );
    }

    /// For consistency with `no_std` mode, we *do not* offer mutex poisoning,
    /// even if the underlying implementation does.
    /// In exchange, the guard is not `UnwindSafe` either.
    #[test]
    fn store_lock_not_poisoned() {
        let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
        let listener = sl.listener();

        // Try to poison the mutex by panicking inside it
        let _ = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            let _guard = sl.lock();
            panic!("poison");
        }));

        // Listener still works.
        assert_eq!(listener.receive(&["foo"]), true);

        // Inherent receive still works.
        sl.receive(&["bar"]);

        // Locking still works.
        assert_eq!(mem::take(&mut *sl.lock()), vec!["initial", "foo", "bar"]);
    }
}
