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

    // Cell, sync and unsync flavors.
    assert_impl_all!(nosy::unsync::Cell<()>: Unpin);
    assert_not_impl_any!(nosy::unsync::Cell<()>: Send, Sync, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(nosy::unsync::Cell<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::sync::Cell<()>: Send, Sync, Unpin);
        assert_not_impl_any!(nosy::sync::Cell<*const ()>: Send, Sync);
        assert_not_impl_any!(nosy::sync::Cell<()>: RefUnwindSafe, UnwindSafe);
    }

    // CellSource, sync and unsync flavors.
    type CellSourceU<T> = nosy::CellSource<T, nosy::unsync::DynListener<()>>;
    type CellSourceS<T> = nosy::CellSource<T, nosy::sync::DynListener<()>>;
    assert_impl_all!(CellSourceU<()>: Unpin);
    assert_not_impl_any!(CellSourceU<()>: Send, Sync, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(CellSourceU<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(CellSourceS<()>: Send, Sync, Unpin);
        assert_not_impl_any!(CellSourceS<*const ()>: Send, Sync);
        assert_not_impl_any!(CellSourceS<()>: RefUnwindSafe, UnwindSafe);
    }

    // CellWithLocal, sync and unsync flavors.
    assert_impl_all!(nosy::unsync::CellWithLocal<()>: Unpin);
    assert_not_impl_any!(nosy::unsync::CellWithLocal<()>: Send, Sync, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(nosy::unsync::CellWithLocal<*const ()>: Send, Sync);
    #[cfg(feature = "sync")]
    {
        assert_impl_all!(nosy::sync::CellWithLocal<()>: Send, Sync, Unpin);
        assert_not_impl_any!(nosy::sync::CellWithLocal<*const ()>: Send, Sync);
        assert_not_impl_any!(nosy::sync::CellWithLocal<()>: RefUnwindSafe, UnwindSafe);
    }

    // Constant, sync and unsync flavors.
    // Constant doesn't store listeners, so it is Send + Sync even if its listeners aren't.
    assert_impl_all!(nosy::unsync::Constant<()>: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(nosy::unsync::Constant<*const ()>: Send, Sync);
    assert_impl_all!(nosy::sync::Constant<()>: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(nosy::sync::Constant<*const ()>: Send, Sync);

    assert_impl_all!(nosy::Filter<fn(()) -> Option<()>, (), 1>: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::Flag: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_impl_all!(nosy::FlagListener: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::Gate: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    assert_impl_all!(nosy::GateListener<nosy::FlagListener>: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);

    assert_send_sync_if_cfg::<nosy::Log<()>>();
    assert_not_impl_any!(nosy::Log<*const ()>: Send, Sync);
    assert_not_impl_any!(nosy::Log<()>: RefUnwindSafe, UnwindSafe);

    assert_impl_all!(nosy::LogListener<()>: Clone);
    assert_send_sync_if_cfg::<nosy::LogListener<()>>();
    assert_not_impl_any!(nosy::LogListener<()>: RefUnwindSafe, UnwindSafe);

    // Notifier, sync and unsync flavors.
    // Always not RefUnwindSafe because listeners are not.
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

    // RawNotifier, sync and unsync flavors.
    // Always not RefUnwindSafe because listeners are not.
    assert_impl_all!(nosy::unsync::RawNotifier<()>: Unpin);
    assert_not_impl_any!(nosy::unsync::RawNotifier<()>: Send, Sync, RefUnwindSafe, UnwindSafe);
    assert_not_impl_any!(nosy::unsync::RawNotifier<*const ()>: Send, Sync);
    assert_impl_all!(nosy::sync::RawNotifier<()>: Send, Sync, Unpin);
    // A RawNotifier with a !Send + !Sync message type is still Send + Sync
    // because it does not *contain* the messages.
    assert_impl_all!(nosy::sync::RawNotifier<*const ()>: Send, Sync);
    assert_not_impl_any!(nosy::sync::RawNotifier<()>: RefUnwindSafe, UnwindSafe);

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

    #[cfg(feature = "async")]
    {
        assert_impl_all!(nosy::future::WakeFlag: Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
        assert_impl_all!(nosy::future::WakeFlagListener: Clone, Send, Sync, Unpin, RefUnwindSafe, UnwindSafe);
    }
};

// Test that deprecations are as expected
#[expect(unused, deprecated)]
use nosy::Sink as SinkIsDeprecated;
#[expect(unused, deprecated)]
use nosy::SinkListener as SinkListenerIsDeprecated;
