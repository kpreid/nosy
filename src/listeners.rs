use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::maybe_sync::RwLock;
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
    messages: Arc<RwLock<VecDeque<M>>>,
}

/// [`Sink::listener()`] implementation.
pub struct SinkListener<M> {
    weak_messages: Weak<RwLock<VecDeque<M>>>,
}

impl<M> Sink<M> {
    /// Constructs a new empty [`Sink`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /// Returns a [`Listener`] which records the messages it receives in this Sink.
    #[must_use]
    pub fn listener(&self) -> SinkListener<M> {
        SinkListener {
            weak_messages: Arc::downgrade(&self.messages),
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
        self.messages.write().unwrap().drain(..).collect()
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
            cell.write().unwrap().extend(messages.iter().cloned());
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
pub struct DirtyFlag {
    flag: Arc<AtomicBool>,
}

impl fmt::Debug for DirtyFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // never multiline
        write!(f, "DirtyFlag({:?})", self.flag.load(Ordering::Relaxed))
    }
}

/// [`DirtyFlag::listener()`] implementation.
#[derive(Clone, Debug)]
pub struct DirtyFlagListener {
    weak_flag: Weak<AtomicBool>,
}

impl DirtyFlag {
    /// Constructs a new [`DirtyFlag`] with the given initial value.
    #[must_use]
    pub fn new(value: bool) -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(value)),
        }
    }

    /// Constructs a new [`DirtyFlag`] with the given initial value and call
    /// [`Listen::listen()`] with its listener.
    ///
    /// This is a convenience for calling `new()` followed by `listener()`.
    #[must_use]
    pub fn listening<L>(value: bool, source: L) -> Self
    where
        L: Listen,
        DirtyFlagListener: crate::IntoDynListener<L::Msg, L::Listener>,
    {
        let new_self = Self::new(value);
        source.listen(new_self.listener());
        new_self
    }

    /// Returns a [`Listener`] which will set this flag to [`true`] when it receives any
    /// message.
    #[must_use]
    pub fn listener(&self) -> DirtyFlagListener {
        DirtyFlagListener {
            weak_flag: Arc::downgrade(&self.flag),
        }
    }

    /// Returns the flag value, setting it to [`false`] at the same time.
    #[allow(clippy::must_use_candidate)]
    #[inline]
    pub fn get_and_clear(&self) -> bool {
        self.flag.swap(false, Ordering::Acquire)
    }

    /// Set the flag value to [`true`].
    ///
    /// This is equivalent to `self.listener().receive(())`, but more efficient.
    /// It may be useful in situations where the caller of `get_and_clear()` realizes it cannot
    /// actually complete its work.
    #[inline]
    pub fn set(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
}
impl<M> Listener<M> for DirtyFlagListener {
    fn receive(&self, messages: &[M]) -> bool {
        if let Some(cell) = self.weak_flag.upgrade() {
            if !messages.is_empty() {
                cell.store(true, Ordering::Release);
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
    fn dirty_flag_alive() {
        let notifier: Notifier<()> = Notifier::new();
        let flag = DirtyFlag::new(false);
        notifier.listen(flag.listener());
        assert_eq!(notifier.count(), 1);
        drop(flag);
        assert_eq!(notifier.count(), 0);
    }

    #[test]
    fn dirty_flag_set() {
        let flag = DirtyFlag::new(false);

        // not set by zero messages
        flag.listener().receive(&[(); 0]);
        assert!(!flag.get_and_clear());

        // but set by receiving at least one message
        flag.listener().receive(&[()]);
        assert!(flag.get_and_clear());
    }

    #[test]
    fn dirty_flag_debug() {
        assert_eq!(format!("{:?}", DirtyFlag::new(false)), "DirtyFlag(false)");
        assert_eq!(format!("{:?}", DirtyFlag::new(true)), "DirtyFlag(true)");
        let dirtied = DirtyFlag::new(false);
        dirtied.listener().receive(&[()]);
        assert_eq!(format!("{dirtied:?}"), "DirtyFlag(true)");
    }
}
