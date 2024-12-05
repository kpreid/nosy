use core::error::Error;
use core::{fmt, ops};

// cfg_if::cfg_if! {
//     if #[cfg(feature = "sync")] {
//         pub(crate) trait SendSyncIfStd: Send + Sync {}
//         impl<T: Send + Sync> SendSyncIfStd for T {}
//     } else {
//         pub trait SendSyncIfStd {}
//         impl<T> SendSyncIfStd for T {}
//     }
// }

/// Wrapper around [`core::cell::RefCell`] or [`std::sync::Mutex`] depending on whether
/// the `std` feature is enabled.
///
/// # Caution!
///
/// * This may or may not be `Sync`.
/// * This may or may not implement mutex poisoning.
/// * This may or may not deadlock if locked again from the same thread.
#[derive(Default)]
#[must_use]
pub(crate) struct Mutex<T: ?Sized>(InnerMutex<T>);

#[allow(missing_debug_implementations)]
#[must_use]
pub(crate) struct MutexGuard<'a, T: ?Sized>(InnerMutexGuard<'a, T>);

/// Wrapper around [`core::cell::RefCell`] or [`std::sync::RwLock`] depending on whether
/// the `std` feature is enabled.
///
/// # Caution!
///
/// * This may or may not be `Sync`.
/// * This may or may not implement mutex poisoning.
/// * This may or may not deadlock if locked again from the same thread.
#[derive(Default)]
pub(crate) struct RwLock<T: ?Sized>(InnerRwLock<T>);

pub(crate) struct RwLockReadGuard<'a, T: ?Sized>(InnerRwLockReadGuard<'a, T>);
pub(crate) struct RwLockWriteGuard<'a, T: ?Sized>(InnerRwLockWriteGuard<'a, T>);

cfg_if::cfg_if! {
    if #[cfg(feature = "sync")] {
        type InnerMutex<T> = std::sync::Mutex<T>;
        type InnerMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;
        type InnerRwLock<T> = std::sync::RwLock<T>;
        type InnerRwLockReadGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;
        type InnerRwLockWriteGuard<'a, T> =  std::sync::RwLockWriteGuard<'a, T>;
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
        self.0.fmt(f)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Mutex<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(InnerMutex::new(value))
    }
}

impl<T: ?Sized> Mutex<T> {
    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, T>, LockError<MutexGuard<'_, T>>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                let result = self.0.lock()
                    .map_err(|e: std::sync::PoisonError<_>| LockError::Poisoned(MutexGuard(e.into_inner())));
            } else {
                let result = Ok(self.0.borrow_mut());
            }
        }

        result.map(MutexGuard)
    }
}

impl<T> RwLock<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(InnerRwLock::new(value))
    }
}

impl<T: ?Sized> RwLock<T> {
    pub(crate) fn read(&self) -> Result<RwLockReadGuard<'_, T>, LockError<RwLockReadGuard<'_, T>>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                let result = self.0.read()
                    .map_err(|e: std::sync::PoisonError<_>| LockError::Poisoned(RwLockReadGuard(e.into_inner())));
            } else {
                let result = Ok(self.0.borrow());
            }
        }

        result.map(RwLockReadGuard)
    }

    pub(crate) fn write(
        &self,
    ) -> Result<RwLockWriteGuard<'_, T>, LockError<RwLockWriteGuard<'_, T>>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                let result = self.0.write()
                    .map_err(|e: std::sync::PoisonError<_>| LockError::Poisoned(RwLockWriteGuard(e.into_inner())));
            } else {
                let result = Ok(self.0.borrow_mut());
            }
        }

        result.map(RwLockWriteGuard)
    }

    pub(crate) fn try_read(
        &self,
    ) -> Result<RwLockReadGuard<'_, T>, TryLockError<RwLockReadGuard<'_, T>>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "sync")] {
                use std::sync::TryLockError as E;
                let result = self.0.try_read().map_err(|e| match e {
                    E::Poisoned(pe) => TryLockError::Poisoned(RwLockReadGuard(pe.into_inner())),
                    E::WouldBlock => TryLockError::WouldBlock,
                });
            } else {
                let result = self.0.try_borrow()
                    .map_err(|core::cell::BorrowError {..}| TryLockError::WouldBlock);
            }
        }

        result.map(RwLockReadGuard)
    }
}

impl<T: ?Sized> ops::Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: ?Sized> ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<T: ?Sized> ops::Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: ?Sized> ops::Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: ?Sized> ops::DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(clippy::exhaustive_enums)]
pub(crate) enum LockError<G> {
    Poisoned(G),
}

impl<G> LockError<G> {
    // Not ever actually used at the time being.
    // pub(crate) fn into_inner(self) -> G {
    //     match self {
    //         LockError::Poisoned(g) => g,
    //     }
    // }
}

impl<G> Error for LockError<G> {}

impl<G> fmt::Display for LockError<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned(_) => write!(f, "lock was poisoned"),
        }
    }
}

impl<G> fmt::Debug for LockError<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned(_) => f.debug_struct("Poisoned").finish_non_exhaustive(),
        }
    }
}

pub(crate) enum TryLockError<G> {
    #[cfg_attr(
        not(feature = "sync"),
        expect(dead_code, reason = "no poisoning from RefCell")
    )]
    Poisoned(G),
    WouldBlock,
}

impl<G> Error for TryLockError<G> {}

impl<G> fmt::Display for TryLockError<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned(_) => write!(f, "lock was poisoned"),
            Self::WouldBlock => write!(f, "lock is currently locked elsewhere"),
        }
    }
}

impl<G> fmt::Debug for TryLockError<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned(_) => f.debug_struct("Poisoned").finish_non_exhaustive(),
            Self::WouldBlock => write!(f, "WouldBlock"),
        }
    }
}
