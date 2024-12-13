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
    assert_not_impl_any!(nosy::Buffer<'static, (), nosy::unsync::DynListener<()>, 1>: Clone, Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::Buffer<'static, (), nosy::sync::DynListener<()>, 1>: Send, Sync);
        // A Buffer contains messages and is therefore not Send or Sync if the messages aren’t.
        assert_not_impl_any!(
            nosy::Buffer<'static, *const (), nosy::sync::DynListener<*const ()>, 1>: Clone, Send, Sync
        );
    }

    assert_impl_all!(nosy::Filter<fn(()) -> Option<()>, (), 1>: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::Flag: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_impl_all!(nosy::FlagListener: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::Gate: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_impl_all!(nosy::GateListener<nosy::FlagListener>: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    // Notifier, sync and unsync flavors.
    // Always not RefUnwindSafe
    assert_impl_all!(nosy::unsync::Notifier<()>: Unpin);
    assert_not_impl_any!(nosy::unsync::Notifier<()>: Send, Sync, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(nosy::unsync::Notifier<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::sync::Notifier<()>: Send, Sync, Unpin);
        // A Notifier with a !Send + !Sync message type is still Send + Sync
        // because it does not *contain* the messages.
        assert_impl_all!(nosy::sync::Notifier<*const ()>: Send, Sync);
        assert_not_impl_any!(nosy::sync::Notifier<()>: RefUnwindSafe, UnwindSafe);
    }

    assert_impl_all!(nosy::unsync::NotifierForwarder<()>: Clone, Unpin);
    assert_not_impl_any!(nosy::unsync::NotifierForwarder<()>: Send, Sync);
    assert_not_impl_any!(nosy::unsync::NotifierForwarder<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::sync::NotifierForwarder<()>: Clone, Send, Sync, Unpin);
        assert_impl_all!(nosy::sync::NotifierForwarder<*const ()>: Clone, Send, Sync);
    }

    assert_impl_all!(nosy::NullListener: Copy, Send, Sync, RefUnwindSafe, UnwindSafe);

    assert_send_sync_if_cfg::<nosy::Sink<()>>();
    assert_not_impl_any!(nosy::Sink<*const ()>: Send, Sync);
    assert_not_impl_any!(nosy::Sink<()>: RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::SinkListener<()>: Clone);
    assert_send_sync_if_cfg::<nosy::SinkListener<()>>();
    assert_not_impl_any!(nosy::SinkListener<()>: RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::StoreLock<Vec<()>>: Unpin);
    assert_not_impl_any!(nosy::StoreLock<Vec<()>>: RefUnwindSafe, UnwindSafe);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::StoreLock<Vec<()>>: Send, Sync);
    }

    // TODO: StoreLock's lock guard

    assert_impl_all!(nosy::StoreLockListener<Vec<()>>: Clone);
    assert_impl_all!(nosy::StoreLockListener<Vec<()>>: Unpin);
    assert_not_impl_any!(nosy::StoreLockListener<Vec<()>>: RefUnwindSafe, UnwindSafe);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::StoreLockListener<Vec<()>>: Send, Sync);
    }
};
