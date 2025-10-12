//! A mutable cursor for navigating and modifying a `PieList`.

use crate::index::Index;
use crate::list::PieList;
use crate::pool::{ElemPool, IndexError};

/// A cursor with mutable access to a `PieList`.
///
/// A `CursorMut` provides a flexible way to inspect and modify a `PieList`
/// at a specific location. It holds a mutable reference to its `PieList`,
/// which means that while the cursor exists, the list cannot be modified by
/// any other means.
///
/// # Rationale
///
/// The cursor pattern is essential for implementing complex list operations
/// efficiently, such as `split_before` or `splice_before`. It avoids the need
/// for repeated index-based lookups from the beginning or end of the list.
///
/// A key feature of this design is that the `ElemPool` is only borrowed by
/// individual methods of the cursor, not by the `CursorMut` struct itself. This
/// allows multiple independent cursors to exist and operate on different lists
/// within the same pool concurrently.
pub struct CursorMut<'a, T> {
    /// A mutable reference to the list being manipulated.
    list: &'a mut PieList<T>,
    /// The `Index` of the element the cursor is currently pointing to.
    /// This can be the sentinel, indicating an "off-list" position.
    current: Index<T>,
    /// The logical, 0-based index of the `current` element.
    /// If `current` is the sentinel, this value represents the position
    /// where a new element would be (e.g., `list.len()` if at the end).
    logical_index: usize,
}

impl<'a, T> CursorMut<'a, T> {
    /// Creates a new cursor. This is intended for internal use by `PieList`.
    ///
    /// The cursor is initialized to point at a specific `current` index, which
    /// is assumed to correspond to the given `logical_index`.
    pub(crate) fn new(list: &'a mut PieList<T>, current: Index<T>, logical_index: usize) -> Self {
        Self {
            list,
            current,
            logical_index,
        }
    }

    /// Returns the logical index of the cursor's current position.
    ///
    /// The index is 0-based. If the cursor is pointing "past the end" of the
    /// list (i.e., at the sentinel node), this method returns `None`.
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(20, &mut pool).unwrap();
    /// let mut cursor = list.cursor_mut(&mut pool);
    /// assert_eq!(cursor.index(), Some(0));
    ///
    /// cursor.move_next(&mut pool);
    /// assert_eq!(cursor.index(), Some(1));
    ///
    /// cursor.move_next(&mut pool);
    /// assert_eq!(cursor.index(), None); // Past the end
    /// ```
    pub fn index(&self) -> Option<usize> {
        if self.current == self.list.sentinel {
            None
        } else {
            Some(self.logical_index)
        }
    }

