//! Definition of the Index type

use std::{convert::TryFrom, fmt, hash::{Hash, Hasher}, marker::PhantomData};

/// A type-safe, compact handle to an element in an `ElemPool`.
///
/// An `Index<T>` is essentially a wrapper around a `u32`, providing a cheap,
/// `Copy`-able way to reference list elements without relying on raw pointers
/// or garbage collection.
///
/// # Rationale
///
/// Using a custom `Index` type instead of a raw `usize` or `u32` provides
/// several benefits:
/// - **Type Safety:** `Index<Foo>` is a different type from `Index<Bar>`. This
///   prevents accidentally using an index from a pool of `Foo`s to access a
///   pool of `Bar`s. The `PhantomData<T>` marker enforces this at compile time
///   with zero runtime cost.
/// - **"None" State:** The maximum value of `u32` is reserved for `Index::NONE`,
///   creating a clear, efficient "null" or "invalid" state, similar to
///   `Option<T>` but without the added size overhead.
/// - **API Clarity:** Using `Index<T>` in function signatures makes it clear
///   that the function expects a handle to a list element, not just an arbitrary
///   number.
#[derive(Eq)]
pub struct Index<T> {
    ndx: u32,
    _marker: PhantomData<T>,
}

// Manually implement Clone and Copy to avoid trait bounds on `T`.
// This is sound because `PhantomData<T>` has a size of zero.
impl<T> Clone for Index<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Index<T> {}

impl<T> fmt::Debug for Index<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.ndx {
            u32::MAX => write!(f, "Index(-)"),
            _ => write!(f, "{:?}", self.ndx),
        }
    }
}

impl<T> Default for Index<T> {
    /// The default `Index` is `Index::NONE`.
    fn default() -> Self {
        Self::NONE
    }
}

impl<T> PartialEq for Index<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.ndx == other.ndx
    }
}

impl<T> Hash for Index<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ndx.hash(state);
    }
}

impl<T> Index<T> {
    /// An invalid index, conceptually similar to `Option::None`.
    ///
    /// This constant value (`u32::MAX`) is reserved to represent a null or
    /// sentinel link. All valid indices into the `ElemPool` will be less
    /// than this value.
    pub const NONE: Self = Index {
        ndx: u32::MAX,
        _marker: PhantomData,
    };

    /// Returns `true` if the index is valid (i.e., not `Index::NONE`).
    ///
    /// # Example
    /// ```
    /// # use pielist::Index;
    /// let valid_index = Index::<i32>::from(10_u32);
    /// assert!(valid_index.is_some());
    ///
    /// let invalid_index = Index::<i32>::NONE;
    /// assert!(!invalid_index.is_some());
    /// ```
    #[inline]
    pub fn is_some(&self) -> bool {
        self.ndx != u32::MAX
    }

    /// Returns `true` if the index is invalid (i.e., it is `Index::NONE`).
    ///
    /// # Example
    /// ```
    /// # use pielist::Index;
    /// let invalid_index = Index::<i32>::NONE;
    /// assert!(invalid_index.is_none());
    ///
    /// let valid_index = Index::<i32>::from(10_u32);
    /// assert!(!valid_index.is_none());
    /// ```
    #[inline]
    pub fn is_none(&self) -> bool {
        self.ndx == u32::MAX
    }

    /// Converts the `Index` to an `Option<usize>`.
    ///
    /// This is used internally to safely access the `ElemPool`'s underlying `Vec`.
    /// Returns `Some(usize)` for a valid index, and `None` for `Index::NONE`.
    #[inline]
    pub(crate) fn get(&self) -> Option<usize> {
        if self.is_some() {
            Some(self.ndx as usize)
        } else {
            None
        }
    }
}

impl<T> From<u32> for Index<T> {
    /// Creates an `Index` from a raw `u32`.
    #[inline]
    fn from(ndx: u32) -> Index<T> {
        Self {
            ndx,
            _marker: PhantomData,
        }
    }
}

impl<T> From<usize> for Index<T> {
    /// Creates an `Index` from a `usize`.
    ///
    /// This conversion is fallible. Values greater than or equal to
    /// `u32::MAX` will be converted into an invalid index (`Index::NONE`).
    /// This prevents out-of-bounds errors when converting from `usize` on
    /// 64-bit platforms.
    ///
    /// # Example
    /// ```
    /// # use pielist::Index;
    /// let index1 = Index::<char>::from(123_usize);
    /// assert!(index1.is_some());
    ///
    /// let index2 = Index::<char>::from(u32::MAX as usize);
    /// assert!(index2.is_none());
    /// ```
    #[inline]
    fn from(index: usize) -> Index<T> {
        Index::from(u32::try_from(index).unwrap_or(u32::MAX))
    }
}

