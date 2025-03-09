use core::fmt;

use crate::{sync, unsync, Filter, Gate};

#[cfg(doc)]
use crate::{Notifier, Store, StoreLock};

#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
/// A receiver of messages (typically from something implementing [`Listen`]) which can
/// indicate when it is no longer interested in them (typically because the associated
/// recipient has been dropped).
///
/// Listeners are typically used in trait object form, which may be created via the
/// [`IntoDynListener`] trait in addition to the usual coercions;
/// this is done automatically by [`Listen`], but calling it earlier may be useful to minimize
/// the number of separately allocated clones of the listener when the same listener is
/// to be registered with multiple message sources.
///
/// Please note the requirements set out in [`Listener::receive()`].
///
/// Consider implementing [`Store`] and using [`StoreLock`] instead of implementing [`Listener`].
/// [`StoreLock`] provides the weak reference and mutex that are needed in the most common
/// kind of use of [`Listener`].
///
/// # Generic parameters
///
/// * `M` is the type of message that can be received.
pub trait Listener<M>: fmt::Debug {
    /// Process and store the given series of messages.
    ///
    /// Returns `true` if the listener is still interested in further messages (“alive”),
    /// and `false` if it should be dropped because these and all future messages would have
    /// no observable effect.
    /// A call of the form `.receive(&[])` may be performed to query aliveness without
    /// delivering any messages.
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
    ///   For example, if the listener accesses a poisoned mutex, it should do nothing or clear
    ///   the poison, rather than panicking.
    ///
    /// * Do not acquire any locks except ones which are used only for the state of the
    ///   listener itself.
    ///  
    /// * Do not perform any blocking operation except for such locks.
    ///
    /// * Do not access thread-local state, since this may be called from whichever thread(s)
    ///   the sender is using.
    ///
    /// # Advice for implementors
    ///
    /// Note that, since this method takes `&Self`, a `Listener` must use interior
    /// mutability of some variety to store the message. As a `Listener` may be called
    /// from various contexts, and in particular while the sender is still performing
    /// its work, that mutability should in general be limited to setting dirty flags
    /// or inserting into message queues — not attempting to directly perform further
    /// game state changes, and particularly not taking any locks that are not solely
    /// used by the `Listener` and its destination, as that could result in deadlock.
    ///
    /// The typical pattern is for a listener to contain a `Weak<Mutex<...>>` or similar
    /// multiply-owned mutable structure to aggregate incoming messages, which will
    /// then be read and cleared by a later task; see [`StoreLock`] for assistance in
    /// implementing this pattern.
    ///
    /// Note that a [`Notifier`] might call `.receive(&[])` at any time, particularly when
    /// listeners are added. Be careful not to cause a deadlock in this case; it may be
    /// necessary to avoid locking in the case where there are no messages to be delivered.
    fn receive(&self, messages: &[M]) -> bool;

    /// Convert this listener into trait object form, allowing it to be stored in
    /// collections or passed non-generically.
    /// The produced trait object does not implement [`Sync`].
    ///
    /// The purpose of this method over simply calling [`Arc::new()`] is that it will
    /// avoid double-wrapping of a listener that's already in [`Arc`].
    ///
    /// **You should not need to override or call this method;** use [`IntoDynListener`] instead.
    #[doc(hidden)]
    fn into_dyn_listener_unsync(self) -> crate::unsync::DynListener<M>
    where
        Self: Sized + 'static,
    {
        alloc::rc::Rc::new(self)
    }

    /// Convert this listener into trait object form, allowing it to be stored in
    /// collections or passed non-generically.
    /// The produced trait object implements [`Sync`].
    ///
    /// The purpose of this method over simply calling [`Arc::new()`] is that it will
    /// avoid double-wrapping of a listener that's already in [`Arc`].
    ///
    /// **You should not need to override or call this method;** use [`IntoDynListener`] instead.
    #[doc(hidden)]
    fn into_dyn_listener_sync(self) -> crate::sync::DynListener<M>
    where
        Self: Sized + Send + Sync + 'static,
    {
        alloc::sync::Arc::new(self)
    }

    /// Wraps `self` so to apply a map/filter function (similar to [`Iterator::filter_map()`])
    /// to incoming messages, to discard uninteresting messages and transform interesting ones.
    ///
    /// Note: By default, this filter breaks up all message batching into batches of 1.
    /// In order to avoid this and have more efficient message delivery, use
    /// [`Filter::with_stack_buffer()`].
    /// This is unnecessary if `size_of::<M>() == 0`; the buffer is automatically unbounded in
    /// that case.
    ///
    /// # Example
    ///
    /// ```
    /// use nosy::{unsync::Notifier, Flag, Listen as _, Listener as _};
    ///
    /// let notifier = Notifier::new();
    /// let flag = Flag::new(false);
    /// notifier.listen(flag.listener().filter(|&msg: &i32| {
    ///     if msg >= 0 {
    ///         Some(())
    ///     } else {
    ///         None
    ///     }
    /// }));
    ///
    /// // This message is filtered out.
    /// notifier.notify(&-1);
    /// assert_eq!(flag.get_and_clear(), false);
    ///
    /// // This message is passed through:
    /// notifier.notify(&2);
    /// assert_eq!(flag.get_and_clear(), true);
    /// ```
    fn filter<MI, F>(self, function: F) -> Filter<F, Self, 1>
    where
        Self: Sized,
        F: for<'a> Fn(&'a MI) -> Option<M>,
    {
        Filter {
            function,
            target: self,
        }
    }

    /// Wraps `self` to pass messages only until the returned [`Gate`], and any clones
    /// of it, are dropped.
    ///    
    /// This may be used to stop forwarding messages when a dependency no longer exists.
    ///
    /// ```
    /// use nosy::{Listen as _, Listener as _};
    ///
    /// let log = nosy::Log::new();
    /// let (gate, gated) = log.listener().gate();
    /// gated.receive(&["kept1"]);
    /// assert_eq!(log.drain(), vec!["kept1"]);
    /// gated.receive(&["kept2"]);
    /// drop(gate);
    /// gated.receive(&["discarded"]);
    /// assert_eq!(log.drain(), vec!["kept2"]);
    /// ```
    fn gate(self) -> (Gate, crate::GateListener<Self>)
    where
        Self: Sized,
    {
        Gate::new(self)
    }
}

