use std::sync::Arc;

use nosy::{Listen as _, Listener, Sink};
use super::flavor::Notifier;

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