impl<T> fmt::Display for Index<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            Some(n) => write!(f, "{}", n),
            None => write!(f, "-"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // A dummy type to use for the generic Index<T>
    #[derive(PartialEq, Eq)]
    struct MyData;

    #[test]
    fn test_creation_and_constants() {
        let index_from_u32 = Index::<MyData>::from(42_u32);
        assert_eq!(index_from_u32.ndx, 42);

        let index_from_usize = Index::<MyData>::from(123_usize);
        assert_eq!(index_from_usize.ndx, 123);

        // Check the NONE constant
        assert_eq!(Index::<MyData>::NONE.ndx, u32::MAX);
    }

    #[test]
    fn test_default() {
        // Default should be equivalent to NONE
        let default_index = Index::<MyData>::default();
        assert_eq!(default_index, Index::NONE);
        assert!(default_index.is_none());
    }

    #[test]
    fn test_state_checks() {
        let valid_index = Index::<MyData>::from(10_u32);
        let none_index = Index::<MyData>::NONE;

        // is_some()
        assert!(valid_index.is_some());
        assert!(!none_index.is_some());

        // is_none()
        assert!(!valid_index.is_none());
        assert!(none_index.is_none());
    }

    #[test]
    fn test_get() {
        let valid_index = Index::<MyData>::from(99_u32);
        let none_index = Index::<MyData>::NONE;

        assert_eq!(valid_index.get(), Some(99_usize));
        assert_eq!(none_index.get(), None);
    }

    #[test]
    fn test_from_usize_overflow() {
        // A usize that fits into u32
        let normal_usize = u32::MAX - 1;
        let index1 = Index::<MyData>::from(normal_usize as usize);
        assert_eq!(index1.ndx, u32::MAX - 1);
        assert!(index1.is_some());

        // A usize that is exactly u32::MAX should become NONE
        let overflow_usize_1 = u32::MAX;
        let index2 = Index::<MyData>::from(overflow_usize_1 as usize);
        assert_eq!(index2, Index::NONE);
        assert!(index2.is_none());

        // A larger usize should also become NONE
        let overflow_usize_2 = u64::MAX as usize;
        let index3 = Index::<MyData>::from(overflow_usize_2);
        assert_eq!(index3, Index::NONE);
        assert!(index3.is_none());
    }

    #[test]
    fn test_equality() {
        let index1a = Index::<MyData>::from(1_u32);
        let index1b = Index::<MyData>::from(1_u32);
        let index2 = Index::<MyData>::from(2_u32);
        let none1 = Index::<MyData>::NONE;
        let none2 = Index::<MyData>::NONE;

        assert_eq!(index1a, index1b);
        assert_eq!(none1, none2);
        assert_ne!(index1a, index2);
        assert_ne!(index1a, none1);
    }

    #[test]
    fn test_clone_and_copy() {
        let original = Index::<MyData>::from(50_u32);

        // Test Copy
        let copied = original;
        assert_eq!(original, copied);

        // Test Clone
        let cloned = original.clone();
        assert_eq!(original, cloned);

        // Ensure they are distinct in memory but equal in value
        assert_eq!(original.ndx, copied.ndx);
        assert_eq!(original.ndx, cloned.ndx);
    }

    #[test]
    fn test_hash() {
        let mut set = HashSet::new();
        let index1 = Index::<MyData>::from(1_u32);
        let index1_dup = Index::<MyData>::from(1_u32);
        let index2 = Index::<MyData>::from(2_u32);
        let none_index = Index::<MyData>::NONE;

        assert!(set.insert(index1));
        assert!(set.insert(index2));
        assert!(set.insert(none_index));

        // Inserting a duplicate should return false
        assert!(!set.insert(index1_dup));

        // Check size and membership
        assert_eq!(set.len(), 3);
        assert!(set.contains(&index1));
        assert!(set.contains(&index2));
        assert!(set.contains(&none_index));
        assert!(!set.contains(&Index::from(99_u32)));
    }

    #[test]
    fn test_formatting() {
        let valid_index = Index::<MyData>::from(123_u32);
        let none_index = Index::<MyData>::NONE;

        // Test Debug format
        let debug_valid = format!("{:?}", valid_index);
        let debug_none = format!("{:?}", none_index);
        assert_eq!(debug_valid, "123");
        assert_eq!(debug_none, "Index(-)");

        // Test Display format
        let display_valid = format!("{}", valid_index);
        let display_none = format!("{}", none_index);
        assert_eq!(display_valid, "123");
        assert_eq!(display_none, "-");
    }
}
