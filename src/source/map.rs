use core::fmt;

use crate::{IntoListener, Listen, Source};

/// A [`Source`] whose values are produced by applying a function to another source.
///
/// Construct this by calling [`Source::map()`]; see the further documentation there.
pub struct Map<S, F> {
    // TODO: We can allow one of these two to be unsized, but not both.
    // Which one would be better?
    pub(super) function: F,
    pub(super) source: S,
}

impl<S: Source, F> Listen for Map<S, F> {
    type Msg = ();

    type Listener = S::Listener;

    fn listen_raw(&self, listener: Self::Listener) {
        self.source.listen_raw(listener)
    }

    fn listen<L: IntoListener<Self::Listener, Self::Msg>>(&self, listener: L) {
        self.source.listen(listener)
    }
}

impl<S, F, O> Source for Map<S, F>
where
    S: Source,
    F: Fn(S::Value) -> O,
{
    type Value = O;

    fn get(&self) -> Self::Value {
        (self.function)(self.source.get())
    }
}

impl<S: fmt::Debug, F> fmt::Debug for Map<S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Map")
            .field(
                "function",
                &crate::util::Unquote::type_name_of(&self.function),
            )
            .field("source", &self.source)
            .finish()
    }
}
