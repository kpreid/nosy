use core::fmt;

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
///
/// # Generic parameters
///
/// * `M` is the type of message that can be received.
///
/// # Example
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

/// Records messages delivered via [`Listener`] by mutating a value which implements [`Store`].
///
/// This value is referred to as the “state”, and it is kept inside a mutex.
///
/// # Generic parameters
///
/// * `T` is the type of the state.

#[derive(Default)]
pub struct StoreLock<T: ?Sized>(MaRc<maybe_sync::Mutex<T>>);

/// [`StoreLock::listener()`] implementation.
///
/// You should not usually need to use this type explicitly.
///
/// # Generic parameters
///
/// * `T` is the type of the state.
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
            .field("type", &crate::util::Unquote::type_name::<T>())
            // not useful to print weak_target unless we were to upgrade and lock it
            .field("alive", &self.alive())
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
    /// Otherwise, poor performance or deadlock may result.
    ///
    /// # Panics
    ///
    /// If it is called while the same thread has already acquired the lock, it may panic or hang,
    /// depending on the mutex implementation in use.
    #[must_use]
    pub fn lock(&self) -> impl core::ops::DerefMut<Target = T> + use<'_, T> {
        self.0.lock()
    }

    /// Replaces the current state with [`T::default()`](Default) and returns it.
    ///
    /// This is not more powerful than [`lock()`](Self::lock),
    /// but it offers the guarantee that it will hold the lock for as little time as possible.
    /// It is equivalent to `mem::replace(&mut *self.0.lock(), T::default())`.
    ///
    /// # Panics
    ///
    /// If it is called while the same thread has already acquired the lock, it may panic or hang,
    /// depending on the mutex implementation in use.
    #[must_use]
    pub fn take(&self) -> T
    where
        T: Sized + Default,
    {
        // We are not using mem::take() so as to avoid executing `T::default()` with the lock held.
        let replacement: T = T::default();
        let state: &mut T = &mut *self.0.lock();
        core::mem::replace(state, replacement)
    }

    /// Delivers messages like `self.listener().receive(messages)`,
    /// but without creating a temporary listener.
    ///
    /// # Panics
    ///
    /// If it is called while the same thread has already acquired the lock, it may panic or hang,
    /// depending on the mutex implementation in use.
    pub fn receive<M>(&self, messages: &[M])
    where
        T: Store<M>,
    {
        receive_bare_mutex(&self.0, messages);
    }
}

impl<T: ?Sized> StoreLockListener<T> {
    /// Used in [`fmt::Debug`] implementations.
    pub(crate) fn alive(&self) -> bool {
        self.0.strong_count() > 0
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

impl<T: ?Sized> Clone for StoreLockListener<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
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
