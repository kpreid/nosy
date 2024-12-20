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
