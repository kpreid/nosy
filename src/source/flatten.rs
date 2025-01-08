use alloc::sync::Arc;
use alloc::sync::Weak;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;

use crate::GateListener;
use crate::NotifierForwarder;
use crate::{maybe_sync, Gate, IntoDynListener, Listen, Listener, Notifier, Source};

// -------------------------------------------------------------------------------------------------

/// A [`Source`] that takes a [`Source`] whose value is another [`Source`] and produces
/// the value of the latter source.
///
/// Construct this by calling [`Source::flatten()`]; see the further documentation there.
#[derive(Debug)]
pub struct Flatten<S: Source<Value: Source>>(Arc<Inner<S>>);

#[derive(Debug)]
struct Inner<S: Source<Value: Source>> {
    /// Source from which we get the source from which we get the value.
    source_of_source: S,

    /// Source we got from `source_of_source`, and its generation number.
    ///
    /// This is updated upon the next `get()` or `listen()` invocation after `source_of_source`
    /// notifies us. It is `None` on first construction. It is never locked except during `get()`,
    /// but`S::Value` is necessarily cloned while it is held.
    state: maybe_sync::Mutex<Option<State<S::Value>>>,

    /// Incremented when `source_of_source` tells us the source changed.
    ///
    /// Comparing this to `source`’s generation number tells you whether `source` is stale.
    generation_from_listening: AtomicU32,

    /// Notifier for our own listeners.
    ///
    /// Design note: It would be nice if we did not need this, and could simply have our clients’
    /// listeners listen to the two sources. Unfortunately, that would not allow us to attach our
    /// clients’ listeners to the inner source when a new inner source appears.
    /// So, we must instead forward explicitly.
    ///
    /// TODO: Make `NotifierForwarder` more generic and avoid needing this nested `Arc`?
    notifier: Arc<Notifier<(), S::Listener>>,
}

#[derive(Debug)]
struct State<S> {
    generation: u32,

    /// This is the “inner” source — the source that comes from `source_of_source`.
    source: S,

    #[allow(dead_code, reason = "used for its Drop behavior")]
    /// This gate is how we cancel listening to an old source.
    gate: Gate,
}

// -------------------------------------------------------------------------------------------------

impl<S> Flatten<S>
where
    S: Source<Value: Source + Clone>,
    OuterListener<S>: IntoDynListener<(), S::Listener>,
    InnerListener<S>: IntoDynListener<(), <S::Value as Listen>::Listener>,
{
    /// Constructs a source whose value is `source_of_source.get().get()`.
    pub fn new(source_of_source: S) -> Self {
        let new_self = Self(Arc::new(Inner {
            source_of_source,
            generation_from_listening: AtomicU32::new(0),
            state: maybe_sync::Mutex::new(None),
            notifier: Arc::new(Notifier::new()),
        }));

        new_self.0.source_of_source.listen(OuterListener {
            shared: Arc::downgrade(&new_self.0),
        });

        new_self
    }

    fn get_source_and_listen_if_new(&self) -> S::Value {
        // First, determine how new a source we need to have.
        // (It’s okay if this is incremented after we check it.)
        let needed_generation = self.0.generation_from_listening.load(Ordering::Relaxed);

        match *self.0.state.lock() {
            Some(State {
                generation,
                ref source,
                gate: _,
            }) if generation == needed_generation => return source.clone(),
            _ => {}
        }

        // The generation did not match; `source_of_source` has a new source for us.
        // Therefore, *without the lock held*, prepare an updated `State` with the new source.
        let source = self.0.source_of_source.get();

        // Start listening to the new source.
        let (gate, gated_forwarder) = Notifier::forwarder(Arc::downgrade(&self.0.notifier)).gate();
        source.listen(InnerListener(gated_forwarder));
        let new_state = State {
            generation: 0,
            source,
            gate,
        };

        // Now, take the lock and store this new state, if nobody else got to it meanwhile.
        match *self.0.state.lock() {
            // Someone else got to it first. Use theirs.
            Some(State {
                generation,
                ref source,
                gate: _,
            }) if generation >= needed_generation => source.clone(),

            // Generation is old, so replace it.
            ref mut state_guard => {
                let source = new_state.source.clone();
                *state_guard = Some(new_state);
                source
            }
        }
    }
}

impl<S> Listen for Flatten<S>
where
    S: Source<Value: Source + Clone>,
{
    type Msg = ();
    type Listener = S::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        self.0.notifier.listen_raw(listener)
    }

    fn listen<L: IntoDynListener<Self::Msg, Self::Listener>>(&self, listener: L)
    where
        Self: Sized,
    {
        self.0.notifier.listen(listener)
    }
}

impl<S> Source for Flatten<S>
where
    S: Source<Value: Source + Clone>,
    OuterListener<S>: IntoDynListener<(), S::Listener>,
    InnerListener<S>: IntoDynListener<(), <S::Value as Listen>::Listener>,
{
    type Value = <S::Value as Source>::Value;

    fn get(&self) -> Self::Value {
        self.get_source_and_listen_if_new().get()
    }
}

// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
#[allow(unnameable_types)] // TODO: really?
pub struct OuterListener<S: Source<Value: Source>> {
    shared: Weak<Inner<S>>,
}

impl<S: Source<Value: Source>> Listener<()> for OuterListener<S> {
    fn receive(&self, messages: &[()]) -> bool {
        let Some(shared) = self.shared.upgrade() else {
            return false;
        };

        if !messages.is_empty() {
            shared
                .generation_from_listening
                .fetch_add(1, Ordering::Relaxed);

            shared.notifier.notify(&());
        }

        true
    }
}

#[derive(Clone, Debug)]
#[allow(unnameable_types)] // TODO: really?
pub struct InnerListener<S: Source<Value: Source>>(
    GateListener<NotifierForwarder<(), S::Listener>>,
);

impl<S: Source<Value: Source>> Listener<()> for InnerListener<S> {
    fn receive(&self, messages: &[()]) -> bool {
        self.0.receive(messages)
    }
}
