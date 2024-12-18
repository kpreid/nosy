#![allow(
    clippy::module_name_repetitions,
    reason = "false positive on private module; TODO: remove after Rust 1.84 is released"
)]

use alloc::sync::Arc;
use core::{fmt, mem};

use crate::maybe_sync::{Mutex, MutexGuard};
use crate::{Listen, Listener, Notifier, Source};

#[cfg(doc)]
use crate::{sync, unsync};

// -------------------------------------------------------------------------------------------------

/// An interior-mutable container for a value which notifies when the value changed.
///
/// The implementation uses a mutex to manage access to the value.
/// The mutex will only be locked as long as necessary to clone or compare the value,
/// so deadlock is impossible unless `T as Clone` or `T as PartialEq` uses a shared lock itself.
///
/// Access to the value requires cloning it, so if the clone is not cheap,
/// consider wrapping the value with [`Arc`] to reduce the cost to reference count changes.
///
/// # Generic parameters
///
/// * `T` is the type of the value.
/// * `L` is the type of [`Listener`] this cell accepts,
///   usually a trait object type such as [`sync::DynListener`].
///
/// We recommend that you use the type aliases [`sync::Cell`] or [`unsync::Cell`],
/// to avoid writing the type parameter `L` outside of special cases.
pub struct Cell<T, L> {
    /// Access to the state this cell shares with all `Source`s.
    /// Publicly, only `Cell` can be used to mutate the `CellSource` data.
    shared: Arc<CellSource<T, L>>,
}

/// [`Cell::as_source()`] implementation.
///
/// This type can be coerced to `dyn Source`.
///
/// # Generic parameters
///
/// * `T` is the type of the value.
/// * `L` is the type of [`Listener`] this source accepts,
///   usually a trait object type such as [`sync::DynListener`].
//---
// Design note: Despite being public, this also serves as the internal *shared* data structure
// between `Cell` and its sources. By doing this, we can hand out `Arc<CellSource>` for coercion
// to `Arc<dyn Source>` and avoid a layer of indirection that would be present if we instead did
//
//     struct CellSource<T, L>(Arc<CellShared<T, L>>);
//     impl<T, L> Source for CellSource<T, L> {...}
pub struct CellSource<T, L> {
    value_mutex: Mutex<T>,
    notifier: Notifier<(), L>,
}

// -------------------------------------------------------------------------------------------------