    /// Provides a mutable reference to the element at the cursor's current position.
    ///
    /// If the cursor is not pointing at a valid element (e.g., it's past the end),
    /// this returns `None`.
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// let mut cursor = list.cursor_mut(&mut pool);
    ///
    /// if let Some(value) = cursor.peek_mut(&mut pool) {
    ///     *value = 99;
    /// }
    ///
    /// assert_eq!(list.front(&pool), Some(&99));
    /// ```
    pub fn peek_mut<'p>(&mut self, pool: &'p mut ElemPool<T>) -> Option<&'p mut T> {
        if self.current == self.list.sentinel {
            None
        } else {
            pool.data_mut(self.current)
        }
    }

    /// Moves the cursor to the next element in the list.
    ///
    /// If the cursor is already past the end of the list, this has no effect.
    pub fn move_next(&mut self, pool: &mut ElemPool<T>) {
        if self.current == self.list.sentinel {
            return;
        }
        self.current = pool.next(self.current);
        // If we moved to the sentinel, the logical index is now the list's length.
        // Otherwise, it's one greater than before.
        if self.current == self.list.sentinel {
            self.logical_index = self.list.len();
        } else {
            self.logical_index += 1;
        }
    }

    /// Moves the cursor to the previous element in the list.
    ///
    /// If the cursor is at the beginning of the list, it will move to a position
    /// "before the start" (the sentinel), and its logical index will become 0.
    /// If it is already "past the end", this method has no effect.
    pub fn move_prev(&mut self, pool: &mut ElemPool<T>) {
        if self.current == self.list.sentinel {
            // This case handles being "past the end". To move to the actual
            // last element, one should use `cursor_mut_at(len - 1)`.
            // Here, we do nothing to prevent unexpected wrapping.
            return;
        }
        let prev_element = pool.prev(self.current);
        self.current = prev_element;
        // If we moved to the sentinel from the first element, the logical index becomes 0.
        // Otherwise, it's one less than before.
        if self.current == self.list.sentinel {
            self.logical_index = 0;
        } else {
            self.logical_index -= 1;
        }
    }

    /// Inserts a new element *before* the cursor's current position.
    ///
    /// After insertion, the cursor will point to the newly inserted element.
    ///
    /// # Rationale
    /// This behavior is consistent with Rust's standard library `LinkedList::CursorMut`
    /// and is useful for building up a sequence at a specific point.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns `IndexError` if the pool fails to allocate a new element.
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(30, &mut pool).unwrap();
    /// // Cursor is at index 1 (value 30)
    /// let mut cursor = list.cursor_mut_at(1, &mut pool).unwrap();
    ///
    /// cursor.insert_before(20, &mut pool).unwrap();
    ///
    /// // Cursor is now at the new element '20', which is at index 1
    /// assert_eq!(cursor.index(), Some(1));
    /// assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 20);
    ///
    /// let vec: Vec<_> = list.iter(&pool).copied().collect();
    /// assert_eq!(vec, vec![10, 20, 30]);
    /// ```
    pub fn insert_before(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_before(new_idx, self.current)?;
        self.list.len += 1;
        // The new element is now at the cursor's position.
        self.current = new_idx;
        // The logical index remains the same, as we inserted before it.
        Ok(())
    }

    /// Inserts a new element *after* the cursor's current position.
    ///
    /// The cursor's position does not change.
    ///
    /// # Rationale
    /// This is useful for appending elements after a specific point without
    /// losing the current position.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns `IndexError` if the pool fails to allocate a new element.
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(30, &mut pool).unwrap();
    /// // Cursor is at index 0 (value 10)
    /// let mut cursor = list.cursor_mut(&mut pool);
    ///
    /// cursor.insert_after(20, &mut pool).unwrap();
    ///
    /// // Cursor is still at index 0 (value 10)
    /// assert_eq!(cursor.index(), Some(0));
    /// assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 10);
    ///
    /// let vec: Vec<_> = list.iter(&pool).copied().collect();
    /// assert_eq!(vec, vec![10, 20, 30]);
    /// ```
    pub fn insert_after(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_after(new_idx, self.current)?;
        self.list.len += 1;
        // The cursor's position and logical index do not change.
        Ok(())
    }

    /// Removes the element at the cursor's current position and returns its data.
    ///
    /// After removal, the cursor moves to the next element. If the removed element
    /// was the last one, the cursor moves to the "past the end" position.
    ///
    /// # Rationale
    /// Moving to the next element is a common pattern when draining or filtering
    /// a list, making it convenient.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Returns
    /// `Some(T)` if an element was removed, `None` if the cursor was not on a
    /// valid element.
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(20, &mut pool).unwrap();
    /// # list.push_back(30, &mut pool).unwrap();
    /// // Cursor at index 1 (value 20)
    /// let mut cursor = list.cursor_mut_at(1, &mut pool).unwrap();
    ///
    /// let removed = cursor.remove_current(&mut pool);
    /// assert_eq!(removed, Some(20));
    ///
    /// // Cursor is now at the next element (30), which is now at index 1
    /// assert_eq!(cursor.index(), Some(1));
    /// assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 30);
    /// ```
    pub fn remove_current(&mut self, pool: &mut ElemPool<T>) -> Option<T> {
        if self.current == self.list.sentinel {
            return None;
        }
        let old_current = self.current;
        // Move to the next element before modifying links.
        self.current = pool.next(old_current);
        // The logical index stays the same because the subsequent elements shift left.

        pool.index_linkout(old_current).unwrap();
        self.list.len -= 1;
        let data = pool.data_swap(old_current, None);
        pool.index_del(old_current).unwrap();

        data
    }

    /// Splits the list into two at the cursor's current position.
    ///
    /// The original list (`self`) is modified to contain all elements from the
    /// current position onwards. A new `PieList` is returned containing all
    /// elements *before* the current position.
    ///
    /// After the split, the cursor remains on the same element, which is now the
    /// first element of the original list. Its logical index becomes 0.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(20, &mut pool).unwrap();
    /// # list.push_back(30, &mut pool).unwrap();
    /// # list.push_back(40, &mut pool).unwrap();
    ///
    /// let front_list;
    /// {
    ///     // Cursor at index 2 (value 30)
    ///     let mut cursor = list.cursor_mut_at(2, &mut pool).unwrap();
    ///
    ///     // Split the list. `front_list` will get elements {10, 20}.
    ///     front_list = cursor.split_before(&mut pool).unwrap();
    ///
    ///     // The cursor is now at the beginning of the modified original list.
    ///     assert_eq!(cursor.index(), Some(0));
    ///     assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 30);
    /// } // cursor is dropped here, releasing the mutable borrow on `list`.
    ///
    /// // The original list now contains {30, 40}.
    /// assert_eq!(list.len(), 2);
    /// assert_eq!(list.front(&pool), Some(&30));
    ///
    /// // The new list contains the elements before the split point.
    /// assert_eq!(front_list.len(), 2);
    /// assert_eq!(front_list.front(&pool), Some(&10));
    /// ```
    pub fn split_before(&mut self, pool: &mut ElemPool<T>) -> Result<PieList<T>, IndexError> {
        let original_len = self.list.len();
        let split_point_idx = self.logical_index;
        if split_point_idx == 0 {
            // Nothing to split off, return a new empty list.
            return Ok(PieList::new(pool));
        }

        let mut new_list = PieList::new(pool);
        // Identify the four key "boundary" nodes.
        let original_front = pool.next(self.list.sentinel);
        let element_before_split = pool.prev(self.current);
        let original_back = pool.prev(self.list.sentinel);

        // --- Rewire links ---

        // 1. Form the new list:
        //    (new_sentinel) <-> (original_front) <-> ... <-> (element_before_split) <-> (new_sentinel)
        pool.get_mut(new_list.sentinel)?
            .new_links(element_before_split, original_front);
        pool.get_mut(original_front)?.new_prev(new_list.sentinel);
        pool.get_mut(element_before_split)?
            .new_next(new_list.sentinel);

        // 2. Form the original, now-shortened list:
        //    (old_sentinel) <-> (self.current) <-> ... <-> (original_back) <-> (old_sentinel)
        pool.get_mut(self.list.sentinel)?
            .new_links(original_back, self.current);
        pool.get_mut(self.current)?.new_prev(self.list.sentinel);
        // The original back might not exist if we split off everything, but `self.current`
        // would become the new back and its `next` points to sentinel, so this works.
        // We can safely unwrap because `original_back` must be a valid node if `split_point_idx > 0`.
        pool.get_mut(original_back)
            .unwrap()
            .new_next(self.list.sentinel);

        // --- Update lengths and cursor state ---
        self.list.len = original_len - split_point_idx;
        new_list.len = split_point_idx;
        self.logical_index = 0; // The cursor is now at the start of the modified list.
        Ok(new_list)
    }

    /// Moves all elements from `other` into `self`'s list, inserting them
    /// just before the cursor's current position.
    ///
    /// After the operation, `other` is left empty. The cursor's position does
    /// not change, but its logical index is updated to reflect the newly
    /// inserted elements.
    ///
    /// # Rationale
    /// This is a highly efficient way to merge two lists without iterating
    /// or reallocating elements. It only requires a few pointer updates.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Example
    /// ```
    /// # use pielist::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// let mut list1 = PieList::new(&mut pool);
    /// list1.push_back(10, &mut pool).unwrap();
    /// list1.push_back(40, &mut pool).unwrap();
    ///
    /// let mut list2 = PieList::new(&mut pool);
    /// list2.push_back(20, &mut pool).unwrap();
    /// list2.push_back(30, &mut pool).unwrap();
    ///
    /// {
    ///     // Cursor at index 1 (value 40) in list1
    ///     let mut cursor = list1.cursor_mut_at(1, &mut pool).unwrap();
    ///     cursor.splice_before(&mut list2, &mut pool).unwrap();
    ///
    ///     // Cursor still points to 40, but its logical index is now 3
    ///     assert_eq!(cursor.index(), Some(3));
    /// } // cursor is dropped here, releasing the mutable borrow on `list1`.
    ///
    /// assert!(list2.is_empty());
    /// assert_eq!(list1.len(), 4);
    ///
    /// let vec: Vec<_> = list1.iter(&pool).copied().collect();
    /// assert_eq!(vec, vec![10, 20, 30, 40]);
    /// ```
    pub fn splice_before(
        &mut self,
        other: &mut PieList<T>,
        pool: &mut ElemPool<T>,
    ) -> Result<(), IndexError> {
        if other.is_empty() {
            return Ok(());
        }

        // Identify the boundary nodes.
        let element_before_cursor = pool.prev(self.current);
        let other_first = pool.next(other.sentinel);
        let other_last = pool.prev(other.sentinel);

        // Rewire the links to merge `other` into `self`.
        // (element_before_cursor) <-> (other_first)
        pool.get_mut(element_before_cursor)?.new_next(other_first);
        pool.get_mut(other_first)?.new_prev(element_before_cursor);
        // (other_last) <-> (self.current)
        pool.get_mut(self.current)?.new_prev(other_last);
        pool.get_mut(other_last)?.new_next(self.current);

        // Update lengths and cursor index.
        self.list.len += other.len;
        self.logical_index += other.len;
        other.len = 0;

        // Reset the now-empty `other` list's sentinel to point to itself.
        let other_sentinel = other.sentinel;
        pool.get_mut(other_sentinel)?
            .new_links(other_sentinel, other_sentinel);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::list::PieList;
    use crate::pool::{ElemPool, IndexError};

    fn list_with_items(pool: &mut ElemPool<i32>, items: &[i32]) -> PieList<i32> {
        let mut list = PieList::new(pool);
        for &item in items {
            list.push_back(item, pool).unwrap();
        }
        list
    }

    #[test]
    fn test_cursor_peek_and_index() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 20]);
        let mut cursor = list.cursor_mut(&mut pool);

        assert_eq!(cursor.index(), Some(0));
        assert_eq!(cursor.peek_mut(&mut pool), Some(&mut 10));
    }

    #[test]
    fn test_cursor_navigation() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 20, 30]);
        let mut cursor = list.cursor_mut(&mut pool);

        cursor.move_next(&mut pool);
        assert_eq!(cursor.index(), Some(1));
        assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 20);

        cursor.move_next(&mut pool);
        assert_eq!(cursor.index(), Some(2));
        assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 30);

        cursor.move_next(&mut pool);
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek_mut(&mut pool), None);

        // move_prev from the end does nothing
        cursor.move_prev(&mut pool);
        assert_eq!(cursor.index(), None);

        // Test move_prev from a valid position
        let mut cursor2 = list.cursor_mut_at(2, &mut pool).unwrap();
        cursor2.move_prev(&mut pool);
        assert_eq!(cursor2.index(), Some(1));
        assert_eq!(*cursor2.peek_mut(&mut pool).unwrap(), 20);

        cursor2.move_prev(&mut pool);
        cursor2.move_prev(&mut pool);
        // Cursor is now "before the beginning"
        assert_eq!(cursor2.index(), None);
        // Its logical index is 0, ready for an insert_before at the front
        assert_eq!(cursor2.logical_index, 0);
    }

    #[test]
    fn test_cursor_mut_at_out_of_bounds() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 20]);

        assert!(matches!(
            list.cursor_mut_at(2, &mut pool),
            Err(IndexError::IndexOutOfBounds)
        ));
        assert!(matches!(
            list.cursor_mut_at(99, &mut pool),
            Err(IndexError::IndexOutOfBounds)
        ));
    }

    #[test]
    fn test_cursor_insert_before() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 30]);

        // Use a scope to manage the cursor's lifetime.
        {
            let mut cursor = list.cursor_mut_at(1, &mut pool).unwrap();
            cursor.insert_before(20, &mut pool).unwrap();
            assert_eq!(cursor.list.len(), 3);
            assert_eq!(cursor.index(), Some(1));
            assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 20);
        } // Cursor is dropped here, its borrow on `list` is released.

        assert_eq!(list.pop_front(&mut pool), Some(10));
        assert_eq!(list.pop_front(&mut pool), Some(20));
        assert_eq!(list.pop_front(&mut pool), Some(30));
    }

    #[test]
    fn test_cursor_insert_after() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 30]);

        {
            let mut cursor = list.cursor_mut(&mut pool);
            cursor.insert_after(20, &mut pool).unwrap();
            assert_eq!(cursor.list.len(), 3);
            assert_eq!(cursor.index(), Some(0));
            assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 10);
        }

        assert_eq!(list.pop_front(&mut pool), Some(10));
        assert_eq!(list.pop_front(&mut pool), Some(20));
        assert_eq!(list.pop_front(&mut pool), Some(30));
    }

    #[test]
    fn test_cursor_remove_current() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 20, 30]);

        let mut cursor = list.cursor_mut_at(1, &mut pool).unwrap();
        assert_eq!(cursor.remove_current(&mut pool), Some(20));
        assert_eq!(cursor.list.len(), 2);
        assert_eq!(cursor.index(), Some(1));
        assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 30);

        assert_eq!(cursor.remove_current(&mut pool), Some(30));
        assert_eq!(cursor.list.len(), 1);
        assert_eq!(cursor.index(), None);
    }

    #[test]
    fn test_split_before() {
        let mut pool = ElemPool::new();
        let mut list1 = list_with_items(&mut pool, &[1, 2, 3, 4, 5]);
        let list2; // Declare list2 outside the scope.

        {
            let mut cursor = list1.cursor_mut_at(2, &mut pool).unwrap();
            list2 = cursor.split_before(&mut pool).unwrap();
            assert_eq!(cursor.list.len(), 3);
            assert_eq!(cursor.index(), Some(0));
            assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 3);
        } // Cursor is dropped here, releasing borrows.

        // Now it's safe to check the final state of both lists.
        assert_eq!(list2.len(), 2);
        let vec2: Vec<_> = list2.iter(&pool).copied().collect();
        assert_eq!(vec2, vec![1, 2]);

        assert_eq!(list1.len(), 3);
        let vec1: Vec<_> = list1.iter(&pool).copied().collect();
        assert_eq!(vec1, vec![3, 4, 5]);
    }

    #[test]
    fn test_splice_before() {
        let mut pool = ElemPool::new();
        let mut list1 = list_with_items(&mut pool, &[10, 40]);
        let mut list2 = list_with_items(&mut pool, &[20, 30]);

        {
            let mut cursor = list1.cursor_mut_at(1, &mut pool).unwrap(); // Cursor at 40
            cursor.splice_before(&mut list2, &mut pool).unwrap();

            // Check state while cursor is alive
            assert_eq!(cursor.list.len(), 4);
            assert!(list2.is_empty()); // Spliced list should be empty
            assert_eq!(cursor.index(), Some(3)); // Index moved from 1 to 1+2=3
            assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 40);
        }

        // Check final state of list1
        let vec: Vec<_> = list1.iter(&pool).copied().collect();
        assert_eq!(vec, vec![10, 20, 30, 40]);
    }

    #[test]
    fn test_independent_cursors_on_multiple_lists() {
        let mut pool = ElemPool::new();
        let mut list1 = list_with_items(&mut pool, &[10, 20, 30]);
        let mut list2 = list_with_items(&mut pool, &[100, 200, 300]);

        // Create cursors for both lists.
        let mut cursor1 = list1.cursor_mut_at(1, &mut pool).unwrap(); // At 20
        let mut cursor2 = list2.cursor_mut_at(1, &mut pool).unwrap(); // At 200

        // 1. Mutate list1 via its cursor.
        cursor1.insert_before(15, &mut pool).unwrap(); // list1: 10, 15, 20, 30
        assert_eq!(*cursor1.peek_mut(&mut pool).unwrap(), 15);

        // 2. Mutate list2 via its cursor.
        *cursor2.peek_mut(&mut pool).unwrap() = 250; // list2: 100, 250, 300
        assert_eq!(*cursor2.peek_mut(&mut pool).unwrap(), 250);

        // 3. Mutating list1 directly is now impossible because cursor1 holds a borrow.
        // Instead, we can access its length through the cursor.
        assert_eq!(cursor1.list.len(), 4);

        // Release the borrow on list2 by dropping its cursor
        drop(cursor2);

        // 4. Now it's safe to mutate list2 directly, even while cursor1 (on list1) still exists.
        assert_eq!(list2.pop_front(&mut pool), Some(100)); // list2 is now {250, 300}

        // Final verification after cursors are out of scope.
        drop(cursor1);

        assert_eq!(list1.len(), 4);
        assert_eq!(list2.len(), 2);
        assert_eq!(pool.len(), 6); // 4 + 2 = 6 used elements

        let vec1: Vec<_> = list1.iter(&pool).copied().collect();
        assert_eq!(vec1, vec![10, 15, 20, 30]);

        let vec2: Vec<_> = list2.iter(&pool).copied().collect();
        assert_eq!(vec2, vec![250, 300]);
    }
}
