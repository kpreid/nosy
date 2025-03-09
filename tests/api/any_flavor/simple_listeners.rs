use super::flavor::Notifier;
use nosy::{Flag, Listen as _, Listener as _, Log, NullListener};

#[test]
fn null_alive() {
    let notifier: Notifier<()> = Notifier::new();
    notifier.listen(NullListener);
    assert_eq!(notifier.count(), 0);
}

#[test]
fn tuple() {
    let log = Log::new();

    // Tuple of alive listener and dead listener is alive
    assert_eq!((log.listener(), NullListener).receive(&["SN"]), true);
    assert_eq!(log.drain(), vec!["SN"]);

    // Tuple of dead listener and alive listener is alive
    assert_eq!((NullListener, log.listener()).receive(&["NS"]), true);
    assert_eq!(log.drain(), vec!["NS"]);

    // Tuple of alive listener and alive listener is alive
    assert_eq!((log.listener(), log.listener()).receive(&["SS"]), true);
    assert_eq!(log.drain(), vec!["SS", "SS"]);

    // Tuple of dead listener and dead listener is dead
    assert_eq!((NullListener, NullListener).receive(&["NN"]), false);
}

#[test]
fn log_alive() {
    let notifier: Notifier<()> = Notifier::new();
    let log = Log::new();
    notifier.listen(log.listener());
    assert_eq!(notifier.count(), 1);
    drop(log);
    assert_eq!(notifier.count(), 0);
}

#[test]
fn flag_alive() {
    let notifier: Notifier<()> = Notifier::new();
    let flag = Flag::new(false);
    notifier.listen(flag.listener());
    assert_eq!(notifier.count(), 1);
    drop(flag);
    assert_eq!(notifier.count(), 0);
}

#[test]
fn flag_set() {
    let flag = Flag::new(false);

    // not set by zero messages
    flag.listener().receive(&[(); 0]);
    assert!(!flag.get_and_clear());

    // but set by receiving at least one message
    flag.listener().receive(&[()]);
    assert!(flag.get_and_clear());
}

#[test]
fn flag_debug() {
    assert_eq!(format!("{:?}", Flag::new(false)), "Flag(false)");
    assert_eq!(format!("{:?}", Flag::new(true)), "Flag(true)");

    // Test the listener's Debug in all states too
    let flag = Flag::new(false);
    let listener = flag.listener();
    assert_eq!(
        format!("{flag:?} {listener:?}"),
        "Flag(false) FlagListener { alive: true, value: false }"
    );
    listener.receive(&[()]);
    assert_eq!(
        format!("{flag:?} {listener:?}"),
        "Flag(true) FlagListener { alive: true, value: true }"
    );
    drop(flag);
    assert_eq!(format!("{listener:?}"), "FlagListener { alive: false }");
}
