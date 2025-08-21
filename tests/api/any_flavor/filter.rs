use super::flavor::Notifier;
use nosy::{Listen as _, Listener as _, Log, NullListener};

use crate::tools::CaptureBatch;

#[test]
fn filter_debug() {
    fn foo(x: &i32) -> Option<i32> {
        Some(x + 1)
    }

    let listener = NullListener.filter(foo);

    let fn_name = std::any::type_name_of_val(&foo);
    assert_eq!(
        format!("{listener:?}"),
        format!("Filter {{ function: {fn_name}, target: NullListener }}")
    );
}

#[test]
fn filter_pointer_fmt_eq() {
    let listener = Log::<()>::new().listener();
    assert_eq!(
        format!("{listener:p}"),
        format!("{:p}", listener.filter(|&x| Some(x))),
        "pointer formatting not equal as expected"
    )
}

#[test]
fn filter_filtering_and_drop() {
    let notifier: Notifier<Option<i32>> = Notifier::new();
    let log = Log::new();
    notifier.listen(log.listener().filter(|&x| x));
    assert_eq!(notifier.count(), 1);

    // Try delivering messages
    notifier.notify(&Some(1));
    notifier.notify(&None);
    assert_eq!(log.drain(), vec![1]);

    // Drop the log and the notifier should observe it gone
    drop(log);
    assert_eq!(notifier.count(), 0);
}

/// Test the behavior when `with_stack_buffer()` is not called,
/// leaving the buffer size implicitly at 1.
#[test]
fn filter_batch_size_1() {
    let notifier: Notifier<i32> = Notifier::new();
    let log: Log<Vec<i32>> = Log::new();
    notifier.listen(CaptureBatch(log.listener()).filter(|&x: &i32| Some(x)));

    // Send some batches
    notifier.notify_many(&[0, 1]);
    notifier.notify_many(&[]);
    notifier.notify_many(&[2, 3]);

    // Expect the batches to be of size at most 1
    assert_eq!(
        log.drain(),
        vec![vec![], vec![0], vec![1], vec![], vec![2], vec![3]]
    );
}

#[test]
fn filter_batching_nzst() {
    let notifier: Notifier<i32> = Notifier::new();
    let log: Log<Vec<i32>> = Log::new();
    notifier.listen(
        CaptureBatch(log.listener())
            .filter(|&x: &i32| Some(x))
            .with_stack_buffer::<2>(),
    );

    // Send some batches
    notifier.notify_many(&[0, 1]);
    notifier.notify_many(&[]);
    notifier.notify_many(&[2, 3, 4]);

    // Expect the batches to be of size at most 2
    assert_eq!(
        log.drain(),
        vec![vec![], vec![0, 1], vec![], vec![2, 3], vec![4]]
    );
}

/// If the message value is a ZST, then batches are unbounded.
#[test]
fn filter_batching_zst() {
    let notifier: Notifier<i32> = Notifier::new();
    let log: Log<Vec<()>> = Log::new();
    notifier.listen(
        CaptureBatch(log.listener()).filter(|&x: &i32| if x == 2 { None } else { Some(()) }),
    );

    // Send some batches
    notifier.notify_many(&[0, 1]);
    notifier.notify_many(&[]);
    notifier.notify_many(&[2, 3, 4, 5]);

    // Expect batches to be preserved and filtered, even though we didn’t set a batch size.
    assert_eq!(
        log.drain(),
        vec![
            vec![],           // initial liveness check on listen()
            vec![(), ()],     // first nonempty batch
            vec![],           // empty batch
            vec![(), (), ()], // second nonempty batch, with 1 item dropped
        ]
    );
}
