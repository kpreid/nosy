//! Type-erasure related traits and impls.

use crate::{sync, unsync, Listener};

#[cfg(doc)]
use crate::{Listen, Notifier};

// -------------------------------------------------------------------------------------------------
// Traits

#[cfg_attr(not(feature = "async"), allow(rustdoc::broken_intra_doc_links))]
/// Conversion from a specific listener type to a more general listener type,
/// such as a trait object.
///
/// This trait is typically used via calling [`Listen::listen()`], or an [`IntoListener`] bound
/// on a function’s parameter, to convert listeners of the provided type into a more common type
/// that can be stored in a [`Notifier`]’s listener set.
///
/// # Generic parameters
///
/// * `Self` is the listener type being converted to.
/// * `L` is the listener type being converted from.
/// * `M` is the type of message accepted by the listener.
///
/// # When to implement `FromListener`
///
/// There are two kinds of implementations of `FromListener`:
///
/// * Conversion to trait objects, like [`sync::DynListener`] and [`unsync::DynListener`], or enums.
///   You would write such an implementation if you are using a custom type for your type-erased
///   listeners (e.g. to add more required traits to allow more inspection of the listeners).
///
/// * Reflexive implementations, allowing the use of a [`Notifier`] that accepts only one type of
///   listener and does no dynamic dispatch.
///   You may write such an implementation, `impl FromListener<MyListener, MyMsg> for MyListener`,
///   whenever you implement `Listener`.
///
///   Unfortunately, we cannot provide a blanket implementation of this type,
///   `impl<L: Listener<M>, M> FromListener<L, M> for L`,
///   because it would conflict with the other kind of implementation.
///
/// # Example implementation
///
/// Suppose that you want to use *only* [`Flag`][crate::Flag] and
/// [`WakeFlag`][crate::future::WakeFlag] listeners,
/// perhaps because they use atomic operations and no custom code or locks.
/// A custom implementation of [`FromListener`] can offer this restriction, and
/// potential performance improvement by avoiding indirection and allocation:
///
/// ```
#[cfg_attr(not(feature = "async"), doc = "# #[cfg(any())] mod dummy {")] // skip if nosy::future::WakeFlag doesn't exist
/// use nosy::Listen as _;
///
/// /// A non-boxing type for all `Flag` listeners and no other kinds of listener.
/// #[derive(Debug)]
/// enum AnyFlag {
///     NoWake(nosy::FlagListener),
///     Wake(nosy::future::WakeFlagListener),
/// }
///
/// // Dispatches to the listeners in each variant.
/// impl<M> nosy::Listener<M> for AnyFlag {
///     fn receive(&self, messages: &[M]) -> bool {
///        match self {
///            Self::NoWake(l) => l.receive(messages),
///            Self::Wake(l) => l.receive(messages),
///        }
///     }
/// }
///
/// // Whenever possible, provide a reflexive, idempotent implementation,
/// // so that an already-converted listener can still be passed to `listen()` methods.
/// impl<M> nosy::FromListener<AnyFlag, M> for AnyFlag {
///     fn from_listener(listener: AnyFlag) -> AnyFlag {
///         listener
///     }
/// }
///
/// // The actual work of conversion.
/// impl<M> nosy::FromListener<nosy::FlagListener, M> for AnyFlag {
///     fn from_listener(listener: nosy::FlagListener) -> AnyFlag {
///         AnyFlag::NoWake(listener)
///     }
/// }
/// impl<M> nosy::FromListener<nosy::future::WakeFlagListener, M> for AnyFlag {
///     fn from_listener(listener: nosy::future::WakeFlagListener) -> AnyFlag {
///         AnyFlag::Wake(listener)
///     }
/// }
///
/// fn example_usage(notifier: &nosy::Notifier<(), AnyFlag>) {
///     notifier.listen(nosy::Flag::new(false).listener());
/// }
#[cfg_attr(not(feature = "async"), doc = "# }")] // skip if nosy::future::WakeFlag doesn't exist
/// ```
///
/// # Why not `From`?
///
/// The reason we define this trait instead of using the standard [`From`] trait is that [coherence]
/// would prohibit providing the needed blanket implementations — we cannot write
///
/// ```ignore
/// impl<L, M> From<L> for nosy::unsync::DynListener<M>
/// where
///     L: nosy::Listener<M>,
/// # {
/// #     fn from(value: L) -> Self { todo!();}
/// # }
/// ```
///
/// because `L` is an uncovered type parameter and [`DynListener`][crate::unsync::DynListener]
/// is not a local type, and
/// even if that were not the case, this implementation would conflict with
/// the blanket implementation `impl<T> From<T> for T`.
///
/// [coherence]: https://doc.rust-lang.org/reference/items/implementations.html#trait-implementation-coherence
pub trait FromListener<L: Listener<M>, M>: Listener<M> {
    /// Convert the given [`Listener`] into `Self`.
    fn from_listener(listener: L) -> Self;
}

