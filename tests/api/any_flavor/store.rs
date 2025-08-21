use core::mem;

use super::flavor;
use nosy::{FromListener, Listener as _, StoreLock};

#[test]
fn store_lock_debug() {
    let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
    assert_eq!(format!("{sl:?}"), "StoreLock([\"initial\"])");
}

#[test]
fn store_lock_listener_debug() {
    let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
    let listener = sl.listener();
    assert_eq!(
        format!("{listener:?}"),
        "StoreLockListener { type: alloc::vec::Vec<&str>, alive: true }"
    );
    drop(sl);
    assert_eq!(
        format!("{listener:?}"),
        "StoreLockListener { type: alloc::vec::Vec<&str>, alive: false }"
    );
}

#[test]
fn pointer_fmt_eq() {
    let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
    let listener = sl.listener();
    assert_eq!(
        format!("{sl:p}"),
        format!("{listener:p}"),
        "pointer formatting not equal as expected"
    )
}

#[test]
fn store_lock_meets_trait_bounds() {
    let _ = flavor::DynListener::<i32>::from_listener(StoreLock::new(vec![]).listener());
}

#[test]
fn store_lock_basics() {
    let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
    let listener = sl.listener();

    // Receive one message and see it added to the initial state
    assert_eq!(listener.receive(&["foo"]), true);
    assert_eq!(mem::take(&mut *sl.lock()), vec!["initial", "foo"]);

    // Receive multiple messages in multiple batches
    assert_eq!(listener.receive(&["bar", "baz"]), true);
    assert_eq!(listener.receive(&["separate"]), true);
    assert_eq!(mem::take(&mut *sl.lock()), vec!["bar", "baz", "separate"]);

    // Receive after drop
    drop(sl);
    assert_eq!(listener.receive(&["too late"]), false);
}

#[test]
fn store_lock_receive_inherent() {
    let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
    sl.receive(&["from inherent receive"]);

    assert_eq!(
        mem::take(&mut *sl.lock()),
        vec!["initial", "from inherent receive"]
    );
}

/// For consistency with `no_std` mode, we *do not* offer mutex poisoning,
/// even if the underlying implementation does.
/// In exchange, the guard is not `UnwindSafe` either.
#[test]
fn store_lock_not_poisoned() {
    let sl: StoreLock<Vec<&'static str>> = StoreLock::new(vec!["initial"]);
    let listener = sl.listener();

    // Try to poison the mutex by panicking inside it
    let _ = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
        let _guard = sl.lock();
        panic!("poison");
    }));

    // Listener still works.
    assert_eq!(listener.receive(&["foo"]), true);

    // Inherent receive still works.
    sl.receive(&["bar"]);

    // Locking still works.
    assert_eq!(mem::take(&mut *sl.lock()), vec!["initial", "foo", "bar"]);
}
