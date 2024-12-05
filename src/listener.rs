#![allow(
    clippy::module_name_repetitions,
    reason = "false positive; TODO: remove after Rust 1.84 is released"
)]

use alloc::sync::Arc;
use core::fmt;

#[cfg(doc)]
use crate::StoreLock;

/// A receiver of messages (typically from something implementing [`Listen`]) which can
/// indicate when it is no longer interested in them (typically because the associated
/// recipient has been dropped).
///
/// Listeners are typically used in trait object form, which may be created by calling
/// [`erased()`](Self::erased); this is done implicitly by [`Notifier`], but calling it
/// earlier may in some cases be useful to minimize the number of separately allocated
/// clones of the listener.
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
    /// Messages are provided in a batch for efficiency of dispatch.
    /// Each message in the provided slice should be processed exactly the same as if
    /// it were the only message provided.
    /// If the slice is empty, there should be no observable effect.
    ///
    /// This method should not panic under any circumstances, in order to ensure the sender's
    /// other work is not interfered with.
    /// For example, if the listener accesses a poisoned mutex, it should do nothing or clear
    /// the poison, rather than panicking.
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
    /// then be read and cleared by a later task; see [`FnListener`] for assistance in
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
        Arc::new(self)
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
        Arc::new(self)
    }

    /// Apply a map/filter function (similar to [`Iterator::filter_map()`]) to incoming messages.
    ///
    /// Note: By default, this filter breaks up all message batching into batches of 1.
    /// In order to avoid this and have more efficient message delivery, use
    /// [`Filter::with_stack_buffer()`].
    /// This is unnecessary if `size_of::<M>() == 0`; the buffer is automatically unbounded in
    /// that case.
    ///
    /// TODO: Doc test
    fn filter<MI, F>(self, function: F) -> crate::Filter<F, Self, 1>
    where
        Self: Sized,
        F: for<'a> Fn(&'a MI) -> Option<M>,
    {
        crate::Filter {
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
    /// use synch::{Listen, Listener, Gate, Sink};
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
    fn gate(self) -> (crate::Gate, crate::GateListener<Self>)
    where
        Self: Sized,
    {
        crate::Gate::new(self)
    }
}

// -------------------------------------------------------------------------------------------------
// Type-erasure related traits and impls.

/// Conversion from a concrete listener type to some flavor of boxed trait object.
///
/// This trait is a helper for `Listen` and generally cannot be usefully implemented directly.
pub trait IntoDynListener<M, L: Listener<M>>: Listener<M> {
    fn into_dyn_listener(self) -> L;
}

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
}
impl<M> Listener<M> for crate::sync::DynListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        (**self).receive(messages)
    }

    fn erased_unsync(self) -> crate::unsync::DynListener<M> {
        self
    }

    fn erased_sync(self) -> crate::sync::DynListener<M> {
        self
    }
}

// -------------------------------------------------------------------------------------------------

/// Ability to subscribe to a source of messages, causing a [`Listener`] to receive them
/// as long as it wishes to.
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
    /// nor are subscriptions deduplicated.
    fn listen<L: crate::IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L);
}

impl<T: Listen> Listen for &T {
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
    use crate::unsync::DynListener;
    use crate::Sink;
    use alloc::{format, vec};

    #[test]
    fn erased_listener() {
        let sink = Sink::new();
        let listener: DynListener<&str> = sink.listener().erased_unsync();

        // Should not gain a new wrapper when erased() again.
        assert_eq!(
            Arc::as_ptr(&listener),
            Arc::as_ptr(&listener.clone().erased_unsync())
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
    fn dyn_listener_debug() {
        let sink: Sink<&str> = Sink::new();
        let listener: DynListener<&str> = Arc::new(sink.listener());

        assert_eq!(format!("{listener:?}"), "SinkListener { alive: true, .. }");
    }
}
