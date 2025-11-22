use core::cmp::Ordering;
use core::fmt;
use core::marker::PhantomData;

use crate::index::Index;
use crate::{ElemPool, PieList};

/// A lightweight, temporary view into a [`PieList`] that borrows the backing [`ElemPool`].
///
/// # The Problem
/// Because `pie_core` uses an arena allocator (`ElemPool`), a `PieList` structure only holds
/// indices. It does not "own" the data directly. This prevents it from implementing standard
/// Rust traits like [`IntoIterator`], [`Debug`], or [`PartialEq`], because those traits expect
/// to access the data without asking for a second argument (the pool).
///
/// # The Solution: `PieView`
/// `PieView` bundles the list handle and the data pool together into a single, lightweight
/// struct. This struct implements all the standard traits you expect from a collection.
///
/// # Examples
///
/// ## Printing a List (Debug)
/// ```rust
/// use pie_core::{ElemPool, PieList, PieView};
///
/// let mut pool = ElemPool::new();
/// let mut list = PieList::new(&mut pool);
/// list.push_back(10, &mut pool).unwrap();
/// list.push_back(20, &mut pool).unwrap();
///
/// // Standard list doesn't implement Debug:
/// // println!("{:?}", list); // Compile Error!
///
/// // The View does:
/// let view = PieView::new(&list, &pool);
/// assert_eq!(format!("{:?}", view), "[10, 20]");
/// ```
///
/// ## Iterating (for loop)
/// ```rust
/// # use pie_core::{ElemPool, PieList, PieView};
/// # let mut pool = ElemPool::new();
/// # let mut list = PieList::new(&mut pool);
/// # list.push_back(1, &mut pool).unwrap();
/// # list.push_back(2, &mut pool).unwrap();
/// // Use the view in a for loop:
/// let mut sum = 0;
/// for &item in PieView::new(&list, &pool) {
///     sum += item;
/// }
/// assert_eq!(sum, 3);
/// ```
///
/// ## Comparing Lists (PartialEq)
/// You can compare two lists for equality, even if they live in different pools,
/// or if one is a `Vec` (via iteration comparison, though direct `PartialEq` is for `PieView` vs `PieView`).
///
/// ```rust
/// # use pie_core::{ElemPool, PieList, PieView};
/// let mut pool1 = ElemPool::new();
/// let mut list1 = PieList::new(&mut pool1);
/// list1.push_back("apple", &mut pool1).unwrap();
///
/// let mut pool2 = ElemPool::new();
/// let mut list2 = PieList::new(&mut pool2);
/// list2.push_back("apple", &mut pool2).unwrap();
///
/// // Compare views:
/// assert_eq!(PieView::new(&list1, &pool1), PieView::new(&list2, &pool2));
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
    /// # Arguments
    /// * `list` - A reference to the `PieList` structure (indices).
    /// * `pool` - A reference to the `ElemPool` where the data resides.
    ///
    /// # Panics
    /// This function does not panic, but using the resulting view might panic if
    /// the `list` contains indices that are invalid for the provided `pool` (e.g.
    /// if you mix up pools).
    pub fn new(list: &'a PieList<T>, pool: &'a ElemPool<T>) -> Self {
        Self { list, pool }
    }
}

// ============================================================================
// Trait: Debug
// ============================================================================

impl<'a, T: fmt::Debug> fmt::Debug for PieView<'a, T> {
    /// Formats the list elements using the backing pool.
    ///
    /// Output format is standard list style: `[elem1, elem2, elem3]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We dereference *self to pass the Copy view.
        // .entries() accepts IntoIterator, so we don't need explicit .into_iter() call.
        f.debug_list().entries(*self).finish()
    }
}

// ============================================================================
// Trait: PartialEq / Eq
// ============================================================================

impl<'a, T: PartialEq> PartialEq for PieView<'a, T> {
    /// Checks if two lists contain the same elements in the same order.
    ///
    /// This performs a deep comparison of the values (`T`), not the indices.
    /// Two lists stored in different pools are considered equal if their contents match.
    fn eq(&self, other: &Self) -> bool {
        // Optimization: If it's the exact same list indices and pool, they are equal.
        if core::ptr::eq(self.list, other.list) && core::ptr::eq(self.pool, other.pool) {
            return true;
        }

        let mut iter_self = (*self).into_iter();
        let mut iter_other = (*other).into_iter();

        loop {
            match (iter_self.next(), iter_other.next()) {
                (Some(a), Some(b)) if a == b => continue,
                (None, None) => return true,
                _ => return false,
            }
        }
    }
}

