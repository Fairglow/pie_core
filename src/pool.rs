//! A generic pool allocator for a multi-headed doubly-linked index-list.

use crate::elem::ListElem;
use crate::Index;
use std::fmt;

/// An error type representing failures in list operations.
///
/// These errors typically arise from providing an invalid `Index` to a pool
/// method, such as one that is out of bounds or points to an already-freed element.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexError {
    /// The provided index was `Index::NONE`.
    IndexIsNone,
    /// The provided index exceeds the bounds of the pool's element vector.
    IndexOutOfBounds,
    /// The element at the index is on the pool's free list and cannot be used.
    ElementIsFree,
    /// An attempt was made to operate on the free list's own sentinel node.
    ElementIsFreeSentinel,
    /// A consistency check failed: an element's `prev` link does not point back correctly.
    BrokenPrevLink,
    /// A consistency check failed: an element's `next` link does not point back correctly.
    BrokenNextLink,
}

impl std::error::Error for IndexError {}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IndexIsNone => write!(f, "Index is NONE"),
            Self::IndexOutOfBounds => write!(f, "Index is out of bounds"),
            Self::ElementIsFree => write!(f, "Element is on the free list"),
            Self::ElementIsFreeSentinel => write!(f, "Element is the free list sentinel"),
            Self::BrokenPrevLink => write!(f, "Element's previous link is inconsistent"),
            Self::BrokenNextLink => write!(f, "Element's next link is inconsistent"),
        }
    }
}

/// A pool of `ListElem<T>` nodes that provides memory for multiple `PieList` instances.
///
/// # Rationale
///
/// The `ElemPool` is the cornerstone of this library's design. It acts as a
/// specialized memory allocator. By pre-allocating memory in a `Vec` and managing
/// its own free list, it avoids the performance cost of frequent calls to the
/// global allocator. This makes creating and destroying list elements extremely fast.
///
/// All list elements, regardless of which `PieList` they belong to, are stored
/// contiguously within this single structure, leading to better cache locality
/// during list traversals compared to traditional node-based linked lists.
///
/// Its public API is minimal, as most interactions are performed through `PieList`
/// and `CursorMut` methods, which take the pool as an argument.
#[derive(Clone, Debug)]
pub struct ElemPool<T> {
    /// The contiguous storage for all list elements (nodes).
    elems: Vec<ListElem<T>>,
    /// A count of elements currently in the free list.
    freed: usize,
    /// The number of elements that contain user data (`Some(T)`).
    /// This count excludes all sentinel nodes and free elements.
    used: usize,
}

impl<T> Default for ElemPool<T> {
    /// Creates a new `ElemPool`, initialized with a single sentinel element
    /// for its internal free list.
    fn default() -> Self {
        let sentinel_index = Self::free_sentinel_index();
        let mut sentinel_elem = ListElem::default();
        // The free list sentinel points to itself, indicating an empty free list.
        let _ = sentinel_elem.new_links(sentinel_index, sentinel_index);
        Self {
            elems: vec![sentinel_elem],
            freed: 0,
            used: 0,
        }
    }
}

// --- Public API ---
impl<T> ElemPool<T> {
    /// Creates a new, empty element pool.
    ///
    /// The pool is initialized with a capacity for zero elements but contains
    /// one internal node to act as the sentinel for the free list.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the fixed index for the free list's sentinel node, which is always 0.
    #[inline(always)]
    fn free_sentinel_index() -> Index<T> {
        Index::from(0u32)
    }

    /// Returns the number of elements holding user data in the pool.
    ///
    /// This is a semantic count that excludes sentinel nodes for active lists
    /// and any elements on the free list. It provides a clear measure of how
    /// many items are actually being stored across all lists.
    #[inline]
    pub fn len(&self) -> usize {
        self.used
    }

