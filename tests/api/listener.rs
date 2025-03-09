//! The tests in this module don't quite fit the uniform `any_flavor` model, so they are
//! not in that group.

use std::rc::Rc;
use std::sync::Arc;

use nosy::{sync, unsync, Flag, IntoDynListener as _, Listen, Log, NullListener};

#[test]
fn dyn_listen_is_possible() {
    let notifier = unsync::Notifier::<()>::new();
    let dyn_listen: &dyn Listen<Msg = (), Listener = unsync::DynListener<()>> = &notifier;

    // This is a direct trait object method call.
    dyn_listen.listen_raw(NullListener.into_dyn_listener());

    // This uses `impl Listen for &T` to be able to use the generic `listen()` method.
    (&dyn_listen).listen(NullListener);
}

#[test]
fn dyn_listener_unsync() {
    let log = Log::new();
    let listener: unsync::DynListener<&str> = log.listener().into_dyn_listener();

    // Should not gain a new wrapper when erased() again.
    assert_eq!(
        Rc::as_ptr(&listener),
        Rc::as_ptr(&listener.clone().into_dyn_listener())
    );

    // Should report alive (and not infinitely recurse).
    assert!(listener.receive(&[]));

    // Should deliver messages.
    assert!(listener.receive(&["a"]));
    assert_eq!(log.drain(), vec!["a"]);

    // Should report dead
    drop(log);
    assert!(!listener.receive(&[]));
    assert!(!listener.receive(&["b"]));
}

#[test]
fn dyn_listener_sync() {
    // Flag, unlike Log, is always Send + Sync so is ok to use in this test
    // without making it conditional.
    let flag = Flag::new(false);
    let listener: sync::DynListener<&str> = flag.listener().into_dyn_listener();

    // Should not gain a new wrapper when converted again.
    assert_eq!(
        Arc::as_ptr(&listener),
        Arc::as_ptr(&listener.clone().into_dyn_listener())
    );

    // Should report alive (and not infinitely recurse).
    assert!(listener.receive(&[]));
    assert_eq!(flag.get_and_clear(), false);

    // Should deliver messages.
    assert!(listener.receive(&["a"]));
    assert_eq!(flag.get_and_clear(), true);

    // Should report dead
    drop(flag);
    assert!(!listener.receive(&[]));
    assert!(!listener.receive(&["b"]));
}

/// Demonstrate that [`DynListener`] implements [`fmt::Debug`].
#[test]
fn dyn_listener_debug_unsync() {
    let flag = Flag::new(false);
    let listener: unsync::DynListener<&str> = Rc::new(flag.listener());

    assert_eq!(
        format!("{listener:?}"),
        "FlagListener { alive: true, value: false }"
    );
    drop(flag);
    assert_eq!(format!("{listener:?}"), "FlagListener { alive: false }");
}

/// Demonstrate that [`DynListener`] implements [`fmt::Debug`].
#[test]
fn dyn_listener_debug_sync() {
    let flag = Flag::new(false);
    let listener: sync::DynListener<&str> = Arc::new(flag.listener());

    assert_eq!(
        format!("{listener:?}"),
        "FlagListener { alive: true, value: false }"
    );
    drop(flag);
    assert_eq!(format!("{listener:?}"), "FlagListener { alive: false }");
}
