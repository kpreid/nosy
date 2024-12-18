#![allow(
    clippy::module_name_repetitions,
    reason = "false positive on private module; TODO: remove after Rust 1.84 is released"
)]

use alloc::sync::{Arc, Weak};
use core::fmt;

use crate::Listener;

/// Breaks a [`Listener`] connection when dropped.
///
/// Construct this using [`Listener::gate()`], or if a placeholder instance with no
/// effect is required, [`Gate::default()`]. Then, drop the [`Gate`] when no more messages
/// should be delivered.
///
/// [`Gate`] and [`GateListener`] are [`Send`] and [`Sync`] regardless of whether the `"sync"`
/// crate feature is enabled.
#[derive(Clone, Default)]
pub struct Gate {
    /// By owning this we keep its [`Weak`] peers alive, and thus the [`GateListener`] active.
    _strong: Arc<()>,
}

impl fmt::Debug for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Gate")
    }
}

impl Gate {
    pub(super) fn new<L>(listener: L) -> (Gate, GateListener<L>) {
        let signaller = Arc::new(());
        let weak = Arc::downgrade(&signaller);
        (
            Gate { _strong: signaller },
            GateListener {
                weak,
                target: listener,
            },
        )
    }
}

/// A [`Listener`] which forwards messages to another,
/// until the corresponding [`Gate`] is dropped.
///
/// Construct this using [`Listener::gate()`].
///
/// # Generic parameters
///
/// * `L` is the type of the listener this forwards to.
///
#[derive(Clone, Debug)]
pub struct GateListener<L> {
    weak: Weak<()>,
    target: L,
}
impl<M, L> Listener<M> for GateListener<L>
where
    L: Listener<M>,
{
    fn receive(&self, messages: &[M]) -> bool {
        if self.weak.strong_count() > 0 {
            self.target.receive(messages)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{unsync::Notifier, Listen as _, Sink};
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn gate() {
        let notifier: Notifier<i32> = Notifier::new();
        let sink = Sink::new();
        let (gate, listener) = Gate::new(sink.listener());
        notifier.listen(listener);
        assert_eq!(notifier.count(), 1);

        // Try delivering messages
        notifier.notify(&1);
        assert_eq!(sink.drain(), vec![1]);

        // Drop the gate and messages should stop passing immediately
        // (even though we didn't even trigger notifier cleanup by calling count())
        drop(gate);
        notifier.notify(&2);
        assert_eq!(sink.drain(), Vec::<i32>::new());

        assert_eq!(notifier.count(), 0);
    }
}
