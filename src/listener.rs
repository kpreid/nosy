#![allow(
    clippy::module_name_repetitions,
    reason = "false positive; TODO: remove after Rust 1.84 is released"
)]

use core::fmt;

use crate::{Filter, Gate};

#[cfg(doc)]
use crate::{Notifier, Store, StoreLock};

#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
/// A receiver of messages (typically from something implementing [`Listen`]) which can
/// indicate when it is no longer interested in them (typically because the associated
/// recipient has been dropped).
///
/// Listeners are typically used in trait object form, which may be created by calling
/// [`erased_unsync()`](Self::erased_unsync) or [`erased_sync()`](Self::erased_sync);
/// this is done implicitly by [`Notifier`], but calling it earlier may in some cases
/// be useful to minimize the number of separately allocated clones of the listener.
///
/// Please note the requirements set out in [`Listener::receive()`].
///
/// Consider implementing [`Store`] and using [`StoreLock`] instead of implementing [`Listener`].
/// [`StoreLock`] provides the weak reference and mutex that are needed in the most common
/// kind of use of [`Listener`].
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
    /// **You should not need to override this method.**
    fn erased_unsync(self) -> crate::unsync::DynListener<M>
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
    /// **You should not need to override this method.**
    #[cfg(feature = "sync")]
    fn erased_sync(self) -> crate::sync::DynListener<M>
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
    /// use synch::{unsync::Notifier, DirtyFlag, Listen as _, Listener as _};
    ///
    /// let notifier = Notifier::new();
    /// let flag = DirtyFlag::new(false);
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
    /// use synch::{Gate, Listen as _, Listener as _, Sink};
    ///
    /// let sink = Sink::new();
    /// let (gate, gated) = sink.listener().gate();
    /// gated.receive(&["kept1"]);
    /// assert_eq!(sink.drain(), vec!["kept1"]);
    /// gated.receive(&["kept2"]);
    /// drop(gate);
    /// gated.receive(&["discarded"]);
    /// assert_eq!(sink.drain(), vec!["kept2"]);
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
/// This trait is a helper for [`Listen`] and generally cannot be usefully implemented directly.
pub trait IntoDynListener<M, L: Listener<M>>: Listener<M> {
    /// Wrap this [`Listener`] into a type-erased form of type `L`.
    fn into_dyn_listener(self) -> L;
}

#[cfg(feature = "sync")]
impl<L, M> IntoDynListener<M, crate::sync::DynListener<M>> for L
where
    L: Listener<M> + Send + Sync + 'static,
{
    fn into_dyn_listener(self) -> crate::sync::DynListener<M> {
        self.erased_sync()
    }
}

impl<L, M> IntoDynListener<M, crate::unsync::DynListener<M>> for L
where
    L: Listener<M> + 'static,
{
    fn into_dyn_listener(self) -> crate::unsync::DynListener<M> {
        self.erased_unsync()
    }
}

impl<M> Listener<M> for crate::unsync::DynListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        (**self).receive(messages)
    }

    fn erased_unsync(self) -> crate::unsync::DynListener<M> {
        self
    }

    // erased_sync() is unimplementable because its bounds are not met.
}
#[cfg(feature = "sync")]
impl<M> Listener<M> for crate::sync::DynListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        (**self).receive(messages)
    }

    // erased_unsync() will result in double-wrapping.
    // That could be avoided by using `Arc` even for `unsync::DynListener`,
    // but that should be a rare case which is not worth the benefit of avoiding
    // unnecessary atomic operations when feature = "sync" isn't even enabled.

    fn erased_sync(self) -> crate::sync::DynListener<M> {
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
/// use synch::{Listen, IntoDynListener, unsync::Notifier};
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
///     fn listen<L: IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L) {
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
    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L);
}

impl<T: Listen> Listen for &T {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L) {
        (**self).listen(listener)
    }
}
impl<T: Listen> Listen for &mut T {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L) {
        (**self).listen(listener)
    }
}
impl<T: Listen> Listen for alloc::boxed::Box<T> {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L) {
        (**self).listen(listener)
    }
}
impl<T: Listen> Listen for alloc::rc::Rc<T> {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L) {
        (**self).listen(listener)
    }
}
impl<T: Listen> Listen for alloc::sync::Arc<T> {
    type Msg = T::Msg;
    type Listener = T::Listener;

    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L) {
        (**self).listen(listener)
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sink;
    use alloc::rc::Rc;
    use alloc::{format, vec};

    #[test]
    fn erased_listener() {
        let sink = Sink::new();
        let listener: crate::unsync::DynListener<&str> = sink.listener().erased_unsync();

        // Should not gain a new wrapper when erased() again.
        assert_eq!(
            Rc::as_ptr(&listener),
            Rc::as_ptr(&listener.clone().erased_unsync())
        );

        // Should report alive (and not infinitely recurse).
        assert!(listener.receive(&[]));

        // Should deliver messages.
        assert!(listener.receive(&["a"]));
        assert_eq!(sink.drain(), vec!["a"]);

        // Should report dead
        drop(sink);
        assert!(!listener.receive(&[]));
        assert!(!listener.receive(&["b"]));
    }

    /// Demonstrate that [`DynListener`] implements [`fmt::Debug`].
    #[test]
    fn dyn_listener_debug_unsync() {
        let sink: Sink<&str> = Sink::new();
        let listener: crate::unsync::DynListener<&str> = Rc::new(sink.listener());

        assert_eq!(format!("{listener:?}"), "SinkListener { alive: true, .. }");
    }

    /// Demonstrate that [`DynListener`] implements [`fmt::Debug`].
    #[cfg(feature = "sync")]
    #[test]
    fn dyn_listener_debug_sync() {
        let sink: Sink<&str> = Sink::new();
        let listener: crate::sync::DynListener<&str> = alloc::sync::Arc::new(sink.listener());

        assert_eq!(format!("{listener:?}"), "SinkListener { alive: true, .. }");
    }
}
