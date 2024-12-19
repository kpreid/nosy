//! Integration with `async` programming.
//!
//! This module is only available if the Cargo feature `"async"` is enabled.

use alloc::sync::{Arc, Weak};
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll};
use futures_core::Stream;

use futures_util::task::AtomicWaker;

use crate::{Listen, Listener};

// -------------------------------------------------------------------------------------------------

/// A [`Listener`] destination which can wake an async task.
///
/// This is similar to an async MPSC channel except that it carries no data, only wakeups.
/// Like a channel, it has a sending side [`WakeFlagListener`], a receiving side [`WakeFlag`],
/// and either side can notice when the other is closed (dropped).
///
/// Its intended use is to allow a looping task to sleep until it needs to take an action.
///
/// `WakeFlag` uses only atomic operations and no locks, and therefore may be used on `no_std`
/// platforms.
///
/// # Example
///
/// In this async code sample, we wake a task which updates `output_cell` based on the contents of
/// `input_cell`:
///
/// ```
/// # async fn yield_now() {
/// #     let mut yielded = false;
/// #     std::future::poll_fn(move |ctx| {
/// #         if yielded {
/// #             std::task::Poll::Ready(())
/// #         } else {
/// #             yielded = true;
/// #             ctx.waker().wake_by_ref();
/// #             std::task::Poll::Pending
/// #         }
/// #     }).await
/// # }
/// # futures::executor::block_on(async {
/// use futures::join;
/// use nosy::{Source as _, unsync::Cell, future::WakeFlag};
///
/// let input_cell: Cell<i32> = Cell::new(0);
/// let input_source = input_cell.as_source();
/// let output_cell: Cell<i32> = Cell::new(0);
/// let output_source = output_cell.as_source();
///
/// let mut flag = WakeFlag::listening(true, &input_source);
///
/// join!(
///     async move {
///         // Woken task.
///         // This async block owns `flag`, `input_source`, and `output_cell`.
///         // Its loop will exit when it is impossible to have any more work to do,
///         // that is, when `input_cell` is dropped.
///
///         while flag.wait().await {
///             // In a real application this might be some substantial computation.
///             output_cell.set_if_unequal(input_source.get() * 10);
///         }
///     },
///     async move {
///         // Writing task.
///         // This async block owns `input_cell`, and drops it when done making
///         // changes.
///         //
///         // (Note: This “yield, then expect the result to have been computed
///         // by now” strategy is not reliable, and therefore not appropriate,
///         // in more complex async programs. It is used here only to prove
///         // that the computation is actually being done in a reasonably short
///         // example.)
///
///         input_cell.set(1);
///         yield_now().await;
///         assert_eq!(output_source.get(), 10);
///
///         input_cell.set(2);
///         yield_now().await;
///         assert_eq!(output_source.get(), 20);
///     },
/// );
/// # })
/// ```
#[derive(Debug)]
pub struct WakeFlag {
    /// Shared state between the [`WakeFlag`] and [`WakeFlagListener`]s.
    shared: Arc<WakeFlagShared>,
}

/// [`WakeFlag`]’s accompanying listener implementation.
#[derive(Clone, Debug)]
pub struct WakeFlagListener {
    shared: Weak<WakeFlagShared>,

    /// This value existing signals to the [`WakeFlag`] that at least one listener exists.
    ///
    /// TODO: When we have more thorough correctness tests, replace this extra allocation with an
    /// atomic counter inside the `WakeFlagShared`.
    _alive: Arc<()>,
}

#[derive(Debug)]
struct WakeFlagFuture<'a> {
    shared: &'a WakeFlagShared,
    done: bool,
}

#[derive(Debug)]
struct WakeFlagShared {
    notified: AtomicBool,

    /// This weak reference breaks when no [`WakeFlagListener`]s exist
    /// and thus the flag can never wake again.
    listeners_alive: Weak<()>,

    /// Woken when a message arrives or a listener is dropped.
    waker: AtomicWaker,
}

impl WakeFlag {
    const SET_ORDERING: Ordering = Ordering::Release;
    const GET_CLEAR_ORDERING: Ordering = Ordering::Acquire;

    /// Constructs a [`WakeFlag`] and paired [`WakeFlagListener`].
    ///
    /// If `wake_immediately` is true, then the waiting task will be woken on the first call
    /// to [`wait()`](Self::wait), even if no message has been received.
    #[must_use]
    pub fn new(wake_immediately: bool) -> (Self, WakeFlagListener) {
        let strong_alive = Arc::new(());
        let listeners_alive = Arc::downgrade(&strong_alive);
        let shared = Arc::new(WakeFlagShared {
            notified: AtomicBool::new(wake_immediately),
            listeners_alive,
            waker: AtomicWaker::new(),
        });
        let listener = WakeFlagListener {
            shared: Arc::downgrade(&shared),
            _alive: strong_alive,
        };
        (Self { shared }, listener)
    }

