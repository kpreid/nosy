use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

use super::flavor::Notifier;
use nosy::{Listen as _, Listener, Sink};

#[test]
fn basics_and_debug() {
    let cn: Notifier<u8> = Notifier::new();
    assert_eq!(format!("{cn:?}"), "Notifier(0)");
    cn.notify(&0);
    assert_eq!(format!("{cn:?}"), "Notifier(0)");
    let sink = Sink::new();
    cn.listen(sink.listener());
    assert_eq!(format!("{cn:?}"), "Notifier(1)");
    // type annotation to prevent spurious inference failures in the presence
    // of other compiler errors
    assert_eq!(sink.drain(), Vec::<u8>::new());
    cn.notify(&1);
    cn.notify(&2);
    assert_eq!(sink.drain(), vec![1, 2]);
    assert_eq!(format!("{cn:?}"), "Notifier(1)");
}

// Test for NotifierForwarder functionality exists as a doc-test.

#[test]
fn forwarder_debug() {
    let n: Arc<Notifier<u8>> = Arc::new(Notifier::new());
    let nf = Notifier::forwarder(Arc::downgrade(&n));
    assert_eq!(
        format!("{nf:?}"),
        "NotifierForwarder { alive(shallow): true, .. }"
    );
    drop(n);
    assert_eq!(
        format!("{nf:?}"),
        "NotifierForwarder { alive(shallow): false, .. }"
    );
}

#[test]
fn close_drops_listeners() {
    #[derive(Debug)]
    struct DropDetector(Arc<()>);
    impl Listener<()> for DropDetector {
        fn receive(&self, _: &[()]) -> bool {
            true
        }
    }

    let notifier = Notifier::<()>::new();
    let detector = DropDetector(Arc::new(()));
    let weak = Arc::downgrade(&detector.0);
    notifier.listen(detector);

    assert_eq!(weak.strong_count(), 1);
    notifier.close();
    assert_eq!(weak.strong_count(), 0);
}

#[test]
#[should_panic = "cannot send messages after Notifier::close()"]
fn notify_after_close() {
    let notifier = Notifier::<()>::new();
    notifier.close();
    notifier.notify(&());
}

#[test]
fn drops_listeners_even_with_no_messages() {
    static COUNT_EXISTS: AtomicU32 = AtomicU32::new(0);
    static COUNT_RECEIVES: AtomicU32 = AtomicU32::new(0);

    #[derive(Debug)]
    struct MomentaryListener {
        first: AtomicBool,
    }
    impl MomentaryListener {
        fn new() -> Self {
            COUNT_EXISTS.fetch_add(1, Relaxed);
            Self {
                first: AtomicBool::new(true),
            }
        }
    }
    impl Listener<()> for MomentaryListener {
        fn receive(&self, _: &[()]) -> bool {
            COUNT_RECEIVES.fetch_add(1, Relaxed);
            self.first.swap(false, Relaxed)
        }
    }
    impl Drop for MomentaryListener {
        fn drop(&mut self) {
            COUNT_EXISTS.fetch_sub(1, Relaxed);
        }
    }

    let notifier = Notifier::<()>::new();
    for _ in 0..100 {
        notifier.listen(MomentaryListener::new());
    }

    // The exact number here will depend on the details of `Notifier` and `Vec`;
    // we are assuming in particular that the `Vec`'s capacity will not be 10 or more.
    // If the count is zero, then that is a sign that this test is broken.
    let count = COUNT_EXISTS.load(Relaxed);
    assert!(
        (0..10).contains(&count),
        "listeners leaked or dropped early: {count}"
    );

    // Check that the total number of receive() calls is reasonable.
    // If this value is greater than twice the number of listeners, then one of them
    // must have been invoked a third time, which is a bad sign though not strictly prohibited.
    let count = dbg!(COUNT_RECEIVES.load(Relaxed));
    assert!(
        (190..=200).contains(&count),
        "excessive receive()s: {count}"
    );
}