    /// Returns `true` if the pool contains no user data.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    /// Returns the total number of elements (used, sentinels, or free) that the pool
    /// can hold without reallocating its internal vector. This count excludes the
    /// pool's own free-list sentinel.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.elems.len() - 1
    }

    /// Returns the number of active lists associated with this pool.
    ///
    /// This is calculated by subtracting the number of data-holding elements and
    /// free elements from the total capacity, with the remainder being the
    /// sentinel nodes for active lists.
    #[inline]
    pub fn list_count(&self) -> usize {
        self.capacity() - self.used - self.freed
    }

    /// Checks if a given index points to an element that contains user data.
    ///
    /// Returns `false` if the index is `NONE`, out of bounds, or points to a
    /// free/sentinel element.
    #[inline]
    pub fn contains(&self, index: Index<T>) -> bool {
        index
            .get()
            .and_then(|n| self.elems.get(n))
            .is_some_and(|e| e.is_used())
    }

    /// Performs a detailed validation of an index and its surrounding links.
    ///
    /// This is a powerful debugging tool to verify the structural integrity of a list.
    /// It checks that:
    /// 1. The index is valid and in bounds.
    /// 2. The element at the index contains data.
    /// 3. The `prev` element's `next` link points back to this index.
    /// 4. The `next` element's `prev` link points back to this index.
    ///
    /// # Errors
    /// Returns `Ok(())` on success, or an `IndexError` variant describing the
    /// first validation failure encountered.
    #[inline]
    pub fn validate_index(&self, index: Index<T>) -> Result<(), IndexError> {
        let ndx = index.get().ok_or(IndexError::IndexIsNone)?;
        let elem = self.elems.get(ndx).ok_or(IndexError::IndexOutOfBounds)?;
        if !elem.is_used() {
            return Err(IndexError::ElementIsFree);
        }
        let prev_ndx = elem.prev.get().ok_or(IndexError::BrokenPrevLink)?;
        let prev_elem = self
            .elems
            .get(prev_ndx)
            .ok_or(IndexError::BrokenPrevLink)?;
        if prev_elem.next != index {
            return Err(IndexError::BrokenPrevLink);
        }
        let next_ndx = elem.next.get().ok_or(IndexError::BrokenNextLink)?;
        let next_elem = self
            .elems
            .get(next_ndx)
            .ok_or(IndexError::BrokenNextLink)?;
        if next_elem.prev != index {
            return Err(IndexError::BrokenNextLink);
        }
        Ok(())
    }

    /// Allocates a new index, reusing a free element if available or creating a new one.
    ///
    /// This is the primary method for acquiring a new node from the pool. It
    /// first checks the free list. If the free list is not empty, it unlinks
    /// and returns the first available node. If the free list is empty, it
    /// pushes a new `ListElem` to the end of the internal `Vec`.
    pub(crate) fn index_new(&mut self) -> Result<Index<T>, IndexError> {
        let free_sentinel_ndx = Self::free_sentinel_index();
        let ndx_to_reuse = self.next(free_sentinel_ndx);

        if ndx_to_reuse != free_sentinel_ndx {
            // Free list is not empty, reuse an element.
            self.index_linkout(ndx_to_reuse)?;
            self.freed -= 1;
            Ok(ndx_to_reuse)
        } else {
            // Free list is empty, allocate a new element.
            // This can only fail on OOM, which will panic.
            let ndx = Index::from(self.elems.len());
            let mut new_elem = ListElem::default();
            // A new element is initialized to point to itself.
            let _ = new_elem.new_links(ndx, ndx);
            self.elems.push(new_elem);
            Ok(ndx)
        }
    }

    /// Returns an index to the free list.
    ///
    /// The caller must ensure the element has already been unlinked from any
    /// active list and that its data has been taken. This method links the
    /// element at the given `index` to the front of the free list.
    pub(crate) fn index_del(&mut self, index: Index<T>) -> Result<(), IndexError> {
        let free_sentinel_ndx = Self::free_sentinel_index();
        if index == free_sentinel_ndx {
            return Err(IndexError::ElementIsFreeSentinel);
        }
        // This check ensures the index is valid before we use it.
        self.get(index)?;
        // Link the element into the front of the free list.
        self.index_link_after(index, free_sentinel_ndx)?;
        self.freed += 1;
        Ok(())
    }

    /// Gets an immutable reference to the `ListElem` at the given index.
    #[inline]
    pub(crate) fn get(&self, index: Index<T>) -> Result<&ListElem<T>, IndexError> {
        let n = index.get().ok_or(IndexError::IndexIsNone)?;
        self.elems.get(n).ok_or(IndexError::IndexOutOfBounds)
    }

    /// Gets a mutable reference to the `ListElem` at the given index.
    #[inline]
    pub(crate) fn get_mut(&mut self, index: Index<T>) -> Result<&mut ListElem<T>, IndexError> {
        let n = index.get().ok_or(IndexError::IndexIsNone)?;
        self.elems.get_mut(n).ok_or(IndexError::IndexOutOfBounds)
    }

    /// Gets the `next` index for the element at the given index.
    #[inline]
    pub(crate) fn next(&self, index: Index<T>) -> Index<T> {
        // `unwrap_or_default` returns `Index::NONE` on error.
        self.get(index).map(|i| i.next).unwrap_or_default()
    }

    /// Gets the `prev` index for the element at the given index.
    #[inline]
    pub(crate) fn prev(&self, index: Index<T>) -> Index<T> {
        self.get(index).map(|i| i.prev).unwrap_or_default()
    }

    /// Gets an immutable reference to the data inside the element at the given index.
    #[inline]
    pub(crate) fn data(&self, index: Index<T>) -> Option<&T> {
        self.get(index).ok().and_then(|i| i.data.as_ref())
    }

    /// Gets a mutable reference to the data inside the element at the given index.
    #[inline]
    pub(crate) fn data_mut(&mut self, index: Index<T>) -> Option<&mut T> {
        self.get_mut(index).ok().and_then(|i| i.data.as_mut())
    }

    /// Swaps the data in an element and updates the pool's `used` count accordingly.
    ///
    /// This is the sole method responsible for modifying an element's data, as it
    /// correctly maintains the pool's `used` counter.
    #[inline]
    pub(crate) fn data_swap(&mut self, index: Index<T>, data: Option<T>) -> Option<T> {
        let elem = self.get_mut(index).ok()?;
        let old_data = elem.new_data(data);
        match (old_data.is_some(), elem.data.is_some()) {
            (false, true) => self.used += 1, // Went from None -> Some
            (true, false) => self.used -= 1, // Went from Some -> None
            _ => {}                          // No change in used status
        }
        old_data
    }

    /// Unlinks an element from its current position in a list.
    /// After this operation, the element points to itself.
    #[inline]
    pub(crate) fn index_linkout(&mut self, index: Index<T>) -> Result<(), IndexError> {
        let (prev_ndx, next_ndx) = self.get_mut(index)?.new_links(index, index);
        self.get_mut(prev_ndx)?.new_next(next_ndx);
        self.get_mut(next_ndx)?.new_prev(prev_ndx);
        Ok(())
    }

    /// Links `this` element immediately after the `after` element.
    #[inline]
    pub(crate) fn index_link_after(
        &mut self,
        this: Index<T>,
        after: Index<T>,
    ) -> Result<(), IndexError> {
        let next_ndx = self.get_mut(after)?.new_next(this);
        let _ = self.get_mut(this)?.new_links(after, next_ndx);
        self.get_mut(next_ndx)?.new_prev(this);
        Ok(())
    }

    /// Links `this` element immediately before the `before` element.
    #[inline]
    pub(crate) fn index_link_before(
        &mut self,
        this: Index<T>,
        before: Index<T>,
    ) -> Result<(), IndexError> {
        let prev_ndx = self.get_mut(before)?.new_prev(this);
        let _ = self.get_mut(this)?.new_links(prev_ndx, before);
        self.get_mut(prev_ndx)?.new_next(this);
        Ok(())
    }
}

