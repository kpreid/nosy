/// A data structure which records received messages for later processing.
///
/// Its role is similar to [`Listener`], except that it is permitted and expected to `&mut` mutate
/// itself, but **should not** communicate outside itself.
/// A [`Listener`] is a sort of channel by which to transmit messages;
/// a `Store` is a data structure which is a destination for messages.
///
/// After implementing `Store`, wrap it in a [`StoreLock`] to make use of it.
///
/// Generally, a `Store` implementation will combine and de-duplicate messages in some
/// fashion. For example, if the incoming messages were notifications of modified regions of data
/// as rectangles, then one might define a `Store` owning an `Option<Rect>` that contains the union
/// of all the rectangle messages, as an adequate constant-space approximation of the whole.
///
/// # Generic parameters
///
/// * `M` is the type of message that can be received.
///
/// # Example
///
/// ```rust
/// use nosy::{Listen as _, Listener as _, StoreLock};
/// use nosy::unsync::{Cell, Notifier};
///
/// /// Message type delivered from other sources.
/// /// (This enum might be aggregated from multiple `Source`s’ change notifications.)
/// #[derive(Debug)]
/// enum WindowChange {
///     Resized,
///     Contents,
/// }
///
/// /// Tracks what we need to do in response to the messages.
/// #[derive(Debug, Default, PartialEq)]
/// struct Todo {
///     resize: bool,
///     redraw: bool,
/// }
///
/// impl nosy::Store<WindowChange> for Todo {
///     fn receive(&mut self, messages: &[WindowChange]) {
///         for message in messages {
///             match message {
///                 WindowChange::Resized => {
///                     self.resize = true;
///                     self.redraw = true;
///                 }
///                 WindowChange::Contents => {
///                     self.redraw = true;
///                 }
///             }
///         }
///     }
/// }
///
/// // These would actually come from external data sources.
/// let window_size: Cell<[u32; 2]> = Cell::new([100, 100]);
/// let contents_notifier: Notifier<()> = Notifier::new();
///
/// // Create the store and attach its listener to the data sources.
/// let todo_store: StoreLock<Todo> = StoreLock::default();
/// window_size.listen(todo_store.listener().filter(|()| Some(WindowChange::Resized)));
/// contents_notifier.listen(todo_store.listener().filter(|()| Some(WindowChange::Contents)));
///
/// // Make a change and see it reflected in the store.
/// window_size.set([200, 120]);
/// assert_eq!(todo_store.take(), Todo { resize: true, redraw: true });
/// ```
pub trait Store<M> {
    /// Record the given series of messages.
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
    ///
    /// * Do not perform any blocking operation, such as acquiring locks,
    ///   for performance and to avoid deadlock with locks held by the sender.
    ///   (Normally, locking is to be provided separately, e.g. by [`StoreLock`].)
    ///
    /// * Do not access thread-local state, since this may be called from whichever thread(s)
    ///   the sender is using.
    ///
    /// # Advice for implementors
    ///
    /// Implementations should take care to be efficient, both in time taken and other
    /// costs such as working set size. This method is typically called with a mutex held and the
    /// original message sender blocking on it, so inefficiency here may have an effect on
    /// distant parts of the application.
    fn receive(&mut self, messages: &[M]);
}

// -------------------------------------------------------------------------------------------------

/// This is a poor implementation of [`Store`] because it allocates unboundedly.
/// It should be used only for tests of message processing.
impl<M: Clone + Send> Store<M> for alloc::vec::Vec<M> {
    fn receive(&mut self, messages: &[M]) {
        self.extend_from_slice(messages);
    }
}

impl<M: Clone + Send + Ord> Store<M> for alloc::collections::BTreeSet<M> {
    fn receive(&mut self, messages: &[M]) {
        self.extend(messages.iter().cloned());
    }
}
#[cfg(feature = "std")]
impl<M: Clone + Send + Eq + core::hash::Hash, S: core::hash::BuildHasher> Store<M>
    for std::collections::HashSet<M, S>
{
    fn receive(&mut self, messages: &[M]) {
        self.extend(messages.iter().cloned());
    }
}
