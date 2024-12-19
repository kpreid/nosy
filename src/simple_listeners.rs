use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::{Listen, Listener, StoreLock, StoreLockListener};

// -------------------------------------------------------------------------------------------------

/// A [`Listener`] which discards all messages.
///
/// Use this when a [`Listener`] is demanded, but there is nothing it should do.
#[expect(clippy::exhaustive_structs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NullListener;

impl<M> Listener<M> for NullListener {
    fn receive(&self, _messages: &[M]) -> bool {
        false
    }
}

// -------------------------------------------------------------------------------------------------

/// Tuples of listeners may be used to distribute messages to multiple listeners with static
/// dispatch.
impl<M, L1, L2> Listener<M> for (L1, L2)
where
    L1: Listener<M>,
    L2: Listener<M>,
{
    fn receive(&self, messages: &[M]) -> bool {
        // note non-short-circuiting or
        self.0.receive(messages) | self.1.receive(messages)
    }
}

// -------------------------------------------------------------------------------------------------

/// A [`Listener`] destination which stores all the messages it receives.
///
/// This is a slightly more convenient interface for a [`StoreLock<Vec<M>>`](StoreLock).
///
/// This is only intended for testing; real listeners should not unboundedly allocate
/// duplicate messages.
///
/// # Generic parameters
///
/// * `M` is the type of the messages.
#[derive(Debug)]
pub struct Sink<M>(StoreLock<Vec<M>>);

/// [`Sink::listener()`] implementation.
///
/// # Generic parameters
///
/// * `M` is the type of the messages.
pub struct SinkListener<M>(StoreLockListener<Vec<M>>);

impl<M> Sink<M> {
    /// Constructs a new empty [`Sink`].
    #[must_use]
    pub fn new() -> Self {
        Self(StoreLock::default())
    }

    /// Returns a [`Listener`] which records the messages it receives in this Sink.
    #[must_use]
    pub fn listener(&self) -> SinkListener<M> {
        SinkListener(self.0.listener())
    }

    /// Remove and return all messages returned so far.
    ///
    /// ```
    /// use nosy::{Listener, Sink};
    ///
    /// let sink = Sink::new();
    /// sink.listener().receive(&[1]);
    /// sink.listener().receive(&[2]);
    /// assert_eq!(sink.drain(), vec![1, 2]);
    /// sink.listener().receive(&[3]);
    /// assert_eq!(sink.drain(), vec![3]);
    /// ```
    #[must_use]
    pub fn drain(&self) -> Vec<M> {
        self.0.lock().drain(..).collect()
    }
}

impl<M> fmt::Debug for SinkListener<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SinkListener").field(&self.0).finish()
    }
}

impl<M: Clone + Send + Sync> Listener<M> for SinkListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        self.0.receive(messages)
    }
}

impl<M> Clone for SinkListener<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<M> Default for Sink<M> {
    // This implementation cannot be derived because we do not want M: Default
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "async"), allow(rustdoc::broken_intra_doc_links))]
/// A [`Listener`] destination which records only whether any messages have been received,
/// until cleared.
///
/// It is implemented as a shared [`AtomicBool`].
/// It is [`Send`] and [`Sync`] regardless of whether the `"sync"` crate feature is enabled.
///
/// The atomic orderings used are [`Release`](Ordering::Release) for setting the flag, and
/// [`Acquire`](Ordering::Acquire) for reading and clearing it.
/// We do not recommend relying on this as your sole source of synchronization in unsafe code,
/// but this does mean that if the notification is carried across threads then the recipient
/// can rely on seeing effects that happened before the flag was set.
///
/// The name of this type comes from the concept of a “dirty flag”, marking that state is
/// unsaved or out of sync, but it can also be understood as a metaphorical mailbox flag —
/// it signals that something has arrived, but not what.
///
/// # See also
///
/// * [`future::WakeFlag`](crate::future::WakeFlag) is similar but wakes an async task
///   instead of needing to be polled.
pub struct Flag {
    shared: Arc<AtomicBool>,
}

/// [`Flag::listener()`] implementation.
#[derive(Clone)]
pub struct FlagListener {
    weak: Weak<AtomicBool>,
}

impl fmt::Debug for Flag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // never multiline
        write!(f, "Flag({:?})", self.shared.load(Ordering::Relaxed))
    }
}
impl fmt::Debug for FlagListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let strong = self.weak.upgrade();

        let mut ds = f.debug_struct("FlagListener");
        ds.field("alive", &strong.is_some());
        if let Some(strong) = strong {
            ds.field("value", &(strong.load(Ordering::Relaxed)));
        }
        ds.finish()
    }
}

impl Flag {
    const SET_ORDERING: Ordering = Ordering::Release;
    const GET_CLEAR_ORDERING: Ordering = Ordering::Acquire;

    /// Constructs a new [`Flag`] with the given initial value.
    ///
    /// ```
    /// # use nosy::Flag;
    /// assert_eq!(Flag::new(false).get_and_clear(), false);
    /// assert_eq!(Flag::new(true).get_and_clear(), true);
    /// ```
    #[must_use]
    pub fn new(value: bool) -> Self {
        Self {
            shared: Arc::new(AtomicBool::new(value)),
        }
    }

