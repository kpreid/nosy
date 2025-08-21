use core::future::Future as _;
use core::pin::pin;
use core::task::{Context, Poll};

use futures::task::noop_waker_ref; // TODO: we can replace this with Waker::noop() when MSRV is Rust 1.85

use nosy::Listener as _;

// -------------------------------------------------------------------------------------------------

/// Basic functionality test using only `poll()` and ignoring wakers.
#[test]
fn wake_flag_polling() {
    let ctx = &mut Context::from_waker(noop_waker_ref());
    let (mut flag, listener) = nosy::future::WakeFlag::new(true);

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
