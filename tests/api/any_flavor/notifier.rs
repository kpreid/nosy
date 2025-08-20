use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

use super::flavor::{Notifier, RawNotifier};
use nosy::sync::DynListener;
use nosy::{FromListener, Listen as _, Listener, Log};

#[test]
fn basics_and_debug() {
    let cn: Notifier<u8> = Notifier::new();
    assert_eq!(format!("{cn:?}"), "Notifier(0)");
    cn.notify(&0);
    assert_eq!(format!("{cn:?}"), "Notifier(0)");
    let log = Log::new();
    cn.listen(log.listener());
    assert_eq!(format!("{cn:?}"), "Notifier(1)");
    // type annotation to prevent spurious inference failures in the presence
    // of other compiler errors
    assert_eq!(log.drain(), Vec::<u8>::new());
    cn.notify(&1);
    cn.notify(&2);
    assert_eq!(log.drain(), vec![1, 2]);
    assert_eq!(format!("{cn:?}"), "Notifier(1)");
}
#[test]
fn basics_and_debug_raw() {
    let mut cn: RawNotifier<u8> = RawNotifier::new();
    assert_eq!(format!("{cn:?}"), "RawNotifier(0)");
    cn.notify(&0);
    assert_eq!(format!("{cn:?}"), "RawNotifier(0)");
    let log = Log::new();
    cn.listen(log.listener());
    assert_eq!(format!("{cn:?}"), "RawNotifier(1)");
    // type annotation to prevent spurious inference failures in the presence
    // of other compiler errors
    assert_eq!(log.drain(), Vec::<u8>::new());
    cn.notify(&1);
    cn.notify(&2);
    assert_eq!(log.drain(), vec![1, 2]);
    assert_eq!(format!("{cn:?}"), "RawNotifier(1)");
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
// RawNotifier has no close(), so nothing to test.

#[test]
#[should_panic = "cannot send messages after Notifier::close()"]
fn notify_after_close() {
    let notifier = Notifier::<()>::new();
    notifier.close();
    notifier.notify(&());
}
// RawNotifier has no close(), so nothing to test.

#[test]
fn drops_listeners_even_with_no_messages_regular() {
    let notifier = Notifier::<()>::new();
    drops_listeners_even_with_no_messages(&mut |l| notifier.listen(l));
}
#[test]
fn drops_listeners_even_with_no_messages_raw() {
    let mut notifier = RawNotifier::<()>::new();
    drops_listeners_even_with_no_messages(&mut |l| notifier.listen(l));
}
fn drops_listeners_even_with_no_messages(listen: &mut dyn FnMut(DynListener<()>)) {
    #[derive(Debug, Default)]
    struct Counters {
        exists: AtomicU32,
        receives: AtomicU32,
    }

    let counters: Arc<Counters> = Arc::default();

    #[derive(Debug)]
    struct MomentaryListener {
        counters: Arc<Counters>,
        first: AtomicBool,
    }
    impl MomentaryListener {
        fn new(counters: Arc<Counters>) -> Self {
            counters.exists.fetch_add(1, Relaxed);
            Self {
                counters,
                first: AtomicBool::new(true),
            }
        }
    }
    impl Listener<()> for MomentaryListener {
        fn receive(&self, _: &[()]) -> bool {
            self.counters.receives.fetch_add(1, Relaxed);
            self.first.swap(false, Relaxed)
        }
    }
    impl Drop for MomentaryListener {
        fn drop(&mut self) {
            self.counters.exists.fetch_sub(1, Relaxed);
        }
    }

    for _ in 0..100 {
        listen(DynListener::from_listener(MomentaryListener::new(
            counters.clone(),
        )));
    }

    // The exact number here will depend on the details of `Notifier` and `Vec`;
    // we are assuming in particular that the `Vec`'s capacity will not be 10 or more.
    // If the count is zero, then that is a sign that this test is broken.
    let count = counters.exists.load(Relaxed);
    assert!(
        (0..10).contains(&count),
        "listeners leaked or dropped early: {count}"
    );

    // Check that the total number of receive() calls is reasonable.
    // If this value is greater than twice the number of listeners, then one of them
    // must have been invoked a third time, which is a bad sign though not strictly prohibited.
    let count = dbg!(counters.receives.load(Relaxed));
    assert!(
        (190..=200).contains(&count),
        "excessive receive()s: {count}"
    );
}

#[test]
fn count_regular() {
    let n: Notifier<u8> = Notifier::new();
    assert_eq!(n.count(), 0);

    let log1 = Log::<u8>::new();
    n.listen(log1.listener());
    assert_eq!(n.count(), 1);

    let log2 = Log::<u8>::new();
    n.listen(log2.listener());
    assert_eq!(n.count(), 2);

    drop(log1);
    assert_eq!(n.count(), 1);

    drop(log2);
    assert_eq!(n.count(), 0);
}

#[test]
fn count_raw() {
    let mut n: RawNotifier<u8> = RawNotifier::new();
    assert_eq!(n.count(), 0);

    let log1 = Log::<u8>::new();
    n.listen(log1.listener());
    assert_eq!(n.count(), 1);

    let log2 = Log::<u8>::new();
    n.listen(log2.listener());
    assert_eq!(n.count(), 2);

    drop(log1);
    assert_eq!(n.count(), 1);

    drop(log2);
    assert_eq!(n.count(), 0);
}
