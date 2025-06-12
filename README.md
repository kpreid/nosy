`nosy`
======

`nosy` is a Rust library for broadcasting messages/events such as change notifications.

The niche which `nosy` seeks to fill is: delivering precise change notifications
(e.g. “these particular elements of this collection have changed”) from a data source
to a set of listeners (observers) in such a way that

* there is no unbounded buffering of messages (as an unbounded channel would have),
* there is no blocking/suspending (as a bounded channel would have),
* there is no execution of further application logic while the message is being delivered
  (as plain event-listener registration would have), and
* the scheduling of the execution of said application logic is fully under application control
  (rather than implicitly executing some sort of work queue, as a “reactive” framework might).

The tradeoff we make in order to achieve this is that message delivery does involve execution
of a *small* amount of code on behalf of each listener;
this code is responsible for deciding whether the message is of interest, and if so, storing it
or its implications for later reading.
(We could say that *the listeners are nosy*.)

Because of this strategy, `nosy` is not a good choice if you expect to have very many listeners
of the same character (e.g. many identical worker tasks updating their state); in those cases,
you would probably be better off using a conventional broadcast channel or watch channel.
It is also not a good choice if it is critical that no third-party code executes on your thread
or while your function is running.

Platform requirements
---------------------

`nosy` is compatible with `no_std` platforms.
The minimum requirements for using `nosy` are the following.
(All [platforms which support `std`] meet these requirements, and many others do too.)

* The `alloc` standard library crate, and a global allocator.
* Pointer-sized and `u8`-sized atomics (`cfg(target_has_atomic = "ptr")` and `cfg(target_has_atomic = "8")`).

[platforms which support `std`]: https://doc.rust-lang.org/rustc/platform-support.html

License
-------

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Contribution
------------

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.