    /// Constructs a [`WakeFlag`] with the given initial state and call
    /// [`Listen::listen()`] with its listener.
    ///
    /// This is a convenience for calling [`WakeFlag::new()`] followed by
    /// `source.listen(listener)`.
    #[must_use]
    pub fn listening<L>(wake_immediately: bool, source: L) -> Self
    where
        L: Listen,
        WakeFlagListener: crate::IntoDynListener<L::Msg, L::Listener>,
    {
        let (flag, listener) = Self::new(wake_immediately);
        source.listen(listener);
        flag
    }

    /// Suspend the current async task until at least one message is received,
    /// messages have already been received since the last call to `wait()`,
    /// or no more messages will arrive.
    ///
    /// When a message is received, returns [`true`].
    /// When no more messages will be received because all listeners have been dropped,
    /// returns [`false`];
    /// afterward, calls to `wait()` will always immediately return [`false`].
    ///
    /// This function is “cancellation safe”: if the future is dropped before it completes,
    /// there is no effect on the state of the [`WakeFlag`], as if `wait()` had never been
    /// called at all.
    //---
    // Design note: The `&mut self` is solely to enforce non-concurrent usage
    // (because it would lead to lost signal bugs), not because of any actual mutation.
    #[inline]
    #[must_use]
    pub async fn wait(&mut self) -> bool {
        WakeFlagFuture {
            shared: &self.shared,
            done: false,
        }
        .await
    }

    /// Set the flag, causing the next call to [`wait()`](Self::wait) to return immediately.
    ///
    /// This is equivalent to calling `.receive(&[()])` on the listener, but can be done
    /// without access to the listener.
    /// It may be useful in situations where the task wishes to immediately wake again
    /// for some reason, without having separate logic for that.
    #[inline]
    pub fn notify(&self) {
        self.shared.notify_message();
    }
}

impl<M> Listener<M> for WakeFlagListener {
    fn receive(&self, messages: &[M]) -> bool {
        if let Some(shared) = self.shared.upgrade() {
            if !messages.is_empty() {
                shared.notify_message();
            }
            true
        } else {
            false
        }
    }
}

impl Drop for WakeFlagListener {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.waker.wake();
        }
    }
}

impl Future for WakeFlagFuture<'_> {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        assert!(!self.done);
        let poll_outcome = self.shared.poll(cx);
        if poll_outcome.is_ready() {
            self.get_mut().done = true;
        }
        poll_outcome
    }
}

impl WakeFlagShared {
    fn notify_message(&self) {
        self.notified.store(true, WakeFlag::SET_ORDERING);
        self.waker.wake();
    }

    /// Shared logic between [`WakeFlagFuture::poll()`] and [`Stream::poll_next()`].
    fn poll(&self, cx: &mut Context<'_>) -> Poll<bool> {
        if let Some(answer) = self.get_and_clear() {
            return Poll::Ready(answer);
        }
        self.waker.register(cx.waker());
        if let Some(answer) = self.get_and_clear() {
            Poll::Ready(answer)
        } else {
            Poll::Pending
        }
    }

    fn get_and_clear(&self) -> Option<bool> {
        if self.notified.swap(false, WakeFlag::GET_CLEAR_ORDERING) {
            Some(true)
        } else if self.listeners_alive.strong_count() == 0 {
            Some(false)
        } else {
            None
        }
    }
}

// -------------------------------------------------------------------------------------------------

/// As a [`Stream`], [`WakeFlag`] will produce `()` once for each time
/// [`WakeFlag::wait()`] would produce [`true`].
impl Stream for WakeFlag {
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.shared
            .poll(cx)
            .map(|alive| if alive { Some(()) } else { None })
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use core::pin::pin;
    use futures::task::noop_waker_ref;

    /// Basic functionality test using only `poll()` and ignoring wakers.
    #[test]
    fn wake_flag_polling() {
        let ctx = &mut Context::from_waker(noop_waker_ref());
        let (mut flag, listener) = WakeFlag::new(true);

        // First poll succeeds immediately because we initialized with true.
        assert_eq!(pin!(flag.wait()).as_mut().poll(ctx), Poll::Ready(true));

        {
            // Second poll of a new future returns Pending.
            let mut future = pin!(flag.wait());
            assert_eq!(future.as_mut().poll(ctx), Poll::Pending);

            // When a message is received, then polling will return Ready(true).
            listener.receive(&[()]);
            assert_eq!(future.as_mut().poll(ctx), Poll::Ready(true));
        }

        // When the listener is dropped, then polling will return Ready(false).
        drop(listener);
        assert_eq!(pin!(flag.wait()).as_mut().poll(ctx), Poll::Ready(false));
    }
}