impl<'a, T: Eq> Eq for PieView<'a, T> {}

// ============================================================================
// Trait: PartialOrd / Ord
// ============================================================================

impl<'a, T: PartialOrd> PartialOrd for PieView<'a, T> {
    /// Compares two lists lexicographically.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let mut iter_self = (*self).into_iter();
        let mut iter_other = (*other).into_iter();

        loop {
            match (iter_self.next(), iter_other.next()) {
                (Some(a), Some(b)) => match a.partial_cmp(b) {
                    Some(Ordering::Equal) => continue,
                    non_eq => return non_eq,
                },
                (None, None) => return Some(Ordering::Equal),
                (None, Some(_)) => return Some(Ordering::Less),
                (Some(_), None) => return Some(Ordering::Greater),
            }
        }
    }
}

impl<'a, T: Ord> Ord for PieView<'a, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

// ============================================================================
// Trait: IntoIterator
// ============================================================================

impl<'a, T> IntoIterator for PieView<'a, T> {
    type Item = &'a T;
    type IntoIter = PieViewIter<'a, T>;

    /// Creates an iterator that yields references (`&'a T`) to the elements in the list.
    fn into_iter(self) -> Self::IntoIter {
        // Retrieve the sentinel node to determine where to start and stop.
        let sentinel_node = self
            .pool
            .get(self.list.sentinel)
            .expect("PieList Sentinel is missing/invalid");

        PieViewIter {
            curr: sentinel_node.next, // Start at the node *after* the sentinel
            sentinel: self.list.sentinel,
            pool: self.pool,
            _marker: PhantomData,
        }
    }
}

/// An iterator over the elements of a [`PieList`] via a [`PieView`].
///
/// This struct is created by the [`into_iter`](PieView::into_iter) method on `PieView`
/// (or simply by using `PieView` in a `for` loop).
pub struct PieViewIter<'a, T> {
    curr: Index<T>,
    sentinel: Index<T>,
    pool: &'a ElemPool<T>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for PieViewIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        // If the current node is the sentinel, we have reached the end of the list.
        if self.curr == self.sentinel {
            return None;
        }

        // Fetch the node from the pool
        let node = self
            .pool
            .get(self.curr)
            .expect("Corrupted List: Next pointer invalid");

        // Advance to the next node
        self.curr = node.next;

        // Return the data.
        // node.data is Option<T>. We need Option<&T>.
        // .as_ref() converts &Option<T> to Option<&T>.
        node.data.as_ref()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElemPool, PieList};

    #[test]
    fn test_view_debug_impl() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();
        list.push_back(30, &mut pool).unwrap();

        let view = PieView::new(&list, &pool);
        let output = format!("{:?}", view);

        assert_eq!(output, "[10, 20, 30]");
    }

    #[test]
    fn test_view_iteration() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(1, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        list.push_back(3, &mut pool).unwrap();

        let view = PieView::new(&list, &pool);

        // Test IntoIterator (collect)
        let collected: Vec<&i32> = view.into_iter().collect();
        assert_eq!(collected, vec![&1, &2, &3]);

        // Test syntax sugar loop
        let mut sum = 0;
        for item in view {
            sum += item;
        }
        assert_eq!(sum, 6);
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

        let view1 = PieView::new(&list1, &pool1);
        let view2 = PieView::new(&list2, &pool2);
        let view3 = PieView::new(&list3, &pool3);

        assert_eq!(view1, view2, "Lists with same content should be equal");
        assert_ne!(view1, view3, "Lists with different content should not be equal");
    }

    #[test]
    fn test_view_with_non_copy_types() {
        // This test ensures that our manual Copy implementation works
        // even if T is not Copy (like String).
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back("hello".to_string(), &mut pool).unwrap();
        list.push_back("world".to_string(), &mut pool).unwrap();

        let view = PieView::new(&list, &pool);

        // If we hadn't manually implemented Copy, this line would fail to compile
        // because it tries to copy the view (which contains String logic implicitly).
        let output = format!("{:?}", view);
        assert_eq!(output, "[\"hello\", \"world\"]");
    }

    #[test]
    fn test_view_empty_list() {
        let mut pool = ElemPool::new();
        let list: PieList<i32> = PieList::new(&mut pool);

        let view = PieView::new(&list, &pool);

        assert_eq!(format!("{:?}", view), "[]");
        assert!(view.into_iter().next().is_none());
    }
}
