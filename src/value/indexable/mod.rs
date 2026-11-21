mod index;
mod name;

pub use index::Index;
pub use name::Name;

/// Marker trait for values that can be used as an index key when looking
/// up elements inside compound values (arrays, objects, etc.).
///
/// Every key can be converted into a canonical [`Index`]; the trait itself
/// declares no methods.
///
/// Implemented by [`Name`] and [`Index`].
pub trait IndexKey: Into<Index> {}

impl IndexKey for Name {}
impl IndexKey for Index {}

/// A compound value that can be looked up with an index key.
pub trait Indexable {
    /// The element type returned by a successful lookup.
    type Output;

    /// Returns a shared reference to the element identified by `index`.
    ///
    /// Returns `None` when the key does not resolve to a stored element.
    fn get<I: IndexKey>(&self, index: I) -> Option<&Self::Output>;

    /// Returns a mutable reference to the element identified by `index`.
    ///
    /// Returns `None` when the key does not resolve to a stored element.
    fn get_mut<I: IndexKey>(&mut self, index: I) -> Option<&mut Self::Output>;

    /// Returns an owned copy of the element identified by `index`.
    ///
    /// Returns `None` when the key does not resolve to a stored element.
    fn get_owned<I: IndexKey>(&self, index: I) -> Option<Self::Output>
    where
        Self::Output: Clone;
}
