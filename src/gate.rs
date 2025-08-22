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
    strong: Arc<()>,
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
            Gate { strong: signaller },
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

impl<L: Listener<M>, M> crate::FromListener<GateListener<L>, M> for GateListener<L> {
    /// No-op conversion returning the listener unchanged.
    fn from_listener(listener: GateListener<L>) -> Self {
        listener
    }
}

impl fmt::Pointer for Gate {
    /// Produces an address which identifies this [`Gate`] and its associated [`GateListener`]s.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.strong.fmt(f)
    }
}

impl<L: fmt::Pointer> fmt::Pointer for GateListener<L> {
    /// Produces the addresses identifying the gate and the underlying listener.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { weak, target } = self;
        write!(
            f,
            "GateListener {{ gate: {weak:p}, target: {target:p} }}",
            weak = weak.as_ptr(),
            target = *target, // deref the &L so we call its own fmt::Pointer and not `&`'s
        )
    }
}
