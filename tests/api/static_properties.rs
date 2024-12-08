#[allow(
    dead_code,
    reason = "TODO: why is this counted dead and how do we stop it?"
)]
use std::panic::{RefUnwindSafe, UnwindSafe};

use static_assertions::{assert_impl_all, assert_not_impl_any};

#[allow(
    dead_code,
    reason = "TODO: why is this counted dead and how do we stop it?"
)]
#[cfg(feature = "sync")]
const fn assert_send_sync_if_cfg<T: Send + Sync>() {}
#[allow(
    dead_code,
    reason = "TODO: why is this counted dead and how do we stop it?"
)]
#[cfg(not(feature = "sync"))]
const fn assert_send_sync_if_cfg<T>() {}

const _: () = {
    // All types of interest in the library are listed here, in alphabetical order.
    // TODO: Self-test that this covers all types of interest?

    // Buffer, sync and unsync flavors
    assert_not_impl_any!(synch::Buffer<'static, (), synch::unsync::DynListener<()>, 1>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::Buffer<'static, (), synch::sync::DynListener<()>, 1>: Send, Sync);
        // A Buffer contains messages and is therefore not Send or Sync if the messages aren’t.
        assert_not_impl_any!(
            synch::Buffer<'static, *const (), synch::sync::DynListener<*const ()>, 1>: Send, Sync
        );
    }

    assert_impl_all!(synch::DirtyFlag: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_impl_all!(synch::DirtyFlagListener: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_impl_all!(synch::Filter<fn(()) -> Option<()>, (), 1>: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_impl_all!(synch::Gate: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_impl_all!(synch::GateListener<synch::DirtyFlagListener>: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    // Notifier, sync and unsync flavors.
    // Always not RefUnwindSafe
    assert_impl_all!(synch::unsync::Notifier<()>: Unpin);
    assert_not_impl_any!(synch::unsync::Notifier<()>: Send, Sync, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(synch::unsync::Notifier<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::sync::Notifier<()>: Send, Sync, Unpin);
        // A Notifier with a !Send + !Sync message type is still Send + Sync
        // because it does not *contain* the messages.
        assert_impl_all!(synch::sync::Notifier<*const ()>: Send, Sync);
        assert_not_impl_any!(synch::sync::Notifier<()>: RefUnwindSafe, UnwindSafe);
    }

    assert_impl_all!(synch::unsync::NotifierForwarder<()>: Unpin);
    assert_not_impl_any!(synch::unsync::NotifierForwarder<()>: Send, Sync);
    assert_not_impl_any!(synch::unsync::NotifierForwarder<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::sync::NotifierForwarder<()>: Send, Sync, Unpin);
        assert_impl_all!(synch::sync::NotifierForwarder<*const ()>: Send, Sync);
    }

    assert_impl_all!(synch::NullListener: Send, Sync, Copy, RefUnwindSafe, UnwindSafe);

    assert_send_sync_if_cfg::<synch::Sink<()>>();
    assert_not_impl_any!(synch::Sink<*const ()>: Send, Sync);
    assert_not_impl_any!(synch::Sink<()>: RefUnwindSafe, UnwindSafe);

    assert_send_sync_if_cfg::<synch::SinkListener<()>>();
    assert_not_impl_any!(synch::SinkListener<()>: RefUnwindSafe, UnwindSafe);

    assert_impl_all!(synch::StoreLock<Vec<()>>: Unpin);
    assert_not_impl_any!(synch::StoreLock<Vec<()>>: RefUnwindSafe, UnwindSafe);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::StoreLock<Vec<()>>: Send, Sync);
    }

    // TODO: StoreLock's lock guard

    assert_impl_all!(synch::StoreLockListener<Vec<()>>: Unpin);
    assert_not_impl_any!(synch::StoreLockListener<Vec<()>>: RefUnwindSafe, UnwindSafe);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::StoreLockListener<Vec<()>>: Send, Sync);
    }
};
