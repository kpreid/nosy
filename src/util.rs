use core::fmt;

/// Wraps a string such that it does not get quoted when printed with [`fmt::Debug`].
pub(crate) struct Unquote<'a>(pub &'a str);

impl Unquote<'static> {
    pub(crate) fn type_name_of<T: ?Sized>(val: &T) -> Self {
        Unquote(core::any::type_name_of_val(val))
    }

    pub(crate) fn type_name<T: ?Sized>() -> Self {
        Unquote(core::any::type_name::<T>())
    }
}

impl fmt::Debug for Unquote<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
