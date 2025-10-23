use core::fmt;

use crate::maybe_sync::{self, MaRc, MaWeak};
use crate::{Listener, Store};

// -------------------------------------------------------------------------------------------------

/// Records messages delivered via [`Listener`] by mutating a value which implements [`Store`].
///
/// This value is referred to as the “state”, and it is kept inside a mutex.
///
/// # Generic parameters
///
/// * `T` is the type of the state, which should implement [`Store`].

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

impl<T: ?Sized> fmt::Pointer for StoreLock<T> {
    /// Produces an address which is the same for this [`StoreLock`] and its associated
    /// [`StoreLockListener`]s.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        MaRc::as_ptr(&self.0).fmt(f)
    }
}
impl<T: ?Sized> fmt::Pointer for StoreLockListener<T> {
    /// Produces an address which is the same for this [`StoreLockListener`] and its associated
    /// [`StoreLock`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.as_ptr().fmt(f)
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
    /// It is equivalent to:
    ///
    /// ```no_run
    /// # use core::mem;
    /// # trait Ext<T> { fn take(&self) -> T; }
    /// # impl<T: Default> Ext<T> for nosy::StoreLock<T> { fn take(&self) -> T {
    /// let replacement = T::default();
    /// mem::replace(&mut *self.lock(), replacement)
    /// # } }
    /// ```
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

impl<T: ?Sized + Store<M> + Send, M> crate::FromListener<StoreLockListener<T>, M>
    for StoreLockListener<T>
{
    /// No-op conversion returning the listener unchanged.
    fn from_listener(listener: StoreLockListener<T>) -> Self {
        listener
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