    /// Constructs a new [`Flag`] with the given initial value and call
    /// [`Listen::listen()`] with its listener.
    ///
    /// This is a convenience for calling `new()` followed by `listener()`.
    ///
    /// ```
    /// use nosy::{Flag, unsync::Notifier};
    ///
    /// let notifier = Notifier::<()>::new();
    /// let flag = Flag::listening(false, &notifier);
    ///
    /// notifier.notify(&());
    /// assert_eq!(flag.get_and_clear(), true);
    /// ```
    #[must_use]
    pub fn listening<L>(value: bool, source: L) -> Self
    where
        L: Listen,
        FlagListener: crate::IntoDynListener<L::Msg, L::Listener>,
    {
        let new_self = Self::new(value);
        source.listen(new_self.listener());
        new_self
    }

    /// Returns a [`Listener`] which will set this flag to [`true`] when it receives any
    /// message.
    #[must_use]
    pub fn listener(&self) -> FlagListener {
        FlagListener {
            weak: Arc::downgrade(&self.shared),
        }
    }

    /// Returns the flag value, setting it to [`false`] at the same time.
    #[allow(clippy::must_use_candidate)]
    #[inline]
    pub fn get_and_clear(&self) -> bool {
        self.shared.swap(false, Self::GET_CLEAR_ORDERING)
    }

    /// Set the flag value to [`true`].
    ///
    /// This is equivalent to `self.listener().receive(())`, but more efficient.
    /// It may be useful in situations where the caller of `get_and_clear()` realizes it cannot
    /// actually complete its work, but wants to try again later.
    ///
    /// ```
    /// # let flag = nosy::Flag::new(true);
    /// # fn try_to_do_the_thing() -> bool { false }
    /// #
    /// if flag.get_and_clear() {
    ///     if !try_to_do_the_thing() {
    ///         flag.set();
    ///     }
    /// # } else { unreachable!();
    /// }
    /// assert_eq!(flag.get_and_clear(), true);
    /// ```
    #[inline]
    pub fn set(&self) {
        self.shared.store(true, Self::SET_ORDERING);
    }
}
impl<M> Listener<M> for FlagListener {
    fn receive(&self, messages: &[M]) -> bool {
        if let Some(cell) = self.weak.upgrade() {
            if !messages.is_empty() {
                cell.store(true, Flag::SET_ORDERING);
            }
            true
        } else {
            false
        }
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unsync::Notifier;
    use alloc::{format, vec};

    #[test]
    fn null_alive() {
        let notifier: Notifier<()> = Notifier::new();
        notifier.listen(NullListener);
        assert_eq!(notifier.count(), 0);
    }

    #[test]
    fn tuple() {
        let sink = Sink::new();

        // Tuple of alive listener and dead listener is alive
        assert_eq!((sink.listener(), NullListener).receive(&["SN"]), true);
        assert_eq!(sink.drain(), vec!["SN"]);

        // Tuple of dead listener and alive listener is alive
        assert_eq!((NullListener, sink.listener()).receive(&["NS"]), true);
        assert_eq!(sink.drain(), vec!["NS"]);

        // Tuple of alive listener and alive listener is alive
        assert_eq!((sink.listener(), sink.listener()).receive(&["SS"]), true);
        assert_eq!(sink.drain(), vec!["SS", "SS"]);

        // Tuple of dead listener and dead listener is dead
        assert_eq!((NullListener, NullListener).receive(&["NN"]), false);
    }

    #[test]
    fn sink_alive() {
        let notifier: Notifier<()> = Notifier::new();
        let sink = Sink::new();
        notifier.listen(sink.listener());
        assert_eq!(notifier.count(), 1);
        drop(sink);
        assert_eq!(notifier.count(), 0);
    }

    #[test]
    fn flag_alive() {
        let notifier: Notifier<()> = Notifier::new();
        let flag = Flag::new(false);
        notifier.listen(flag.listener());
        assert_eq!(notifier.count(), 1);
        drop(flag);
        assert_eq!(notifier.count(), 0);
    }

    #[test]
    fn flag_set() {
        let flag = Flag::new(false);

        // not set by zero messages
        flag.listener().receive(&[(); 0]);
        assert!(!flag.get_and_clear());

        // but set by receiving at least one message
        flag.listener().receive(&[()]);
        assert!(flag.get_and_clear());
    }

    #[test]
    fn flag_debug() {
        assert_eq!(format!("{:?}", Flag::new(false)), "Flag(false)");
        assert_eq!(format!("{:?}", Flag::new(true)), "Flag(true)");

        // Test the listener's Debug in all states too
        let flag = Flag::new(false);
        let listener = flag.listener();
        assert_eq!(
            format!("{flag:?} {listener:?}"),
            "Flag(false) FlagListener { alive: true, value: false }"
        );
        listener.receive(&[()]);
        assert_eq!(
            format!("{flag:?} {listener:?}"),
            "Flag(true) FlagListener { alive: true, value: true }"
        );
        drop(flag);
        assert_eq!(format!("{listener:?}"), "FlagListener { alive: false }");
    }
}