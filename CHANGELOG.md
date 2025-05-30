# Changelog

## Unreleased

### Added

- `Source::map()`
- `Source::flatten()`
- `impl Default for Cell`
- `impl Default for CellWithLocal`
- `impl Listen for Cell`
- `impl Listen for CellWithLocal`
- `RawListener`, for when `Notifier`’s interior mutability is undesirable.

### Changed

- Renamed `Sink` to `Log`, to clarify that it is not the opposite of `Source`.

## 0.1.0 (2024-12-19)

Initial public release.
