use core::future::Future as _;
use core::pin::pin;
use core::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::task::{self, Poll};

use futures::task::SpawnExt as _;
use futures::StreamExt;

use nosy::Listener as _;

use crate::tools::{run_task_without_waiting, yield_now};

// -------------------------------------------------------------------------------------------------

struct WakeDetector {
    woken: AtomicBool,
}
impl std::task::Wake for WakeDetector {
    fn wake(self: Arc<Self>) {
        self.woken.store(true, Relaxed);
    }
}
impl WakeDetector {
    fn take(&self) -> bool {
        self.woken.swap(false, Relaxed)
    }
}

/// Basic functionality test using explicit `poll()` and checking the waker behavior too.
#[test]
fn wake_flag_polling() {
    let wake_detector = Arc::new(WakeDetector {
        woken: AtomicBool::new(false),
    });
    let waker = task::Waker::from(wake_detector.clone());
    let ctx = &mut task::Context::from_waker(&waker);
    let (mut flag, listener) = nosy::future::WakeFlag::new(true);

    // First poll succeeds immediately because we initialized with true.
    assert_eq!(pin!(flag.wait()).as_mut().poll(ctx), Poll::Ready(true));
    assert_eq!(wake_detector.take(), false);

    {
        // Second poll of a new future returns Pending.
        let mut future = pin!(flag.wait());
        assert_eq!(future.as_mut().poll(ctx), Poll::Pending);
        assert_eq!(wake_detector.take(), false);

        // When a message is received, then the waker will be called
        // and polling will return Ready(true).
        listener.receive(&[()]);
        assert_eq!(wake_detector.take(), true);
        assert_eq!(future.as_mut().poll(ctx), Poll::Ready(true));
        assert_eq!(wake_detector.take(), false);
    }

    // When the listener is dropped, then polling will return Ready(false),
    // and the waker (*if* one is registered again) will be woken;
    {
        let mut future = pin!(flag.wait());
        // Get the waker registered again
        assert_eq!(future.as_mut().poll(ctx), Poll::Pending);
        assert_eq!(wake_detector.take(), false);

        drop::<nosy::future::WakeFlagListener>(listener);

        assert_eq!(wake_detector.take(), true);
        assert_eq!(future.as_mut().poll(ctx), Poll::Ready(false));
        assert_eq!(wake_detector.take(), false);
    }
}

/// Test `impl WakeFlag for Stream`.
#[test]
fn wake_flag_stream() {
    // just using this as interior mutable Vec, not something under test
    let event_log = nosy::Log::new();
    let log_listener = event_log.listener();

    run_task_without_waiting(async move {
        let (mut flag, listener) = nosy::future::WakeFlag::new(false);

        futures::join!(
            async {
                while let Some(()) = flag.next().await {
                    log_listener.receive(&["received"]);
                }
            },
            async {
                for _ in 0..2 {
                    log_listener.receive(&["sending"]);
                    listener.receive(&[()]);
                    log_listener.receive(&["sent"]);
                    yield_now().await;
                }
                drop(listener);
            }
        );
    });

    // Check the expected events occurred
    assert_eq!(
        event_log.drain(),
        vec!["sending", "sent", "received", "sending", "sent", "received"]
    );
}
