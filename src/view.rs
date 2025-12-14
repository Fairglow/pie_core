//! Immutable view implementation.

use crate::list::Iter;
use crate::{ElemPool, PieList};
use core::cmp::Ordering;
use core::fmt;

/// A lightweight, temporary view into a [`PieList`] that borrows the backing [`ElemPool`].
///
/// `PieView` bundles the list handle and the data pool together into a single, lightweight
/// struct. This enables standard Rust traits like [`IntoIterator`], [`Debug`], and [`PartialEq`],
/// which require access to the data without passing a secondary pool argument.
///
/// Since `PieView` only holds shared references, it implements [`Copy`] and [`Clone`]
/// regardless of whether the underlying type `T` implements them.
///
/// # Examples
///
/// ## Printing (Debug)
/// ```
/// use pie_core::{ElemPool, PieList};
///
/// let mut pool = ElemPool::new();
/// let mut list = PieList::new(&mut pool);
/// list.push_back(10, &mut pool).unwrap();
/// list.push_back(20, &mut pool).unwrap();
///
/// let view = list.view(&pool);
/// assert_eq!(format!("{:?}", view), "[10, 20]");
///
/// list.clear(&mut pool);
/// ```
///
/// ## Iterating
/// ```
/// # use pie_core::{ElemPool, PieList};
/// # let mut pool = ElemPool::new();
/// # let mut list = PieList::new(&mut pool);
/// # list.push_back(1, &mut pool).unwrap();
/// # list.push_back(2, &mut pool).unwrap();
/// let view = list.view(&pool);
///
/// let mut sum = 0;
/// for &item in view {
///     sum += item;
/// }
/// assert_eq!(sum, 3);
/// # list.clear(&mut pool);
/// ```
///
/// ## Equality
/// You can compare two lists for equality, even if they live in different pools.
///
/// ```
/// # use pie_core::{ElemPool, PieList};
/// let mut pool1 = ElemPool::new();
/// let mut list1 = PieList::new(&mut pool1);
/// list1.push_back("apple", &mut pool1).unwrap();
///
/// let mut pool2 = ElemPool::new();
/// let mut list2 = PieList::new(&mut pool2);
/// list2.push_back("apple", &mut pool2).unwrap();
///
/// // Compare views
/// assert_eq!(list1.view(&pool1), list2.view(&pool2));
/// # list2.clear(&mut pool2);
/// # list1.clear(&mut pool1);
/// ```
pub struct PieView<'a, T> {
    pub(crate) list: &'a PieList<T>,
    pub(crate) pool: &'a ElemPool<T>,
}

// MANUAL IMPLEMENTATION OF COPY/CLONE
// We cannot use #[derive(Copy, Clone)] because it adds a `T: Copy` bound.
// PieView only holds references, so it is always Copy, even if T is String or Vec.

impl<'a, T> Clone for PieView<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for PieView<'a, T> {}

impl<'a, T> PieView<'a, T> {
    /// Creates a new view for the given `list` using the data in `pool`.
    ///
    /// It is often more ergonomic to use [`PieList::view`] instead.
    pub fn new(list: &'a PieList<T>, pool: &'a ElemPool<T>) -> Self {
        Self { list, pool }
    }

    /// Returns the number of elements in the list.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::<i32>::new(&mut pool);
    /// let view = list.view(&pool);
    /// assert_eq!(view.len(), 0);
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Returns `true` if the list is empty.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::<i32>::new(&mut pool);
    /// let view = list.view(&pool);
    /// assert!(view.is_empty());
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Returns a reference to the front element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// let view = list.view(&pool);
    /// assert_eq!(view.front(), Some(&10));
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn front(&self) -> Option<&T> {
        self.list.front(self.pool)
    }

    /// Returns a reference to the back element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// let view = list.view(&pool);
    /// assert_eq!(view.back(), Some(&10));
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn back(&self) -> Option<&T> {
        self.list.back(self.pool)
    }

    /// Creates an iterator over the list's elements.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(1, &mut pool).unwrap();
    /// # list.push_back(2, &mut pool).unwrap();
    /// let view = list.view(&pool);
    /// let vec: Vec<_> = view.iter().copied().collect();
    /// assert_eq!(vec, vec![1, 2]);
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<'a, T> {
        self.list.iter(self.pool)
    }
}

