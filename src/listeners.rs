use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::maybe_sync::{MaRc, MaWeak, RwLock};
use crate::{Listen, Listener};

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

/// Tuples of listeners may be used to distribute messages.
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

/// A [`Listener`] which stores all the messages it receives.
///
/// This is only intended for testing.
#[derive(Debug)]
pub struct Sink<M> {
    messages: MaRc<RwLock<VecDeque<M>>>,
}

/// [`Sink::listener()`] implementation.
pub struct SinkListener<M> {
    weak_messages: MaWeak<RwLock<VecDeque<M>>>,
}

impl<M> Sink<M> {
    /// Constructs a new empty [`Sink`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: MaRc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Returns a [`Listener`] which records the messages it receives in this Sink.
    #[must_use]
    pub fn listener(&self) -> SinkListener<M> {
        SinkListener {
            weak_messages: MaRc::downgrade(&self.messages),
        }
    }

    /// Remove and return all messages returned so far.
    ///
    /// ```
    /// use synch::{Listener, Sink};
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
        self.messages.write().drain(..).collect()
    }
}

impl<M> fmt::Debug for SinkListener<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SinkListener")
            // not useful to print weak_messages unless we were to upgrade and lock it
            .field("alive", &(self.weak_messages.strong_count() > 0))
            .finish_non_exhaustive()
    }
}

impl<M: Clone + Send + Sync> Listener<M> for SinkListener<M> {
    fn receive(&self, messages: &[M]) -> bool {
        if let Some(cell) = self.weak_messages.upgrade() {
            cell.write().extend(messages.iter().cloned());
            true
        } else {
            false
        }
    }
}

impl<M> Clone for SinkListener<M> {
    fn clone(&self) -> Self {
        Self {
            weak_messages: self.weak_messages.clone(),
        }
    }
}

impl<M> Default for Sink<M> {
    // This implementation cannot be derived because we do not want M: Default

    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------------------------------------

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
    /// actually complete its work.
    ///
    /// ```
    /// # let flag = synch::Flag::new(true);
    /// # fn try_to_do_the_thing() -> bool { false }
    /// #
    /// if flag.get_and_clear() {
    ///     if !try_to_do_the_thing() {
    ///         flag.set();
    ///     }
    /// # } else { unreachable!();
    /// }
    /// # assert!(flag.get_and_clear());
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
    use alloc::format;

    #[test]
    fn null_alive() {
        let notifier: Notifier<()> = Notifier::new();
        notifier.listen(NullListener);
        assert_eq!(notifier.count(), 0);
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
