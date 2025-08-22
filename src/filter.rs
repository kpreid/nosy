use core::fmt;

use crate::Listener;

/// A [`Listener`] which transforms or discards messages before passing them on to another
/// [`Listener`].
///
/// Construct this by calling [`Listener::filter()`]; see its documentation for more information.
///
/// # Generic parameters
///
/// * `F` is the type of the filter function to use.
/// * `T` is the type of the listener to pass filtered messages to.
/// * `BATCH` is the maximum number of filtered messages to gather before passing them on.
///   It is used as the size of a stack-allocated array, so should be chosen with the size of
///   the message type in mind.
//---
// Example/doc-test is in `filter()`
#[derive(Clone)]
pub struct Filter<F, T, const BATCH: usize> {
    /// The function to transform and possibly discard each message.
    pub(super) function: F,
    /// The recipient of the messages.
    pub(super) target: T,
}

impl<F, T> Filter<F, T, 1> {
    /// Request that the filter accumulate output messages into a batch.
    ///
    /// This causes each [receive](Listener::receive) operation to allocate an array `[MO; BATCH]`
    /// on the stack, where `MO` is the output message type produced by `F`.
    /// Therefore, the buffer size should be chosen keeping the size of `MO` in mind.
    /// Also, the amount of buffer used cannot exceed the size of the input batch,
    /// so it is not useful to choose a buffer size larger than the expected batch size.
    ///
    /// If `MO` is a zero-sized type, then the buffer is always unbounded,
    /// so `with_stack_buffer()` has no effect and is unnecessary in that case.
    pub fn with_stack_buffer<const BATCH: usize>(self) -> Filter<F, T, BATCH> {
        Filter {
            function: self.function,
            target: self.target,
        }
    }
}

impl<F, T: fmt::Debug, const BATCH: usize> fmt::Debug for Filter<F, T, BATCH> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Filter")
            // functions usually don’t implement Debug, but the type name may be the function name
            .field(
                "function",
                &crate::util::Unquote::type_name_of(&self.function),
            )
            .field("target", &self.target)
            .finish()
    }
}

impl<F, T: fmt::Pointer, const BATCH: usize> fmt::Pointer for Filter<F, T, BATCH> {
    /// Produces the same output as the listener this filter wraps.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        T::fmt(&self.target, f)
    }
}

impl<MI, MO, F, T, const BATCH: usize> Listener<MI> for Filter<F, T, BATCH>
where
    F: Fn(&MI) -> Option<MO> + Send + Sync,
    T: Listener<MO>,
{
    fn receive(&self, messages: &[MI]) -> bool {
        if const { size_of::<MO>() == 0 } {
            // If the size of the output message is zero, then we can buffer an arbitrary number
            // of them without occupying any memory or performing any allocation, and therefore
            // preserve the input batching for free.
            let mut filtered_messages = alloc::vec::Vec::<MO>::new();
            for message in messages {
                if let Some(filtered_message) = (self.function)(message) {
                    filtered_messages.push(filtered_message);
                }
            }
            // Deliver entire batch of ZST messages.
            self.target.receive(filtered_messages.as_slice())
        } else {
            let mut buffer: arrayvec::ArrayVec<MO, BATCH> = arrayvec::ArrayVec::new();
            for message in messages {
                if let Some(filtered_message) = (self.function)(message) {
                    // Note that we do this fullness check before, not after, pushing a message,
                    // so if the buffer fills up exactly, we will use the receive() call after
                    // the end of the loop, not this one.
                    if buffer.is_full() {
                        let alive = self.target.receive(&buffer);
                        if !alive {
                            // Target doesn’t want any more messages, so we don’t need to filter
                            // them.
                            return false;
                        }
                        buffer.clear();
                    }
                    buffer.push(filtered_message);
                }
            }
            // Deliver final partial batch, if any, and final liveness check.
            self.target.receive(&buffer)
        }
    }
}

impl<MI, MO, F, T: Listener<MO>, const BATCH: usize> crate::FromListener<Filter<F, T, BATCH>, MI>
    for Filter<F, T, BATCH>
where
    F: Fn(&MI) -> Option<MO> + Send + Sync,
    T: Listener<MO>,
{
    /// No-op conversion returning the listener unchanged.
    fn from_listener(listener: Filter<F, T, BATCH>) -> Self {
        listener
    }
}
