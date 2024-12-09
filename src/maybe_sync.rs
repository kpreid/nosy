use core::marker::PhantomData;
use core::{fmt, ops};

// Maybe Arc, maybe Rc.
// Since both types are in `alloc`, the only thing this saves is atomic refcounting operations,
// but we might as well.
cfg_if::cfg_if! {
    if #[cfg(feature = "sync")] {
        pub(crate) type MaRc<T> = alloc::sync::Arc<T>;
        pub(crate) type MaWeak<T> = alloc::sync::Weak<T>;
    } else {
        pub(crate) type MaRc<T> = alloc::rc::Rc<T>;
        pub(crate) type MaWeak<T> = alloc::rc::Weak<T>;
    }
}

/// Wrapper around [`core::cell::RefCell`] or [`std::sync::Mutex`] depending on whether
/// the `std` feature is enabled.
///
/// # Caution!
///
/// * This may or may not be `Sync`.
/// * This does not offer mutex poisoning.
/// * This may or may not deadlock if locked again from the same thread.
#[derive(Default)]
#[must_use]
pub(crate) struct Mutex<T: ?Sized> {
    /// We want to be *not* `RefUnwindSafe`, just like `RefCell` is not.
    /// Since `RefUnwindSafe` is an auto trait, we have to do this circuitously
    /// by taking advantage of `dyn`’s “nothing but what you specify”.
    _phantom: PhantomData<dyn Send + Sync + Unpin /* + !UnwindSafe + !RefUnwindSafe */>,

    lock: InnerMutex<T>,
}

#[allow(missing_debug_implementations)]
#[must_use]
pub(crate) struct MutexGuard<'a, T: ?Sized> {
    guard: InnerMutexGuard<'a, T>,
    _phantom: PhantomData<dyn Sync /* + !Send + !UnwindSafe + !RefUnwindSafe */>,
}

/// Wrapper around [`core::cell::RefCell`] or [`std::sync::RwLock`] depending on whether
/// the `std` feature is enabled.
///
/// # Caution!
///
/// * This may or may not be `Sync`.
/// * This does not offer mutex poisoning.
/// * This may or may not deadlock if locked again from the same thread.
#[derive(Default)]
pub(crate) struct RwLock<T: ?Sized> {
    /// We want to be *not* `RefUnwindSafe`, just like `RefCell` is not.
    /// Since `RefUnwindSafe` is an auto trait, we have to do this circuitously
    /// by taking advantage of `dyn`’s “nothing but what you specify”.
    _phantom: PhantomData<dyn Send + Sync + Unpin /* + !UnwindSafe + !RefUnwindSafe */>,

    lock: InnerRwLock<T>,
}

pub(crate) struct RwLockReadGuard<'a, T: ?Sized> {
    guard: InnerRwLockReadGuard<'a, T>,
    _phantom: PhantomData<dyn Sync /* + !Send + !UnwindSafe + !RefUnwindSafe */>,
}
pub(crate) struct RwLockWriteGuard<'a, T: ?Sized> {
    guard: InnerRwLockWriteGuard<'a, T>,
    _phantom: PhantomData<dyn Sync /* + !Send + !UnwindSafe + !RefUnwindSafe */>,
}

cfg_if::cfg_if! {
    if #[cfg(feature = "sync")] {
        type InnerMutex<T> = std::sync::Mutex<T>;
        type InnerMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;
        type InnerRwLock<T> = std::sync::RwLock<T>;
        type InnerRwLockReadGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;
        type InnerRwLockWriteGuard<'a, T> = std::sync::RwLockWriteGuard<'a, T>;
    } else {
        type InnerMutex<T> = core::cell::RefCell<T>;
        type InnerMutexGuard<'a, T> = core::cell::RefMut<'a, T>;
        type InnerRwLock<T> = core::cell::RefCell<T>;
        type InnerRwLockReadGuard<'a, T> = core::cell::Ref<'a, T>;
        type InnerRwLockWriteGuard<'a, T> = core::cell::RefMut<'a, T>;
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lock.fmt(f)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lock.fmt(f)
    }
}

impl<T> Mutex<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self {
            lock: InnerMutex::new(value),
            _phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                let guard = unpoison(self.lock.lock());
            } else {
                let guard = self.lock.borrow_mut();
            }
        }
        MutexGuard {
            guard,
            _phantom: PhantomData,
        }
    }
}

impl<T> RwLock<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self {
            lock: InnerRwLock::new(value),
            _phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> RwLock<T> {
    pub(crate) fn read(&self) -> RwLockReadGuard<'_, T> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                let guard = unpoison(self.lock.read());
            } else {
                let guard = self.lock.borrow();
            }
        }
        RwLockReadGuard {
            guard,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn write(&self) -> RwLockWriteGuard<'_, T> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                let guard = unpoison(self.lock.write());
            } else {
                let guard = self.lock.borrow_mut();
            }
        }
        RwLockWriteGuard {
            guard,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, TryLockError> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                use std::sync::TryLockError as E;
                let result = match self.lock.try_read() {
                    Ok(guard) => Ok(guard),
                    Err(E::Poisoned(error)) => Ok(error.into_inner()),
                    Err(E::WouldBlock) => Err(TryLockError),
                };
            } else {
                let result = match self.lock.try_borrow() {
                    Ok(guard) => Ok(guard),
                    Err(core::cell::BorrowError {..}) => Err(TryLockError),
                };
            }
        }
        result.map(|guard| RwLockReadGuard {
            guard,
            _phantom: PhantomData,
        })
    }
}

impl<T: ?Sized> ops::Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}
impl<T: ?Sized> ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}
impl<T: ?Sized> ops::Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}
impl<T: ?Sized> ops::Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}
impl<T: ?Sized> ops::DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

// Doesn't implement Error because it would be dead code.
#[derive(Debug)]
pub(crate) struct TryLockError;

#[cfg(feature = "sync")]
fn unpoison<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    match result {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    }
}