impl<T: Clone, L: Listener<()>> Cell<T, L> {
    /// Creates a new [`Cell`] containing the given value.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            shared: Arc::new(CellSource {
                value_mutex: Mutex::new(value),
                notifier: Notifier::new(),
            }),
        }
    }

    /// Returns a [`Source`] which provides read-only access to the value
    /// held by this cell.
    ///
    /// It can be coerced to [`sync::DynSource`] or [`unsync::DynSource`]
    /// when `T` and `L` meet the required bounds.
    #[must_use]
    pub fn as_source(&self) -> Arc<CellSource<T, L>> {
        self.shared.clone()
    }

    /// Returns a clone of the current value of this cell.
    #[must_use]
    pub fn get(&self) -> T {
        self.shared.value_mutex.lock().clone()
    }

    /// Sets the value of this cell and sends out a change notification.
    ///
    /// Note that this does not test whether the current value is equal to avoid redundant
    /// notifications; if that is desired, call [`set_if_unequal()`](Self::set_if_unequal).
    ///
    /// Caution: While listeners are *expected* not to have immediate side effects on
    /// notification, this cannot be enforced.
    pub fn set(&self, value: T) {
        // Using mem::replace instead of assignment so that _old_value will be dropped
        // after unlocking instead of before.
        let _old_value = mem::replace(&mut *self.shared.value_mutex.lock(), value);

        self.shared.notifier.notify(&());
    }

    /// Sets the contained value to the given value iff they are unequal.
    ///
    /// Compared to [`set()`](Self::set), this avoids sending spurious change notifications.
    ///
    /// Caution: This executes `PartialEq::eq()` with the lock held; this may delay readers of
    /// the value.
    ///
    /// # Example
    ///
    /// ```
    /// use nosy::{Listen as _, Flag, unsync::Cell};
    ///
    /// let cell = Cell::new(1);
    /// let flag = Flag::new(false);
    /// cell.as_source().listen(flag.listener());
    ///
    /// cell.set_if_unequal(2);
    /// assert_eq!(flag.get_and_clear(), true);
    /// cell.set_if_unequal(2);
    /// assert_eq!(flag.get_and_clear(), false);
    /// ```
    pub fn set_if_unequal(&self, value: T)
    where
        T: PartialEq,
    {
        let mut guard: MutexGuard<'_, T> = self.shared.value_mutex.lock();
        if value == *guard {
            return;
        }

        let _old_value = mem::replace(&mut *guard, value);

        // Don't hold the lock while notifying.
        // Listeners shouldn't be trying to read immediately, but we don't want to create
        // this deadlock opportunity regardless.
        drop(guard);

        self.shared.notifier.notify(&());
    }

    /// Sets the contained value by modifying a clone of the old value using the provided
    /// function.
    ///
    /// Note: this function is not atomic, in that other modifications can be made between
    /// the time this function reads the current value and writes the new one. It is not any more
    /// powerful than calling `get()` followed by `set()`.
    pub fn update_mut<F: FnOnce(&mut T)>(&self, f: F) {
        let mut value = self.get();
        f(&mut value);
        self.set(value);
    }
}

// -------------------------------------------------------------------------------------------------

impl<T, L: Listener<()>> Listen for CellSource<T, L> {
    type Msg = ();
    type Listener = L;

    fn listen_raw(&self, listener: Self::Listener) {
        self.notifier.listen_raw(listener);
    }
}

impl<T, L> Source for CellSource<T, L>
where
    T: Clone + fmt::Debug,
    L: Listener<()>,
{
    type Value = T;

    fn get(&self) -> T {
        T::clone(&*self.value_mutex.lock())
    }
}

// -------------------------------------------------------------------------------------------------

/// Like [`Cell`], but allows borrowing the current value,
/// at the cost of requiring `&mut` access to set it, and storing an extra clone.
///
/// We recommend that you use the type aliases [`sync::CellWithLocal`] or [`unsync::CellWithLocal`],
/// to avoid writing the type parameter `L` outside of special cases.
///
/// # Generic parameters
///
/// * `T` is the type of the value.
/// * `L` is the type of [`Listener`] this cell accepts,
///   usually a trait object type such as [`sync::DynListener`].
pub struct CellWithLocal<T, L> {
    cell: Cell<T, L>,
    value: T,
}

impl<T, L> CellWithLocal<T, L>
where
    T: Clone,
    L: Listener<()>,
{
    /// Creates a new [`CellWithLocal`] containing the given value.
    pub fn new(value: T) -> Self {
        Self {
            value: value.clone(),
            cell: Cell::new(value),
        }
    }

    /// Returns a [`Source`] which provides read-only access to the value
    /// held by this cell.
    ///
    /// It can be coerced to [`sync::DynSource`] or [`unsync::DynSource`]
    /// when `T` and `L` meet the required bounds.
    pub fn as_source(&self) -> Arc<CellSource<T, L>> {
        self.cell.as_source()
    }

    /// Sets the value of this cell and sends out a change notification.
    ///
    /// This does not test whether the current value is equal to avoid redundant notifications.
    ///
    /// Caution: While listeners are *expected* not to have immediate side effects on
    /// notification, this cannot be enforced.
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.cell.set(self.value.clone());
    }

    /// Returns a reference to the current value of this cell.
    pub fn get(&self) -> &T {
        &self.value
    }
}

// -------------------------------------------------------------------------------------------------

