#![allow(clippy::bool_assert_comparison, reason = "less legible")]
#![allow(clippy::arc_with_non_send_sync)]

mod listener;
mod static_properties;
mod tools;

// We want to test that `nosy::sync` and `nosy::unsync` are equally usable — that there are no
// differences in declaration that cause one to be able to do something the other cannot,
// other than their fundamental `Send + Sync` difference.
// In order to do that, we write tests that are compiled once for each flavor.
#[cfg(feature = "sync")]
mod sync {
    #![allow(clippy::duplicate_mod)]
    use nosy::sync as flavor;
    include!("any_flavor/mod.rs");
}
mod unsync {
    #![allow(clippy::duplicate_mod)]
    use nosy::unsync as flavor;
    include!("any_flavor/mod.rs");
}
