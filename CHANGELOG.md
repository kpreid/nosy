# Changelog

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