// ============================================================================
// Trait: Debug
// ============================================================================

impl<'a, T: fmt::Debug> fmt::Debug for PieView<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

// ============================================================================
// Trait: PartialEq / Eq
// ============================================================================

impl<'a, T: PartialEq> PartialEq for PieView<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        // Optimization: If it's the exact same list indices and pool, they are equal.
        if core::ptr::eq(self.list, other.list) && core::ptr::eq(self.pool, other.pool) {
            return true;
        }
        // Otherwise, compare contents via iteration
        self.iter().eq(other.iter())
    }
}

impl<'a, T: Eq> Eq for PieView<'a, T> {}

// ============================================================================
// Trait: PartialOrd / Ord
// ============================================================================

impl<'a, T: PartialOrd> PartialOrd for PieView<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<'a, T: Ord> Ord for PieView<'a, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

// ============================================================================
// Trait: IntoIterator
// ============================================================================

impl<'a, T> IntoIterator for PieView<'a, T> {
    type Item = &'a T;
    // We reuse the existing iterator from the list module instead of defining a new one.
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::{ElemPool, PieList};

    #[test]
    fn test_view_debug_impl() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();
        list.push_back(30, &mut pool).unwrap();

        let view = list.view(&pool);
        let output = format!("{:?}", view);

        assert_eq!(output, "[10, 20, 30]");
        list.clear(&mut pool);
    }

    #[test]
    fn test_view_iteration() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(1, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        list.push_back(3, &mut pool).unwrap();

        let view = list.view(&pool);

        // Test IntoIterator (collect)
        let collected: Vec<&i32> = view.into_iter().collect();
        assert_eq!(collected, vec![&1, &2, &3]);

        // Test syntax sugar loop
        let mut sum = 0;
        for item in view {
            sum += item;
        }
        assert_eq!(sum, 6);
        list.clear(&mut pool);
    }

    #[test]
    fn test_view_partial_eq() {
        let mut pool1 = ElemPool::new();
        let mut list1 = PieList::new(&mut pool1);
        list1.push_back(1, &mut pool1).unwrap();
        list1.push_back(2, &mut pool1).unwrap();

        let mut pool2 = ElemPool::new();
        let mut list2 = PieList::new(&mut pool2);
        list2.push_back(1, &mut pool2).unwrap();
        list2.push_back(2, &mut pool2).unwrap();

        let mut pool3 = ElemPool::new();
        let mut list3 = PieList::new(&mut pool3);
        list3.push_back(1, &mut pool3).unwrap();
        list3.push_back(99, &mut pool3).unwrap();

        let view1 = list1.view(&pool1);
        let view2 = list2.view(&pool2);
        let view3 = list3.view(&pool3);

        assert_eq!(view1, view2, "Lists with same content should be equal");
        assert_ne!(view1, view3, "Lists with different content should not be equal");

        list3.clear(&mut pool3);
        list2.clear(&mut pool2);
        list1.clear(&mut pool1);
    }

    #[test]
    fn test_view_partial_ord() {
        let mut pool = ElemPool::new();
        let mut list1 = PieList::new(&mut pool);
        list1.push_back(1, &mut pool).unwrap();

        let mut list2 = PieList::new(&mut pool);
        list2.push_back(2, &mut pool).unwrap();

        let view1 = list1.view(&pool);
        let view2 = list2.view(&pool);

        assert!(view1 < view2);
        assert!(view2 > view1);

        list1.clear(&mut pool);
        list2.clear(&mut pool);
    }

    #[test]
    fn test_view_with_non_copy_types() {
        // This test ensures that our manual Copy implementation works
        // even if T is not Copy (like String).
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back("hello".to_string(), &mut pool).unwrap();
        list.push_back("world".to_string(), &mut pool).unwrap();

        let view = list.view(&pool);

        // If we hadn't manually implemented Copy, this line would fail to compile
        // because it tries to copy the view (which contains String logic implicitly).
        let output = format!("{:?}", view);
        assert_eq!(output, "[\"hello\", \"world\"]");
        list.clear(&mut pool);
    }
}