impl<T: Clone + fmt::Debug, L: Listener<()>> fmt::Debug for Cell<T, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("Cell");
        // Note that we do not simply lock the mutex and avoid cloning the value.
        // This is to ensure that we cannot deadlock or delay by holding the lock while
        // waiting for writes to the caller-provided output stream.
        ds.field("value", &self.get());
        format_cell_metadata(&mut ds, &self.shared);
        ds.finish()
    }
}
impl<T: Clone + fmt::Debug, L: Listener<()>> fmt::Debug for CellSource<T, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            value_mutex: _, // used via get()
            notifier,
        } = self;
        let mut ds = f.debug_struct("CellSource");
        // Note that we do not simply lock the mutex and avoid cloning the value.
        // This is to ensure that we cannot deadlock or delay by holding the lock while
        // waiting for writes to the caller-provided output stream.
        ds.field("value", &self.get());
        // can't use format_cell_metadata() because we don't have the Arc
        ds.field("listeners", &notifier.count());
        ds.finish()
    }
}
impl<T: fmt::Debug, L: Listener<()>> fmt::Debug for CellWithLocal<T, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("CellWithLocal");
        ds.field("value", &self.value);
        format_cell_metadata(&mut ds, &self.cell.shared);
        ds.finish()
    }
}

// Pointer printing implementations to enable determining whether a cell and a source share
// state. Including the debug_struct to avoid confusion over double indirection.
//
// (We would like to impl<T, L> fmt::Pointer for Arc<CellSource<T, L>> {}
// but that implementation is overlapping.)
impl<T, L> fmt::Pointer for Cell<T, L> {
    /// Prints the address of the cell's state storage, which is shared with
    /// [`Source`]s created from this cell.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("Cell");
        ds.field("cell_address", &Arc::as_ptr(&self.shared));
        ds.finish()
    }
}
impl<T, L> fmt::Pointer for CellWithLocal<T, L> {
    /// Prints the address of the cell's state storage, which is shared with
    /// [`Source`]s created from this cell.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("CellWithLocal");
        ds.field("cell_address", &Arc::as_ptr(&self.cell.shared));
        ds.finish()
    }
}

fn format_cell_metadata<T, L: Listener<()>>(
    ds: &mut fmt::DebugStruct<'_, '_>,
    storage: &Arc<CellSource<T, L>>,
) {
    ds.field("owners", &Arc::strong_count(storage));
    ds.field("listeners", &storage.notifier.count());
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sink;
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use pretty_assertions::assert_eq;

    #[test]
    fn cell_and_source_debug() {
        let cell = Cell::<Vec<&'static str>, crate::unsync::DynListener<()>>::new(vec!["hi"]);
        let source = cell.as_source();
        assert_eq!(
            format!("{cell:#?}"),
            indoc::indoc! {
                r#"Cell {
                    value: [
                        "hi",
                    ],
                    owners: 2,
                    listeners: 0,
                }"#
            }
        );
        // Note we can't print `owners` because that information isn't carried along,
        // unfortunately.
        assert_eq!(
            format!("{source:#?}"),
            indoc::indoc! {
               r#"CellSource {
                    value: [
                        "hi",
                    ],
                    listeners: 0,
                }"#
            }
        );
    }

    #[test]
    fn cell_usage() {
        let cell = Cell::<i32, crate::unsync::DynListener<()>>::new(0i32);

        let s = cell.as_source();
        let sink = Sink::new();
        s.listen(sink.listener());

        assert_eq!(sink.drain(), vec![]);
        cell.set(1);
        assert_eq!(1, s.get());
        assert_eq!(sink.drain(), vec![()]);
    }

    #[test]
    fn cell_source_clone() {
        let cell = Cell::<i32, crate::unsync::DynListener<()>>::new(0i32);
        let s = cell.as_source();
        let s = s.clone();
        cell.set(1);
        assert_eq!(s.get(), 1);
    }
}
