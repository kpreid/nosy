use super::flavor::Notifier;
use nosy::{Listen as _, Listener as _, Log};

#[test]
fn gate() {
    let notifier: Notifier<i32> = Notifier::new();
    let log = Log::new();
    let (gate, listener): (nosy::Gate, nosy::GateListener<nosy::LogListener<i32>>) =
        log.listener().gate();
    notifier.listen(listener);
    assert_eq!(notifier.count(), 1);

    // Try delivering messages
    notifier.notify(&1);
    assert_eq!(log.drain(), vec![1]);

    // Drop the gate and messages should stop passing immediately
    // (even though we didn't even trigger notifier cleanup by calling count())
    drop(gate);
    notifier.notify(&2);
    assert_eq!(log.drain(), Vec::<i32>::new());

    assert_eq!(notifier.count(), 0);
}
