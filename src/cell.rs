use alloc::sync::Arc;
use core::fmt;

use crate::{IntoListener, Listen, Listener, LoadStore, Notifier, Source};

#[cfg(doc)]
use crate::{sync, unsync};

// -------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
/// An interior-mutable container for a single value, which notifies when the value is changed.
///
/// This type is a generic wrapper for any kind of interior mutability that can implement
/// [`LoadStore`]
/// — atomics, mutexes, or even [`core::cell::Cell`] —
/// and adds change notifications and the [`Source`] trait to that container type.
///
/// When used with a mutex to manage access to the value,
/// the mutex will only be locked as long as necessary to clone or compare the value, so
/// deadlock is impossible unless the value’s [`Clone`] or [`PartialEq`] uses a shared lock itself.
///
/// In general, reading the value requires cloning it, so if the clone is not cheap,
/// consider wrapping the value with [`Arc`] so that the cost is only updating of reference counts.
///
/// # Generic parameters
///
/// * `S` is the type of the interior-mutable storage of the value, such as a mutex or an atomic
///   type.
///   The type of the value itself is equal to <code>&lt;S as [LoadStore]&gt;::Value</code>.
/// * `L` is the type of [`Listener`] this cell accepts,
///   usually a trait object type such as [`sync::DynListener`].
///
/// We recommend that you use the type aliases [`sync::Cell`] or [`unsync::Cell`] in simple cases.
///
// TODO: Add basic example particularly so users see how `S` works
pub struct Cell<S, L> {
    /// Access to the state this cell shares with all `Source`s.
    /// Publicly, only `Cell` can be used to mutate the `CellSource` data.
    shared: Arc<CellSource<S, L>>,
}

/// [`Cell::as_source()`] implementation.
///
/// This type can be coerced to `dyn Source`.
///
/// # Generic parameters
///
/// The parameters are equal to those of its associated [`Cell`].
///
/// * `S` is the type of the interior-mutable storage of the value, such as a mutex or an atomic
///   type.
///   The type of the value itself is equal to <code>&lt;S as [LoadStore]&gt;::Value</code>.
/// * `L` is the type of [`Listener`] this source accepts,
///   usually a trait object type such as [`sync::DynListener`].
//---
// Design note: Despite being public, this also serves as the internal *shared* data structure
// between `Cell` and its sources. By doing this, we can hand out `Arc<CellSource>` for coercion
// to `Arc<dyn Source>` and avoid a layer of indirection that would be present if we instead did
//
//     struct CellSource<T, L>(Arc<CellShared<T, L>>);
//     impl<T, L> Source for CellSource<T, L> {...}
pub struct CellSource<S, L> {
    value_mutex: S,
    notifier: Notifier<(), L>,
}

// -------------------------------------------------------------------------------------------------

