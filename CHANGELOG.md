# Changelog

## 0.2.0 (Unreleased)

### Added

- New trait implementations allow creating a `Notifier` for a single listener type only, like `nosy::Listener<nosy::Flag, ()>`. Such notifiers can be more efficient by avoiding dynamic dispatch and indirection.
- All provided listener and destination types now implement `fmt::Pointer`.
  This can be used like `eprintln!("{listener:p}")` to identify which shared state a listener is updating.

### Changed

- The minimum supported Rust version is now 1.85.
- The trait `IntoDynListener` has been split into two traits, `FromListener`, and `IntoListener`.
  `FromListener` newly allows blanket implementations for converting to types not defined in `nosy` itself, i.e. `impl<L, M> FromListener<L, M> for MyPointer<dyn MyListener>`.
  `IntoListener` is a sealed trait implemented for all `FromListener`, which is more convenient to use when accepting a listener than `FromListener` is.

## 0.1.1 (2025-06-12)

### Added

- `RawNotifier`, for when `Notifier`’s interior mutability is undesirable.
- `Source::map()` for transforming the value.
- `Source::flatten()` for using sources of sources.
- `StoreLock::take()`, a convenience for using the lock efficiently.
- `impl Default for Cell`
- `impl Default for CellWithLocal`
- `impl Listen for Cell`, a convenience for bypassing `.as_source()`.
- `impl Listen for CellWithLocal`

### Changed

- Renamed `Sink` to `Log`, to clarify that it is not the opposite of `Source`.
  The old name is available but deprecated.
- `Notifier` no longer checks every listener’s aliveness on adding any new listener, but only when the addition would require reallocation.
  This makes `Notifier::listen()` amortized O(<var>N</var>) instead of O(<var>M</var>·<var>N</var>) in the presence of <var>M</var> other listeners, but may result in some listeners being dropped later than previously.

## 0.1.0 (2024-12-19)

Initial public release.