impl<T> fmt::Display for ElemPool<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "ElemPool used {}/{}, {} free:",
            self.len(),
            self.capacity(),
            self.freed
        )?;
        for (i, elem) in self.elems.iter().enumerate() {
            writeln!(f, "  [{}]: {}", i, elem)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list::PieList;

    // Helper function to create a pool and add some elements for testing.
    fn create_pool_with_elems<T>(count: usize, default_data: T) -> (ElemPool<T>, Vec<Index<T>>)
    where
        T: Clone,
    {
        let mut pool = ElemPool::new();
        let mut indices = Vec::new();
        for _i in 0..count {
            let index = pool.index_new().unwrap();
            // Use data_swap to ensure the 'used' counter is updated correctly.
            pool.data_swap(index, Some(default_data.clone()));
            indices.push(index);
        }
        (pool, indices)
    }

    #[test]
    fn test_pool_creation_and_len() {
        let pool: ElemPool<i32> = ElemPool::new();
        assert_eq!(pool.len(), 0); // Should have 0 used elements
        assert_eq!(pool.capacity(), 0);
        assert!(pool.is_empty());
        assert_eq!(pool.elems.len(), 1); // Internal vec has free sentinel
    }

    #[test]
    fn test_index_new_and_len() {
        let (pool, indices) = create_pool_with_elems(3, 100);
        assert_eq!(pool.len(), 3); // 3 used elements
        assert_eq!(pool.capacity(), 3);
        assert!(!pool.is_empty());
        assert_eq!(indices.len(), 3);
        assert_eq!(indices[0].get(), Some(1));
        assert_eq!(indices[1].get(), Some(2));
        assert_eq!(indices[2].get(), Some(3));
    }

    #[test]
    fn test_del_and_reuse() {
        let (mut pool, indices) = create_pool_with_elems(5, 0);
        assert_eq!(pool.len(), 5);
        assert_eq!(pool.freed, 0);

        let deleted_index = indices[2];
        // To delete an element from the pool, we must first remove its data.
        let _data = pool.data_swap(deleted_index, None);
        // data_swap automatically decrements pool.len()
        assert_eq!(pool.len(), 4);

        // Now we can return the data-less element to the free list
        pool.index_del(deleted_index).unwrap();
        assert_eq!(pool.freed, 1);
        assert!(!pool.contains(deleted_index));
        assert_eq!(pool.next(ElemPool::free_sentinel_index()), deleted_index);

        // Allocate a new element, it should reuse the deleted index.
        let reused_index = pool.index_new().unwrap();
        assert_eq!(reused_index, deleted_index);
        // The pool's used count is still 4 because the new element has no data yet
        assert_eq!(pool.len(), 4);
        assert_eq!(pool.freed, 0);

        // Add data to the reused element, length should go up
        pool.data_swap(reused_index, Some(999));
        assert_eq!(pool.len(), 5);
    }

    #[test]
    fn test_del_errors() {
        let mut pool: ElemPool<i32> = ElemPool::new();
        let index = pool.index_new().unwrap();
        pool.data_swap(index, Some(100));

        // Can't delete the sentinel
        assert_eq!(
            pool.index_del(ElemPool::free_sentinel_index()),
            Err(IndexError::ElementIsFreeSentinel)
        );

        // Deleting the same index twice should ideally fail, but our simplified
        // `index_del` doesn't have a robust double-free check. The `PieList` pop
        // logic prevents this from happening in practice. We test the boundary
        // conditions that `index_del` *does* check.
    }

    #[test]
    fn test_contains() {
        let (pool, indices) = create_pool_with_elems(2, 0);
        assert!(pool.contains(indices[0]));
        assert!(pool.contains(indices[1]));
        let nonexistent_index = Index::from(99 as u32);
        assert!(!pool.contains(nonexistent_index));
        assert!(!pool.contains(Index::NONE));
    }

    #[test]
    fn test_linking_logic() {
        let (mut pool, indices) = create_pool_with_elems(3, 0);
        let i1 = indices[0];
        let i2 = indices[1];
        let i3 = indices[2];

        // Initially, new elements point to themselves
        assert_eq!(pool.next(i1), i1);
        assert_eq!(pool.prev(i1), i1);

        // Link them together: i1 <-> i2 <-> i3 <-> i1 (circular)
        pool.index_link_after(i2, i1).unwrap();
        pool.index_link_after(i3, i2).unwrap();
        // Complete the circle
        pool.get_mut(i1).unwrap().new_prev(i3);
        pool.get_mut(i3).unwrap().new_next(i1);

        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.next(i3), i1);

        assert_eq!(pool.prev(i1), i3);
        assert_eq!(pool.prev(i3), i2);
        assert_eq!(pool.prev(i2), i1);

        // Unlink i2
        pool.index_linkout(i2).unwrap();

        // i2 should now point to itself
        assert_eq!(pool.next(i2), i2);
        assert_eq!(pool.prev(i2), i2);
        // i1 and i3 should now be linked
        assert_eq!(pool.next(i1), i3);
        assert_eq!(pool.prev(i3), i1);

        // Link i2 back in before i3
        pool.index_link_before(i2, i3).unwrap();

        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.prev(i3), i2);
        assert_eq!(pool.prev(i2), i1);
    }

    #[test]
    fn test_validate_index() {
        // Let's create a known structure with a real list
        let (mut pool, _) = create_pool_with_elems(0, 0);
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();
        list.push_back(30, &mut pool).unwrap();

        let i1 = pool.next(list.sentinel);
        let i2 = pool.next(i1);
        let i3 = pool.next(i2);

        // All indices in a valid list should validate correctly.
        assert_eq!(pool.validate_index(i1), Ok(()));
        assert_eq!(pool.validate_index(i2), Ok(()));
        assert_eq!(pool.validate_index(i3), Ok(()));

        // Test specific error cases
        assert_eq!(
            pool.validate_index(Index::NONE),
            Err(IndexError::IndexIsNone)
        );
        assert_eq!(
            pool.validate_index(Index::from(99_u32)),
            Err(IndexError::IndexOutOfBounds)
        );

        // Manually free an element to test ElementIsFree error
        pool.data_swap(i2, None);
        assert_eq!(pool.validate_index(i2), Err(IndexError::ElementIsFree));
        pool.data_swap(i2, Some(20)); // Restore for next test

        // Manually break a link to test for inconsistency
        // i1's next now points to i3, but i3's prev still points to i2
        pool.get_mut(i1).unwrap().next = i3;

        // i2 thinks its prev is i1, but i1's next is i3. So i2's prev link is broken.
        assert_eq!(pool.validate_index(i2), Err(IndexError::BrokenPrevLink));
        // i1 thinks its next is i3, but i3's prev is i2. So i1's next link is broken.
        assert_eq!(pool.validate_index(i1), Err(IndexError::BrokenNextLink));
    }
}
