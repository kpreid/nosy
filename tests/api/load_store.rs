use core::sync::atomic;
use nosy::LoadStore;

fn check_load_store_impl<S: LoadStore>(v1: S::Value, v2: S::Value)
where
    S::Value: Clone + PartialEq + core::fmt::Debug,
{
    let container = S::new(v1.clone());

    // get()
    assert_eq!(container.get(), v1, "get 1");
    assert_eq!(container.get(), v1, "get 2");

    // replace()
    let should_be_v1 = container.replace(v2.clone());
    assert_eq!(should_be_v1, v1, "replace result");
    assert_eq!(container.get(), v2, "get after replace");

    // replace_if_unequal() success on replacing v2 with v1
    match container.replace_if_unequal(v1.clone()) {
        Ok(should_be_v2) => assert_eq!(should_be_v2, v2),
        unexpected @ Err(_) => {
            panic!("replace_if_unequal returned unexpected failure {unexpected:?}")
        }
    }
    assert_eq!(container.get(), v1, "get after replace_if_unequal success");

    // replace_if_unequal() failure on replacing v1 with itself
    match container.replace_if_unequal(v1.clone()) {
        Err(should_be_v1) => assert_eq!(should_be_v1, v1),
        unexpected @ Ok(_) => {
            panic!("replace_if_unequal returned unexpected success {unexpected:?}")
        }
    }
    assert_eq!(container.get(), v1, "get after replace_if_unequal failure");
}

macro_rules! test_load_store_impl {
    ($name:ident, $type:ty, [$v1:expr, $v2:expr]) => {
        #[test]
        fn $name() {
            check_load_store_impl::<$type>($v1, $v2);
        }
    };
}

test_load_store_impl!(cell, core::cell::Cell<i32>, [1, 2]);
test_load_store_impl!(refcell, core::cell::RefCell<i32>, [1, 2]);

#[cfg(feature = "std")]
test_load_store_impl!(mutex, std::sync::Mutex<i32>, [1, 2]);
#[cfg(feature = "std")]
test_load_store_impl!(rwlock, std::sync::RwLock<i32>, [1, 2]);

test_load_store_impl!(atomic_bool, atomic::AtomicBool, [false, true]);
test_load_store_impl!(atomic_u8, atomic::AtomicU8, [1, 2]);
test_load_store_impl!(atomic_i8, atomic::AtomicI8, [1, 2]);
test_load_store_impl!(atomic_u16, atomic::AtomicU16, [1, 2]);
test_load_store_impl!(atomic_i16, atomic::AtomicI16, [1, 2]);
test_load_store_impl!(atomic_u32, atomic::AtomicU32, [1, 2]);
test_load_store_impl!(atomic_i32, atomic::AtomicI32, [1, 2]);
test_load_store_impl!(atomic_u64, atomic::AtomicU64, [1, 2]);
test_load_store_impl!(atomic_i64, atomic::AtomicI64, [1, 2]);
test_load_store_impl!(atomic_usize, atomic::AtomicUsize, [1, 2]);
test_load_store_impl!(atomic_isize, atomic::AtomicIsize, [1, 2]);
test_load_store_impl!(
    atomic_ptr,
    atomic::AtomicPtr<u8>,
    [core::ptr::null_mut(), core::ptr::dangling_mut()]
);
