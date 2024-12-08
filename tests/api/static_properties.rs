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

    assert_impl_all!(synch::DirtyFlag: Send, Sync);
    assert_impl_all!(synch::DirtyFlagListener: Send, Sync);

    assert_impl_all!(synch::Filter<fn(()) -> Option<()>, (), 1>: Send, Sync);

    assert_impl_all!(synch::Gate: Send, Sync);
    assert_impl_all!(synch::GateListener<synch::DirtyFlagListener>: Send, Sync);

    // Notifier, sync and unsync flavors
    assert_not_impl_any!(synch::unsync::Notifier<()>: Send, Sync);
    assert_not_impl_any!(synch::unsync::Notifier<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::sync::Notifier<()>: Send, Sync);
        // A Notifier with a !Send + !Sync message type is still Send + Sync
        // because it does not *contain* the messages.
        assert_impl_all!(synch::sync::Notifier<*const ()>: Send, Sync);
    }

    assert_not_impl_any!(synch::unsync::NotifierForwarder<()>: Send, Sync);
    assert_not_impl_any!(synch::unsync::NotifierForwarder<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(synch::sync::NotifierForwarder<()>: Send, Sync);
        assert_impl_all!(synch::sync::NotifierForwarder<*const ()>: Send, Sync);
    }

    assert_impl_all!(synch::NullListener: Send, Sync, Copy, RefUnwindSafe, UnwindSafe);

    // TODO: PoisonError

    assert_send_sync_if_cfg::<synch::Sink<()>>();
    assert_not_impl_any!(synch::Sink<*const ()>: Send, Sync);
    assert_send_sync_if_cfg::<synch::SinkListener<()>>();

    // TODO: StoreLock
    // TODO: StoreLock's lock guard
    // TODO: StoreLockListener
};
