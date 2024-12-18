//! Containers for single values which notify when the value is changed.

#![allow(
    clippy::module_name_repetitions,
    reason = "false positive on private module; TODO: remove after Rust 1.84 is released"
)]

use core::fmt;
use core::marker::PhantomData;

use alloc::sync::Arc;

use crate::{Listen, Listener};

// -------------------------------------------------------------------------------------------------

/// Access to a value that might change and notifications when it does.
///
/// `Source`s should usually, but are not required to, implement [`Clone`] such that all clones
/// have identical future behavior (values returned and messages sent). They should implement
/// [`fmt::Debug`] in a way which identifies the source rather than only its current value.
///
/// The change notifications given are of type `()`; they do not allow access to the new value.
/// This is an unfortunate necessity to allow sources to deliver notifications
/// *after* the value has changed (i.e. while not holding any lock) without also needing a clone
/// of, or reference-counted pointer to, the value.
/// (Listeners are, as always, encouraged not to do significant work,
/// so, while a listener could *try* calling [`Source::get()`] immediately,
/// this is not the intended architecture and is not guaranteed to work.)
///
/// The type aliases [`sync::DynSource`](crate::sync::DynSource)
/// and [`unsync::DynSource`](crate::unsync::DynSource) are available for type-erased `Source`s
/// with type-erased `Listener`s. If you want a value and don’t care about the type of the source
/// it comes from, use them.
//---
// Design note: All `Source`s must implement `Debug`;
// ideally, only `DynSource` would have this requirement, but that is not possible since it would be
// a trait object with two non-marker traits, unless we also introduced another public trait
// dedicated to the purpose, which would be messy. As I see it, “everything must implement Debug”
// is not too onerous a requirement and a much better choice than “you can get no debug info”.
pub trait Source: Listen<Msg = ()> + fmt::Debug {
    /// The type of value which can be obtained from this source.
    ///
    /// This type should usually be clonable (because in any case, arbitrary copies of it
    /// can be obtained from [`Source::get()`]), but this is not required.
    type Value;

    /// Returns the most recent value.
    ///
    /// # Panics
    ///
    /// This method may, but is not required or expected to, panic if called from within
    /// one of this source’s listeners.
    #[must_use]
    fn get(&self) -> Self::Value;
}

impl<T: ?Sized + Source> Source for &T {
    type Value = T::Value;
    fn get(&self) -> Self::Value {
        T::get(*self)
    }
}
impl<T: ?Sized + Source> Source for &mut T {
    type Value = T::Value;
    fn get(&self) -> Self::Value {
        T::get(*self)
    }
}
impl<T: ?Sized + Source> Source for alloc::boxed::Box<T> {
    type Value = T::Value;
    fn get(&self) -> Self::Value {
        T::get(&**self)
    }
}
impl<T: ?Sized + Source> Source for alloc::rc::Rc<T> {
    type Value = T::Value;
    fn get(&self) -> Self::Value {
        T::get(&**self)
    }
}
impl<T: ?Sized + Source> Source for alloc::sync::Arc<T> {
    type Value = T::Value;
    fn get(&self) -> Self::Value {
        T::get(&**self)
    }
}

// -------------------------------------------------------------------------------------------------

/// A [`Source`] of a constant value; never sends a change notification.
///
/// # Generic parameters
///
/// * `T` is the type of the value.
/// * `L` is the type of [`Listener`] this source accepts but never uses
///   (necessary to implement [`Listen`]).
pub struct Constant<T, L> {
    value: T,
    _phantom: PhantomData<fn(L)>,
}

impl<T, L> Constant<T, L> {
    /// Constructs a [`Constant`] whose [`get()`](Source::get) always returns a clone of `value`.
    pub const fn new(value: T) -> Self {
        Self {
            value,
            _phantom: PhantomData,
        }
    }

    /// Destroys this [`Constant`] and returns the value it contained.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T, L: Listener<()>> Listen for Constant<T, L> {
    type Msg = ();
    type Listener = L;

    fn listen<L2: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, _: L2) {
        // do nothing, skipping the boxing that would happen if we only implemented listen_raw()
    }
    fn listen_raw(&self, _: Self::Listener) {}
}

// Design note: If it were possible, we would not have this `Debug` bound.
// It is necessary because `Source` requires `Debug` (see its comments), and our own `Debug`
// implementation prints the value (it would be useless otherwise).
impl<T: Clone + fmt::Debug, L: Listener<()>> Source for Constant<T, L> {
    type Value = T;
    fn get(&self) -> Self::Value {
        self.value.clone()
    }
}

impl<T: fmt::Debug, L> core::fmt::Debug for Constant<T, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Constant").field(&self.value).finish()
    }
}

impl<T: Copy, L> Copy for Constant<T, L> {}
impl<T: Clone, L> Clone for Constant<T, L> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<T: Eq, L> Eq for Constant<T, L> {}
impl<T: PartialEq, L> PartialEq for Constant<T, L> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: core::hash::Hash, L> core::hash::Hash for Constant<T, L> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: Ord, L> Ord for Constant<T, L> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}
impl<T: PartialOrd, L> PartialOrd for Constant<T, L> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Default, L> Default for Constant<T, L> {
    fn default() -> Self {
        Self {
            value: Default::default(),
            _phantom: PhantomData,
        }
    }
}

impl<T, L> AsRef<T> for Constant<T, L> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T, L> core::borrow::Borrow<T> for Constant<T, L> {
    fn borrow(&self) -> &T {
        &self.value
    }
}

impl<T, L> From<T> for Constant<T, L> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

// Convenience conversions directly to coerced trait object,
// instead of `Arc::new()` followed by coercion.
impl<T> From<Constant<T, crate::unsync::DynListener<()>>> for crate::unsync::DynSource<T>
where
    T: Clone + fmt::Debug + 'static,
{
    fn from(value: Constant<T, crate::unsync::DynListener<()>>) -> Self {
        Arc::new(value)
    }
}
impl<T> From<Constant<T, crate::sync::DynListener<()>>> for crate::sync::DynSource<T>
where
    T: Clone + fmt::Debug + Send + Sync + 'static,
{
    fn from(value: Constant<T, crate::sync::DynListener<()>>) -> Self {
        Arc::new(value)
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unsync;
    use crate::Sink;
    use alloc::format;
    use pretty_assertions::assert_eq;

    #[test]
    fn constant_source_debug() {
        let source = unsync::constant(123);
        assert_eq!(format!("{source:?}"), "Constant(123)");
        assert_eq!(format!("{source:#?}"), "Constant(\n    123,\n)");
    }

    #[test]
    fn constant_source_usage() {
        let sink: Sink<()> = Sink::new();
        let source = unsync::constant(123u64);
        source.listen(sink.listener()); // doesn't panic, doesn't do anything
        assert_eq!(source.get(), 123);
        assert_eq!(source.get(), 123);
        assert!(sink.drain().is_empty());
    }

    #[cfg(feature = "sync")] // because `Sink` isn't `Sync` otherwise
    #[test]
    fn constant_source_usage_sync() {
        let sink: Sink<()> = Sink::new();
        let source = crate::sync::constant(123u64);
        source.listen(sink.listener()); // doesn't panic, doesn't do anything
        assert_eq!(source.get(), 123);
        assert_eq!(source.get(), 123);
        assert!(sink.drain().is_empty());
    }
}
