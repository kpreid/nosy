use std::rc;
use std::sync::{self, atomic};

use nosy::{Listener as _, StoreRef};

// -------------------------------------------------------------------------------------------------
// Tests of weak reference `Listener`s based on `StoreRef`.

/// Helper `StoreRef` implementation to test the weak reference `Listener` implementations.
/// We could use the `Mutex`-of-`Store` impls for this, but doing it this way means that the
/// test is independent of other functionality.
#[derive(Debug, Default)]
struct Detector(atomic::AtomicU32);

impl Detector {
    fn load(&self) -> u32 {
        self.0.load(atomic::Ordering::Relaxed)
    }
}

impl nosy::StoreRef<u32> for Detector {
    fn receive(&self, messages: &[u32]) {
        if let Some(&value) = messages.last() {
            self.0.store(value, atomic::Ordering::Relaxed);
        }
    }
}

#[test]
fn rc_weak_receive() {
    let detector = rc::Rc::new(Detector::default());
    let listener = rc::Rc::downgrade(&detector);

    let alive = listener.receive(&[1, 2]);
    assert_eq!(alive, true);
    assert_eq!(detector.load(), 2);

    // check the separate non-upgrading code path for receiving zero messages
    assert_eq!(listener.receive(&[]), true);

    drop(detector);
    let alive = listener.receive(&[3, 4]);
    assert!(!alive);

    assert_eq!(listener.receive(&[]), false);
}

#[test]
fn arc_weak_receive() {
    let detector = sync::Arc::new(Detector::default());
    let listener = sync::Arc::downgrade(&detector);

    let alive = listener.receive(&[1, 2]);
    assert_eq!(alive, true);
    assert_eq!(detector.load(), 2);

    assert_eq!(listener.receive(&[]), true);

    drop(detector);
    let alive = listener.receive(&[3, 4]);
    assert_eq!(alive, false);

    assert_eq!(listener.receive(&[]), false);
}

// -------------------------------------------------------------------------------------------------
// Tests of `StoreRef` implementations for mutexes.
// These tests depend on `impl Store for Vec` being correct.

#[test]
fn refcell() {
    let refcell = core::cell::RefCell::new(Vec::new());

    refcell.receive(&[1]);
    assert_eq!(refcell.borrow().as_slice(), &[1]);

    refcell.receive(&[2]);
    assert_eq!(refcell.borrow().as_slice(), &[1, 2]);
}

#[cfg(feature = "std")]
#[test]
fn mutex() {
    let mutex = sync::Mutex::new(Vec::new());

    mutex.receive(&[1]);
    assert_eq!(mutex.lock().unwrap().as_slice(), &[1]);

    mutex.receive(&[2]);
    assert_eq!(mutex.lock().unwrap().as_slice(), &[1, 2]);

    // poisoning is ignored
    std::panic::catch_unwind(|| {
        let _guard = mutex.lock().unwrap();
        panic!("force poison");
    })
    .unwrap_err();
    mutex.receive(&[3]);
    mutex.clear_poison();
    assert_eq!(mutex.lock().unwrap().as_slice(), &[1, 2, 3]);
}
