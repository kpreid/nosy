use core::mem;
use core::sync::atomic::{self, Ordering::Relaxed};

// -------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "std-sync"), allow(rustdoc::broken_intra_doc_links))]
/// Interior-mutable storage for a value that can be replaced or copied.
///
/// Types implementing this trait can be used to provide interior mutability for
/// [`nosy::Cell`][crate::Cell].
///
/// This trait is implemented for
/// [`core::cell::Cell`],
/// [`std::sync::Mutex`] when the `"sync"` or `"std"` features are enabled,
/// and all [atomic types][core::sync::atomic].
/// You can implement it in order to make use of a `Mutex` type other than the one from [`std`].
///
pub trait LoadStore {
    /// The type of value this container stores.
    ///
    /// For example, `<Mutex<T> as LoadStore>::Value = T`.
    type Value;

    /// Creates a new interior-mutable container whose initial value is `value`.
    fn new(value: Self::Value) -> Self
    where
        Self: Sized;

    /// Returns a clone of the value currently in this interior-mutable container.
    ///
    /// # Panics
    ///
    /// Implementations may panic or hang (deadlock) if called within [`Self::Value`]’s
    /// [`Clone`] or [`PartialEq`] operations, thus potentially causing reentrancy from within
    /// [`Self::replace()`] or [`Self::replace_if_unequal()`].
    fn get(&self) -> Self::Value;

    /// Replaces the value currently in this interior-mutable container,
    /// and returns the old value.
    ///
    /// # Panics
    ///
    /// Implementations may panic or hang (deadlock) if called within [`Self::Value`]’s
    /// [`Clone`] or [`PartialEq`] operations, thus potentially causing reentrancy from within
    /// [`Self::get()`] or [`Self::replace_if_unequal()`].
    ///
    // API design note: By returning the value, even if the caller doesn’t want to do anything with
    // it, we allow the caller to control the timing of the drop of the value, and encourage
    // implementations not to drop the value with a mutex held.
    fn replace(&self, new_value: Self::Value) -> Self::Value;

    /// Compares the current value to `new_value` and replaces it if it is unequal.
    ///
    /// # Errors
    ///
    /// Returns `Ok(old_value)` if the value was different, or `Err(new_value)` if it was the same.
    ///
    /// # Panics
    ///
    /// Implementations may panic or hang (deadlock) if called within [`Self::Value`]’s
    /// [`Clone`] or [`PartialEq`] operations, thus potentially causing reentrancy from within
    /// [`Self::get()`] or [`Self::replace()`].
    fn replace_if_unequal(&self, new_value: Self::Value) -> Result<Self::Value, Self::Value>
    where
        Self::Value: PartialEq;
}

// -------------------------------------------------------------------------------------------------
// `core::cell` impls

impl<T: Copy> LoadStore for core::cell::Cell<T> {
    type Value = T;
    fn new(value: T) -> Self {
        core::cell::Cell::new(value)
    }
    fn get(&self) -> T {
        core::cell::Cell::get(self)
    }
    fn replace(&self, new_value: T) -> T {
        core::cell::Cell::replace(self, new_value)
    }
    fn replace_if_unequal(&self, new_value: Self::Value) -> Result<T, T>
    where
        Self::Value: PartialEq,
    {
        if new_value == core::cell::Cell::get(self) {
            Err(new_value)
        } else {
            Ok(core::cell::Cell::replace(self, new_value))
        }
    }
}

impl<T: Clone> LoadStore for core::cell::RefCell<T> {
    type Value = T;
    fn new(value: T) -> Self {
        core::cell::RefCell::new(value)
    }
    fn get(&self) -> T {
        self.borrow().clone()
    }
    fn replace(&self, new_value: T) -> T {
        mem::replace(&mut *self.borrow_mut(), new_value)
    }
    fn replace_if_unequal(&self, new_value: Self::Value) -> Result<T, T>
    where
        Self::Value: PartialEq,
    {
        let mut guard: core::cell::RefMut<'_, T> = self.borrow_mut();
        if new_value == *guard {
            Err(new_value)
        } else {
            Ok(mem::replace(&mut *guard, new_value))
        }
    }
}

// -------------------------------------------------------------------------------------------------
// `std::sync` impls

