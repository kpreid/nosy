#![cfg_attr(not(feature = "std"), allow(unused_imports))]

use core::mem;
use std::sync::{Arc, Mutex};

use super::flavor;
use nosy::{FromListener, Listener as _};

// -------------------------------------------------------------------------------------------------

// TODO: it is suspicious that we are cfg-ing out these things.
// Consider improvements to features or test organization.

#[cfg(feature = "std")]
#[test]
fn arc_mutex_meets_trait_bounds() {
    let _ = flavor::DynListener::<i32>::from_listener(Arc::downgrade(&Arc::new(Mutex::new(
        Vec::<i32>::new(),
    ))));
}

/// This test is identical except for the types to a test of `StoreLock`.
#[cfg(feature = "std")]
#[test]
fn arc_mutex_store_basics() {
    let sl: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(vec!["initial"]));
    let listener = Arc::downgrade(&sl);

    // Receive one message and see it added to the initial state
    assert_eq!(listener.receive(&["foo"]), true);
    assert_eq!(mem::take(&mut *sl.lock().unwrap()), vec!["initial", "foo"]);

    // Receive multiple messages in multiple batches
    assert_eq!(listener.receive(&["bar", "baz"]), true);
    assert_eq!(listener.receive(&["separate"]), true);
    assert_eq!(
        mem::take(&mut *sl.lock().unwrap()),
        vec!["bar", "baz", "separate"]
    );

    // Receive after drop
    drop(sl);
    assert_eq!(listener.receive(&["too late"]), false);
}