/// Conversion from a concrete listener type to (normally) some flavor of boxed trait object.
///
/// This trait is equivalent to [`FromListener`] but with the source and destination swapped.
/// It allows you to write concise bounds like
/// `fn my_listen(listener: impl IntoListener<DynListener<M>, M>)`.
/// Additionally, `IntoListener` bounds are [eligible for elaboration] whereas the corresponding
/// [`FromListener`] bound would not be, and thus would require an additional `L: Listener<M>`
/// bound on every function or impl.
///
/// This trait may not be implemented. Implement [`FromListener`] instead.
///
/// # Generic parameters
///
/// * `Self` is the listener type being converted from.
/// * `L` is the listener type being converted to.
/// * `M` is the type of message accepted by the listener.
///
/// # Example
///
/// [`IntoListener`] is used when writing functions that take listeners as parameters,
/// similar to [`Listen::listen()`]:
///
/// ```
/// use nosy::{Listen, unsync::DynListener};
///
/// struct MyCell {
///     value: core::cell::Cell<usize>,
///     notifier: nosy::Notifier<(), DynListener<()>>,
/// }
///
/// impl MyCell {
///     pub fn get_and_listen(
///         &self,
///         listener: impl nosy::IntoListener<DynListener<()>, ()>,
///     ) -> usize {
///         self.notifier.listen(listener);
///         self.value.get()
///     }
/// }
/// ```
///
/// [eligible for elaboration]: https://github.com/rust-lang/rust/issues/20671
pub trait IntoListener<L, M>: Sized + Listener<M> {
    /// Wrap this [`Listener`] into a type-erased form of type `L`.
    fn into_listener(self) -> L;

    #[doc(hidden)]
    fn _you_may_not_implement_this_trait(_: into_listener_is_sealed::Sealed);
}

// -------------------------------------------------------------------------------------------------
// Implementations

impl<LIn, LOut, M> IntoListener<LOut, M> for LIn
where
    LIn: Listener<M>,
    LOut: Listener<M> + FromListener<Self, M>,
{
    fn into_listener(self) -> LOut {
        LOut::from_listener(self)
    }

    #[doc(hidden)]
    fn _you_may_not_implement_this_trait(_: into_listener_is_sealed::Sealed) {}
}

mod into_listener_is_sealed {
    #[allow(unnameable_types, missing_debug_implementations)]
    pub struct Sealed;
}

impl<L, M> FromListener<L, M> for sync::DynListener<M>
where
    L: Listener<M> + Send + Sync + 'static,
{
    fn from_listener(listener: L) -> sync::DynListener<M> {
        listener.into_dyn_listener_sync()
    }
}

impl<L, M> FromListener<L, M> for unsync::DynListener<M>
where
    L: Listener<M> + 'static,
{
    fn from_listener(listener: L) -> unsync::DynListener<M> {
        listener.into_dyn_listener_unsync()
    }
}

impl<M> Listener<M> for unsync::DynListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        (**self).receive(messages)
    }

    fn into_dyn_listener_unsync(self) -> unsync::DynListener<M> {
        self
    }

    // into_dyn_listener_sync() is unimplementable because its bounds are not met.
}
impl<M> Listener<M> for sync::DynListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        (**self).receive(messages)
    }

    // into_dyn_listener_unsync() will result in double-wrapping.
    // That could be avoided by using `Arc` even for `unsync::DynListener`,
    // but that should be a rare case which is not worth the benefit of avoiding
    // unnecessary atomic operations when feature = "sync" isn't even enabled.

    fn into_dyn_listener_sync(self) -> sync::DynListener<M> {
        self
    }
}
