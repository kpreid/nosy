use std::future::Future;

use futures::task::LocalSpawnExt as _;

// -------------------------------------------------------------------------------------------------

/// Breaks the listener rules for testing by recording batch boundaries.
#[derive(Debug)]
pub(crate) struct CaptureBatch<L>(pub L);

impl<M: Clone, L> nosy::Listener<M> for CaptureBatch<L>
where
    L: nosy::Listener<Vec<M>>,
{
    fn receive(&self, messages: &[M]) -> bool {
        self.0.receive(&[Vec::from(messages)])
    }
}

#[cfg_attr(not(feature = "async"), allow(dead_code))]
pub(crate) async fn yield_now() {
    let mut yielded = false;
    core::future::poll_fn(move |ctx| {
        if yielded {
            core::task::Poll::Ready(())
        } else {
            yielded = true;
            ctx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    })
    .await
}

/// Run a future to completion, but don't block if it doesn't wake, under the assumption that
/// that indicates a lost-signal bug.
///
/// TODO: allow the future to borrow things and return a value
#[cfg_attr(not(feature = "async"), allow(dead_code))]
pub(crate) fn run_task_without_waiting(future: impl Future<Output = ()> + 'static) {
    let mut executor = futures::executor::LocalPool::new();
    executor.spawner().spawn_local(future).unwrap();
    assert!(executor.try_run_one(), "Test stalled");
}
