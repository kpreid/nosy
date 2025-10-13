//! Testing provided [`Store`] implementations.

use std::collections::BTreeSet;

use nosy::Store;

// -------------------------------------------------------------------------------------------------

#[test]
fn impl_vec() {
    let mut store: Vec<u32> = Vec::new();
    store.receive(&[1, 2]);
    assert_eq!(store, vec![1, 2]);

    store.receive(&[3]);
    assert_eq!(store, vec![1, 2, 3]);
}

#[test]
fn impl_btreeset() {
    let mut store: BTreeSet<u32> = BTreeSet::new();
    store.receive(&[2, 1, 2]);
    assert_eq!(store, [1, 2].into_iter().collect());

    store.receive(&[3, 1]);
    assert_eq!(store, [1, 2, 3].into_iter().collect());
}

#[cfg(feature = "std")]
#[test]
fn impl_hashset() {
    use std::collections::HashSet;

    let mut store: HashSet<u32> = HashSet::new();
    store.receive(&[2, 1, 2]);
    assert_eq!(store, [1, 2].into_iter().collect());

    store.receive(&[3, 1]);
    assert_eq!(store, [1, 2, 3].into_iter().collect());
}