#[cfg(feature = "std")]
impl<T: Clone> LoadStore for std::sync::Mutex<T> {
    type Value = T;
    fn new(value: T) -> Self {
        std::sync::Mutex::new(value)
    }
    fn get(&self) -> T {
        unpoison(self.lock()).clone()
    }
    fn replace(&self, new_value: T) -> T {
        mem::replace(&mut *unpoison(self.lock()), new_value)
    }
    fn replace_if_unequal(&self, new_value: Self::Value) -> Result<T, T>
    where
        Self::Value: PartialEq,
    {
        let mut guard: std::sync::MutexGuard<'_, T> = unpoison(self.lock());
        if new_value == *guard {
            Err(new_value)
        } else {
            Ok(mem::replace(&mut *guard, new_value))
        }
    }
}

#[cfg(feature = "std")]
impl<T: Clone> LoadStore for std::sync::RwLock<T> {
    type Value = T;
    fn new(value: T) -> Self {
        std::sync::RwLock::new(value)
    }
    fn get(&self) -> T {
        unpoison(self.read()).clone()
    }
    fn replace(&self, new_value: T) -> T {
        mem::replace(&mut *unpoison(self.write()), new_value)
    }
    fn replace_if_unequal(&self, new_value: Self::Value) -> Result<T, T>
    where
        Self::Value: PartialEq,
    {
        let mut guard: std::sync::RwLockWriteGuard<'_, T> = unpoison(self.write());
        if new_value == *guard {
            Err(new_value)
        } else {
            Ok(mem::replace(&mut *guard, new_value))
        }
    }
}

/// Lock poisoning can be ignored, because the only way we ever modify the value in the mutex
/// is by [`mem::replace`] which does not panic, so the value is, at worst, stale, not
/// internally inconsistent.
#[cfg(feature = "std")]
fn unpoison<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    match result {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    }
}

// -------------------------------------------------------------------------------------------------
// `core::sync::atomic` impls

macro_rules! impl_load_store_for_atomic {
    ($width:literal, $base_type:ident, $atomic_type:ident) => {
        #[cfg(target_has_atomic = $width)]
        impl LoadStore for atomic::$atomic_type {
            type Value = $base_type;

            fn new(value: Self::Value) -> Self {
                Self::new(value)
            }

            fn get(&self) -> Self::Value {
                Self::load(self, Relaxed)
            }

            fn replace(&self, new_value: Self::Value) -> Self::Value {
                Self::swap(self, new_value, Relaxed)
            }

            fn replace_if_unequal(
                &self,
                new_value: Self::Value,
            ) -> Result<Self::Value, Self::Value> {
                let old_value = Self::swap(self, new_value, Relaxed);
                if old_value == new_value {
                    Err(new_value)
                } else {
                    Ok(old_value)
                }
            }
        }
    };
}

impl_load_store_for_atomic!("8", bool, AtomicBool);
impl_load_store_for_atomic!("8", u8, AtomicU8);
impl_load_store_for_atomic!("8", i8, AtomicI8);
impl_load_store_for_atomic!("16", u16, AtomicU16);
impl_load_store_for_atomic!("16", i16, AtomicI16);
impl_load_store_for_atomic!("32", u32, AtomicU32);
impl_load_store_for_atomic!("32", i32, AtomicI32);
impl_load_store_for_atomic!("64", u64, AtomicU64);
impl_load_store_for_atomic!("64", i64, AtomicI64);
// impl_for_atomic!("128", u128, AtomicU128); // unstable <https://github.com/rust-lang/rust/issues/99069>
// impl_for_atomic!("128", i128, AtomicI128);
impl_load_store_for_atomic!("ptr", usize, AtomicUsize);
impl_load_store_for_atomic!("ptr", isize, AtomicIsize);

impl<T> LoadStore for atomic::AtomicPtr<T> {
    type Value = *mut T;

    fn new(value: Self::Value) -> Self {
        Self::new(value)
    }

    fn get(&self) -> Self::Value {
        Self::load(self, Relaxed)
    }

    fn replace(&self, new_value: Self::Value) -> Self::Value {
        Self::swap(self, new_value, Relaxed)
    }

    fn replace_if_unequal(&self, new_value: Self::Value) -> Result<Self::Value, Self::Value> {
        let old_value = Self::swap(self, new_value, Relaxed);
        if old_value == new_value {
            Err(new_value)
        } else {
            Ok(old_value)
        }
    }
}

// TODO: Also implement for `lock_api` and `arc_swap` optionally