// -------------------------------------------------------------------------------------------------
// Type-erasure related traits and impls.

/// Conversion from a concrete listener type to (normally) some flavor of boxed trait object.
///
/// This trait is a helper for [`Listen`] and generally does not need to be implemented,
/// unless you are using a custom type for your type-erased listeners that is neither
/// [`sync::DynListener`] nor [`unsync::DynListener`].
///
/// # Generic parameters
///
/// * `Self` is the listener type being converted from.
/// * `M` is the type of message accepted by the listener.
/// * `L` is the listener type being converted to.
pub trait IntoDynListener<M, L: Listener<M>>: Listener<M> {
    /// Wrap this [`Listener`] into a type-erased form of type `L`.
    fn into_dyn_listener(self) -> L;
}

impl<L, M> IntoDynListener<M, sync::DynListener<M>> for L
where
    L: Listener<M> + Send + Sync + 'static,
{
    fn into_dyn_listener(self) -> sync::DynListener<M> {
        self.into_dyn_listener_sync()
    }
}

impl<L, M> IntoDynListener<M, unsync::DynListener<M>> for L
where
    L: Listener<M> + 'static,
{
    fn into_dyn_listener(self) -> unsync::DynListener<M> {
        self.into_dyn_listener_unsync()
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

// -------------------------------------------------------------------------------------------------

/// Ability to subscribe to a source of messages, causing a [`Listener`] to receive them
/// as long as it wishes to.
///
/// # Examples
///
/// It is common to implement [`Listen`] so as to delegate to a [`Notifier`].
/// Such an implementation is written as follows:
///
/// ```
/// use nosy::{Listen, unsync::Notifier};
///
/// struct MyType {
///     notifier: Notifier<MyMessage>,
/// }
/// struct MyMessage;
///
/// impl Listen for MyType {
///     // This message type must equal the Notifier’s message type.
///     type Msg = MyMessage;
///
///     // This part is boilerplate.
///     type Listener = <Notifier<Self::Msg> as Listen>::Listener;
///     fn listen_raw(&self, listener: Self::Listener) {
///         self.notifier.listen(listener)
///     }
/// }
/// ```
pub trait Listen {
    /// The type of message which may be obtained from this source.
    ///
    /// Most message types should satisfy `Copy + Send + Sync + 'static`, but this is not required.
    type Msg;

    /// The type which all added listeners must be convertible to.
    type Listener: Listener<Self::Msg>;

    /// Subscribe the given [`Listener`] to this source of messages.
    ///
    /// Note that listeners are removed only via their returning [`false`] from
    /// [`Listener::receive()`]; there is no operation to remove a listener,
    /// and redundant subscriptions will result in redundant messages.
    ///
    /// By default, this method is equivalent to
    ///
    /// ```ignore
    /// self.listen_raw(listener.into_dyn_listener())
    /// ```
    fn listen<L: IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L)
    where
        Self: Sized,
    {
        self.listen_raw(listener.into_dyn_listener())
    }

    /// Subscribe the given [`Listener`] to this source of messages.
    ///
    /// Compared to `listen()`, `listen_raw()` requires that the given listener be of exactly the
    /// type that it will be stored as, rather than automatically wrapping it via the
    /// [`IntoDynListener`]trait. In exchange, it can be used when [`IntoDynListener`] is not
    /// implemented, or with `dyn Listen`.
    /// Also, it is the method which implementors of `Listen` must implement.
    ///
    /// Note that listeners are removed only via their returning [`false`] from
    /// [`Listener::receive()`]; there is no operation to remove a listener,
    /// and redundant subscriptions will result in redundant messages.
    fn listen_raw(&self, listener: Self::Listener);
}

impl<T: ?Sized + Listen> Listen for &T {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        (**self).listen_raw(listener)
    }
}
impl<T: ?Sized + Listen> Listen for &mut T {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        (**self).listen_raw(listener)
    }
}
impl<T: ?Sized + Listen> Listen for alloc::boxed::Box<T> {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        (**self).listen_raw(listener)
    }
}
impl<T: ?Sized + Listen> Listen for alloc::rc::Rc<T> {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        (**self).listen_raw(listener)
    }
}
impl<T: ?Sized + Listen> Listen for alloc::sync::Arc<T> {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        (**self).listen_raw(listener)
    }
}