impl<S: LoadStore, L: Listener<()>> Cell<S, L> {
    /// Creates a new [`Cell`] containing the given value.
    #[must_use]
    pub fn new(value: S::Value) -> Self {
        Self {
            shared: Arc::new(CellSource {
                value_mutex: S::new(value),
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
    pub fn as_source(&self) -> Arc<CellSource<S, L>> {
        self.shared.clone()
    }

    /// Returns a clone of the current value of this cell.
    #[must_use]
    pub fn get(&self) -> S::Value {
        self.shared.value_mutex.get()
    }

    /// Sets the value of this cell and sends out a change notification.
    ///
    /// Note that this does not test whether the current value is equal to avoid redundant
    /// notifications; if that is desired, call [`set_if_unequal()`](Self::set_if_unequal).
    ///
    /// Caution: While listeners are *expected* not to have immediate side effects on
    /// notification, this cannot be enforced.
    pub fn set(&self, value: S::Value) {
        let shared: &CellSource<S, L> = &self.shared;

        let old_value = shared.value_mutex.replace(value);
        shared.notifier.notify(&());
        drop(old_value);
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
    pub fn set_if_unequal(&self, value: S::Value)
    where
        S::Value: PartialEq,
    {
        match self.shared.value_mutex.replace_if_unequal(value) {
            Ok(old_value) => {
                self.shared.notifier.notify(&());
                drop(old_value);
            }
            Err(new_value) => drop(new_value),
        }
    }

    /// Sets the contained value by modifying a clone of the old value using the provided
    /// function.
    ///
    /// Note: this function is not atomic, in that other modifications can be made between
    /// the time this function reads the current value and writes the new one. It is not any more
    /// powerful than calling `get()` followed by `set()`.
    pub fn update_mut<F: FnOnce(&mut S::Value)>(&self, f: F) {
        let mut value = self.get();
        f(&mut value);
        self.set(value);
    }
}

impl<S, L> Drop for Cell<S, L> {
    fn drop(&mut self) {
        // If the `Cell` is dropped, then the `Source`s should not retain their `Listener`s.
        self.shared.notifier.close()
    }
}

// -------------------------------------------------------------------------------------------------

impl<S, L: Listener<()>> Listen for CellSource<S, L> {
    type Msg = ();
    type Listener = L;

    fn listen_raw(&self, listener: Self::Listener) {
        self.notifier.listen_raw(listener);
    }

    fn listen<L2: IntoListener<Self::Listener, Self::Msg>>(&self, listener: L2) {
        self.notifier.listen(listener)
    }
}
impl<S, L: Listener<()>> Listen for Cell<S, L> {
    type Msg = ();
    type Listener = L;

    fn listen_raw(&self, listener: Self::Listener) {
        self.shared.listen_raw(listener);
    }

    fn listen<L2: IntoListener<Self::Listener, Self::Msg>>(&self, listener: L2) {
        self.shared.listen(listener)
    }
}
impl<S: LoadStore<Value: Clone>, L: Listener<()>> Listen for CellWithLocal<S, L> {
    type Msg = ();
    type Listener = L;

    fn listen_raw(&self, listener: Self::Listener) {
        self.cell.listen_raw(listener);
    }

    fn listen<L2: IntoListener<Self::Listener, Self::Msg>>(&self, listener: L2) {
        self.cell.listen(listener)
    }
}

impl<S, L> Source for CellSource<S, L>
where
    S: LoadStore<Value: fmt::Debug>,
    L: Listener<()>,
{
    type Value = S::Value;

    fn get(&self) -> S::Value {
        self.value_mutex.get()
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "sync"), allow(rustdoc::broken_intra_doc_links))]
/// Like [`Cell`], but allows borrowing the current value,
/// at the cost of requiring `&mut` access to set it, and storing an extra clone.
///
/// We recommend that you use the type aliases [`sync::CellWithLocal`] or [`unsync::CellWithLocal`],
/// to avoid writing the type parameter `L` outside of special cases.
///
/// Note: If `S` is not a mutex and `S::Value` is not large, there is probably no advantage to
/// using this type instead of [`Cell`].
///
/// # Generic parameters
///
/// * `S` is the type of the interior-mutable storage of the value, such as a mutex or an atomic
///   type.
///   The type of the value itself is equal to <code>&lt;S as [LoadStore]&gt;::Value</code>.
/// * `L` is the type of [`Listener`] this cell accepts,
///   usually a trait object type such as [`sync::DynListener`].
pub struct CellWithLocal<S: LoadStore, L> {
    cell: Cell<S, L>,
    value: S::Value,
}

impl<S, L> CellWithLocal<S, L>
where
    S: LoadStore<Value: Clone>,
    L: Listener<()>,
{
    /// Creates a new [`CellWithLocal`] containing the given value.
    pub fn new(value: S::Value) -> Self {
        Self {
            value: value.clone(),
            cell: Cell::new(value),
        }
    }

    /// Returns a [`Source`] which provides read-only access to the value
    /// held by this cell.
    ///
    /// It can be coerced to [`sync::DynSource`] or [`unsync::DynSource`]
    /// when `S` and `L` meet the required bounds.
    pub fn as_source(&self) -> Arc<CellSource<S, L>> {
        self.cell.as_source()
    }

    /// Sets the value of this cell and sends out a change notification.
    ///
    /// This does not test whether the current value is equal to avoid redundant notifications.
    ///
    /// Caution: While listeners are *expected* not to have immediate side effects on
    /// notification, this cannot be enforced.
    pub fn set(&mut self, value: S::Value) {
        self.value = value;
        self.cell.set(self.value.clone());
    }

    /// Returns a reference to the current value of this cell.
    pub fn get(&self) -> &S::Value {
        &self.value
    }
}

// -------------------------------------------------------------------------------------------------

impl<S: LoadStore<Value: Default>, L: Listener<()>> Default for Cell<S, L> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}
impl<S: LoadStore<Value: Clone + Default>, L: Listener<()>> Default for CellWithLocal<S, L> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

// -------------------------------------------------------------------------------------------------

impl<S: LoadStore<Value: fmt::Debug>, L: Listener<()>> fmt::Debug for Cell<S, L> {
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
impl<S: LoadStore<Value: fmt::Debug>, L: Listener<()>> fmt::Debug for CellSource<S, L> {
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
impl<S: LoadStore<Value: fmt::Debug>, L: Listener<()>> fmt::Debug for CellWithLocal<S, L> {
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
// (We would like to impl<S, L> fmt::Pointer for Arc<CellSource<S, L>> {}
// but that implementation is overlapping.)
impl<S, L: Listener<()>> fmt::Pointer for Cell<S, L> {
    /// Prints the address of the cell's state storage, which is shared with
    /// [`Source`]s created from this cell.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("Cell");
        ds.field("cell_address", &Arc::as_ptr(&self.shared));
        ds.finish()
    }
}
impl<S: LoadStore, L: Listener<()>> fmt::Pointer for CellWithLocal<S, L> {
    /// Prints the address of the cell's state storage, which is shared with
    /// [`Source`]s created from this cell.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("CellWithLocal");
        ds.field("cell_address", &Arc::as_ptr(&self.cell.shared));
        ds.finish()
    }
}

fn format_cell_metadata<S, L: Listener<()>>(
    ds: &mut fmt::DebugStruct<'_, '_>,
    storage: &Arc<CellSource<S, L>>,
) {
    ds.field("owners", &Arc::strong_count(storage));
    ds.field("listeners", &storage.notifier.count());
}
