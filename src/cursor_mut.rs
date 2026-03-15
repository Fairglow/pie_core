//! A mutable cursor for navigating and modifying a `PieList`.

use crate::{ElemPool, Index, IndexError, PieList};

/// A cursor with mutable access to a `PieList`.
///
/// A `CursorMut` provides a flexible way to inspect and modify a `PieList`
/// at a specific location. It holds a mutable reference to its `PieList`,
/// which means that while the cursor exists, the list cannot be modified by
/// any other means.
///
/// The cursor has two special states:
/// 1. "Before Start": Positioned before the first element (`index()` returns `None`).
/// 2. "After End": Positioned after the last element (`index()` returns `None`).
///
/// # Rationale
///
/// The cursor pattern is essential for implementing complex list operations
/// efficiently, such as `split_before` or `splice_before`. It avoids the need
/// for repeated index-based lookups from the beginning or end of the list.
///
/// # Example
/// ```
/// use pie_core::{ElemPool, PieList};
/// let mut pool = ElemPool::new();
/// let mut list = PieList::new(&mut pool);
/// list.push_back(10, &mut pool).unwrap();
/// list.push_back(20, &mut pool).unwrap();
///
/// let mut cursor = list.cursor_mut(&mut pool); // Starts at index 0 (10)
///
/// // Move forward
/// cursor.move_next(&pool);
/// if let Some(val) = cursor.peek_mut(&mut pool) {
///     *val = 99;
/// }
/// assert_eq!(cursor.peek(&pool), Some(&99));
///
/// // Move backward (supports wrapping from End to Tail, etc)
/// cursor.move_next(&pool); // Now After End
/// cursor.move_prev(&pool); // Back to Tail (99)
/// assert_eq!(cursor.peek(&pool), Some(&99));
///
/// list.clear(&mut pool);
/// ```
#[derive(Debug)]
#[must_use]
pub struct CursorMut<'a, T> {
    /// A mutable reference to the list being manipulated.
    pub(crate) list: &'a mut PieList<T>,
    /// The `Index` of the element the cursor is currently pointing to.
    /// This can be the sentinel, indicating an "off-list" position.
    pub(crate) current: Index<T>,
    /// The logical, 0-based index of the `current` element.
    /// `0` to `len-1` are valid elements.
    /// `len` indicates "After End".
    /// `usize::MAX` indicates "Before Start".
    pub(crate) logical_index: usize,
}

impl<'a, T> CursorMut<'a, T> {
    /// Creates a new cursor. This is intended for internal use by `PieList`.
    pub(crate) fn new(list: &'a mut PieList<T>, current: Index<T>, logical_index: usize) -> Self {
        Self {
            list,
            current,
            logical_index,
        }
    }

    /// Returns the logical index of the cursor's current position.
    ///
    /// The index is 0-based. If the cursor is pointing "Before Start" or "After End",
    /// this method returns `None`.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// let mut cursor = list.cursor_mut(&mut pool);
    /// assert_eq!(cursor.index(), Some(0));
    ///
    /// cursor.move_next(&mut pool);
    /// assert_eq!(cursor.index(), None); // Past the end
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn index(&self) -> Option<usize> {
        if self.current == self.list.sentinel {
            None
        } else {
            Some(self.logical_index)
        }
    }

    /// Provides a reference to the element at the cursor's current position.
    ///
    /// Returns `None` if the cursor is "Before Start" or "After End".
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// let mut cursor = list.cursor_mut(&mut pool);
    ///
    /// assert_eq!(cursor.peek(&pool), Some(&10));
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn peek<'p>(&self, pool: &'p ElemPool<T>) -> Option<&'p T> {
        if self.current == self.list.sentinel {
            None
        } else {
            pool.data(self.current)
        }
    }

    /// Provides a mutable reference to the element at the cursor's current position.
    ///
    /// Returns `None` if the cursor is "Before Start" or "After End".
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
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
    /// # list.clear(&mut pool);
    /// ```
    #[inline]
    pub fn peek_mut<'p>(&mut self, pool: &'p mut ElemPool<T>) -> Option<&'p mut T> {
        if self.current == self.list.sentinel {
            None
        } else {
            pool.data_mut(self.current)
        }
    }

    /// Moves the cursor to the next element in the list.
    ///
    /// - If "Before Start": Moves to the first element (Head).
    /// - If at the last element: Moves to "After End".
    /// - If "After End": Stays at "After End" (no-op).
    #[inline]
    pub fn move_next(&mut self, pool: &ElemPool<T>) {
        let sentinel_slot = self.list.sentinel.slot as usize;
        let current_slot = self.current.slot as usize;

        // Case 1: Already at "After End"
        if current_slot == sentinel_slot && self.logical_index == self.list.len() {
            return;
        }

        // Case 2: At "Before Start" -> Move to Head
        if current_slot == sentinel_slot && self.logical_index == usize::MAX {
            let next_slot = pool.next_slot(sentinel_slot).unwrap();
            self.current = pool.index_from_slot(next_slot);
            self.logical_index = 0;
            return;
        }

        // Case 3: Valid element - use slot-based navigation
        let next_slot = pool.next_slot(current_slot).unwrap();
        self.current = pool.index_from_slot(next_slot);
        if next_slot == sentinel_slot {
            self.logical_index = self.list.len();
        } else {
            self.logical_index += 1;
        }
    }

    /// Moves the cursor to the previous element in the list.
    ///
    /// - If "After End": Moves to the last element (Tail).
    /// - If at the first element: Moves to "Before Start".
    /// - If "Before Start": Stays at "Before Start" (no-op).
    #[inline]
    pub fn move_prev(&mut self, pool: &ElemPool<T>) {
        let sentinel_slot = self.list.sentinel.slot as usize;
        let current_slot = self.current.slot as usize;

        // Case 1: Already at "Before Start"
        if current_slot == sentinel_slot && self.logical_index == usize::MAX {
            return;
        }

        // Case 2: At "After End" -> Move to Tail
        if current_slot == sentinel_slot && self.logical_index == self.list.len() {
            if self.list.is_empty() {
                self.logical_index = usize::MAX; // Empty list: After End -> Before Start
                return;
            }
            let prev_slot = pool.prev_slot(sentinel_slot).unwrap();
            self.current = pool.index_from_slot(prev_slot);
            self.logical_index = self.list.len() - 1;
            return;
        }

        // Case 3: Valid element - use slot-based navigation
        let prev_slot = pool.prev_slot(current_slot).unwrap();
        self.current = pool.index_from_slot(prev_slot);
        if prev_slot == sentinel_slot {
            self.logical_index = usize::MAX; // Moved before first element
        } else {
            self.logical_index -= 1;
        }
    }

    /// Moves the cursor to the first element of the list.
    ///
    /// If the list is empty, results in "After End" (index 0, len 0).
    pub fn move_to_front(&mut self, pool: &ElemPool<T>) {
        let sentinel_slot = self.list.sentinel.slot as usize;
        let next_slot = pool.next_slot(sentinel_slot).unwrap();
        self.current = pool.index_from_slot(next_slot);
        self.logical_index = 0;
    }

    /// Moves the cursor to the last element of the list.
    ///
    /// If the list is empty, results in "After End" (index 0, len 0).
    pub fn move_to_back(&mut self, pool: &ElemPool<T>) {
        let sentinel_slot = self.list.sentinel.slot as usize;
        let prev_slot = pool.prev_slot(sentinel_slot).unwrap();
        self.current = pool.index_from_slot(prev_slot);
        if self.list.is_empty() {
            self.logical_index = 0;
        } else {
            self.logical_index = self.list.len() - 1;
        }
    }

    /// Inserts a new element *before* the cursor's current position.
    ///
    /// - If valid position: Inserts before. Cursor points to *new* element.
    /// - If "After End": Appends to list. Cursor points to *new* element (the new tail).
    /// - If "Before Start": Prepends to list. Cursor points to *new* element (the new head).
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns `IndexError` if the pool fails to allocate a new element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(30, &mut pool).unwrap();
    /// // Cursor is at index 0 (value 30)
    /// let mut cursor = list.cursor_mut(&mut pool);
    ///
    /// cursor.insert_before(20, &mut pool).unwrap();
    ///
    /// // Cursor is now at the new element '20', which is at index 0
    /// assert_eq!(cursor.index(), Some(0));
    /// assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 20);
    ///
    /// let vec: Vec<_> = list.iter(&pool).copied().collect();
    /// assert_eq!(vec, vec![20, 30]);
    /// # list.clear(&mut pool);
    /// ```
    pub fn insert_before(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new_with_data(data)?;

        if self.current == self.list.sentinel && self.logical_index == usize::MAX {
            // Special Case: "Before Start" -> Prepend
            // We link AFTER the sentinel, effectively prepending.
            pool.index_link_after(new_idx, self.list.sentinel)?;
            self.current = new_idx;
            self.logical_index = 0;
        } else {
            // Standard Case (including After End, which links before sentinel)
            pool.index_link_before(new_idx, self.current)?;
            self.current = new_idx;
            // Logical index remains the same (we displaced the old element)
        }
        self.list.len += 1;
        Ok(())
    }

    /// Inserts a new element *after* the cursor's current position.
    ///
    /// - If valid position: Inserts after. Cursor stays on *current* element.
    /// - If "After End": No-op logic (Appends? Undefined?). Currently acts like append.
    /// - If "Before Start": Prepends to list. Cursor stays "Before Start".
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
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
    /// assert_eq!(vec, vec![10, 20]);
    /// # list.clear(&mut pool);
    /// ```
    pub fn insert_after(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new_with_data(data)?;

        if self.current == self.list.sentinel && self.logical_index == usize::MAX {
            // Insert after "Before Start" -> Prepend
            pool.index_link_after(new_idx, self.list.sentinel)?;
            // Cursor stays "Before Start" (usize::MAX)
        } else {
            pool.index_link_after(new_idx, self.current)?;
            // Cursor stays put
        }
        self.list.len += 1;
        Ok(())
    }

    /// Removes the element at the cursor's current position and returns its data.
    ///
    /// After removal, the cursor moves to the *next* element. If the removed element
    /// was the last one, the cursor moves to the "After End" position.
    ///
    /// # Returns
    /// `Some(T)` if an element was removed, `None` if the cursor was not on a
    /// valid element (i.e. Before Start or After End).
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(20, &mut pool).unwrap();
    /// // Cursor at index 0 (value 10)
    /// let mut cursor = list.cursor_mut(&mut pool);
    ///
    /// let removed = cursor.remove_current(&mut pool);
    /// assert_eq!(removed, Some(10));
    ///
    /// // Cursor is now at the next element (20), which shifted to index 0
    /// assert_eq!(cursor.index(), Some(0));
    /// assert_eq!(*cursor.peek_mut(&mut pool).unwrap(), 20);
    /// # list.clear(&mut pool);
    /// ```
    pub fn remove_current(&mut self, pool: &mut ElemPool<T>) -> Option<T> {
        if self.current == self.list.sentinel {
            return None;
        }
        let old_current = self.current;
        // Move to the next element before modifying links.
        self.current = pool.next(old_current);

        pool.index_linkout(old_current).expect("cursor element is linked in list");
        self.list.len -= 1;
        let data = pool.data_swap(old_current, None);
        pool.index_del(old_current).expect("valid node for deletion");

        // Update logical index.
        // If we hit the sentinel (After End), index becomes len.
        if self.current == self.list.sentinel {
            self.logical_index = self.list.len();
        }
        // Otherwise, logical_index stays same because subsequent items shifted left.

        data
    }

    /// Splits the list into two at the cursor's current position.
    ///
    /// The original list (`self`) is modified to contain all elements from the
    /// current position onwards. A new `PieList` is returned containing all
    /// elements *before* the current position.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// # list.push_back(20, &mut pool).unwrap();
    ///
    /// let mut front_list;
    /// {
    ///     // Cursor at index 1 (value 20)
    ///     let mut cursor = list.cursor_mut_at(1, &mut pool).unwrap();
    ///
    ///     // Split. front_list gets {10}. Original keeps {20}.
    ///     front_list = cursor.split_before(&mut pool).unwrap();
    ///
    ///     // Cursor points to 20, which is now index 0 of the truncated list
    ///     assert_eq!(cursor.index(), Some(0));
    /// }
    ///
    /// assert_eq!(list.len(), 1);
    /// assert_eq!(front_list.len(), 1);
    /// # front_list.clear(&mut pool);
    /// # list.clear(&mut pool);
    /// ```
    pub fn split_before(&mut self, pool: &mut ElemPool<T>) -> Result<PieList<T>, IndexError> {
        if self.current == self.list.sentinel && self.logical_index == usize::MAX {
            // Split before "Before Start" -> New list empty
            return Ok(PieList::new(pool));
        }

        let split_len = self.logical_index;
        // self.list becomes the right half (from cursor onwards).
        // returned list gets the front elements.
        let new_list = self.list
            .split_off(self.current, split_len, pool)?;
        // The cursor now points to the same element, which is the new head of the modified list.
        self.logical_index = 0;
        Ok(new_list)
    }

    /// Moves all elements from `other` into `self`'s list, inserting them
    /// just before the cursor's current position.
    ///
    /// After the operation, `other` is left empty.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// let mut list1 = PieList::new(&mut pool);
    /// list1.push_back(30, &mut pool).unwrap();
    ///
    /// let mut list2 = PieList::new(&mut pool);
    /// list2.push_back(10, &mut pool).unwrap();
    /// list2.push_back(20, &mut pool).unwrap();
    ///
    /// {
    ///     // Cursor at 30
    ///     let mut cursor = list1.cursor_mut(&mut pool);
    ///     cursor.splice_before(&mut list2, &mut pool).unwrap();
    ///
    ///     // list1: {10, 20, 30}. Cursor still at 30, now index 2.
    ///     assert_eq!(cursor.index(), Some(2));
    /// }
    ///
    /// assert_eq!(list1.len(), 3);
    /// assert!(list2.is_empty());
    /// # list2.clear(&mut pool);
    /// # list1.clear(&mut pool);
    /// ```
    pub fn splice_before(
        &mut self,
        other: &mut PieList<T>,
        pool: &mut ElemPool<T>,
    ) -> Result<(), IndexError> {
        let other_len = other.len();
        if other_len == 0 {
            return Ok(());
        }

        if self.current == self.list.sentinel && self.logical_index == usize::MAX {
             // Splice before "Before Start" -> Prepend
             self.list.prepend(other, pool)?;
             // Cursor stays "Before Start"
        } else {
            self.list.splice(self.current, other, pool)?;
            self.logical_index += other_len;
        }

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
    fn test_cursor_nav_edges() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 20]);
        let mut cursor = list.cursor_mut(&mut pool); // At 10

        // Move Prev -> Before Start
        cursor.move_prev(&pool);
        assert_eq!(cursor.index(), None);
        // Move Next -> At 10
        cursor.move_next(&pool);
        assert_eq!(cursor.index(), Some(0));

        // Move Next x2 -> After End
        cursor.move_next(&pool);
        cursor.move_next(&pool);
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);

        // Move Prev -> At 20
        cursor.move_prev(&pool);
        assert_eq!(cursor.index(), Some(1));
        assert_eq!(cursor.peek(&pool), Some(&20));

        list.clear(&mut pool);
    }

    #[test]
    fn test_insert_at_edges() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut cursor = list.cursor_mut(&mut pool); // Empty -> After End / Before Start (ambiguous on empty)

        // Empty list logic check
        cursor.move_to_back(&pool); // After End (index 0, len 0)
        cursor.insert_before(100, &mut pool).unwrap(); // Append
        assert_eq!(cursor.index(), Some(0));
        assert_eq!(cursor.peek(&pool), Some(&100));

        // Move Before Start
        cursor.move_prev(&pool); // Before Start
        cursor.insert_before(50, &mut pool).unwrap(); // Prepend
        assert_eq!(cursor.index(), Some(0)); // Cursor moves to new item
        assert_eq!(cursor.peek(&pool), Some(&50));

        // Verify order
        let vec: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(vec, vec![50, 100]);

        list.clear(&mut pool);
    }

    #[test]
    fn test_split_and_splice() {
        let mut pool = ElemPool::new();
        let mut list1 = list_with_items(&mut pool, &[1, 2, 3, 4]);
        let mut list2;

        {
            let mut cursor = list1.cursor_mut_at(2, &mut pool).unwrap(); // At 3
            list2 = cursor.split_before(&mut pool).unwrap();
            // list1 now {3, 4}, list2 {1, 2}
            assert_eq!(cursor.index(), Some(0));
            assert_eq!(cursor.peek(&pool), Some(&3));
        }

        {
            let mut cursor = list2.cursor_mut_at(1, &mut pool).unwrap(); // At 2
            cursor.splice_before(&mut list1, &mut pool).unwrap();
            // list2 should now be {1, 3, 4, 2}
        }

        let vec: Vec<_> = list2.iter(&pool).copied().collect();
        assert_eq!(vec, vec![1, 3, 4, 2]);
        assert!(list1.is_empty());

        list2.clear(&mut pool);
        list1.clear(&mut pool);
    }

    #[test]
    fn test_error_conditions() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10]);

        assert!(matches!(list.cursor_mut_at(1, &mut pool), Err(IndexError::IndexOutOfBounds)));
        list.clear(&mut pool);
    }

    /// Split and splice at larger scale, verifying pool consistency.
    /// This catches sentinel or link corruption that only shows up when
    /// multiple splits/splices interact with the same pool.
    #[test]
    fn test_split_splice_large() {
        let mut pool = ElemPool::new();
        let n = 100;
        let mut list = list_with_items(&mut pool, &(0..n).collect::<Vec<_>>());
        assert_eq!(pool.list_count(), 1);
        assert_eq!(pool.len(), n as usize);

        // Split at the midpoint.
        let mut back_half;
        {
            let mut cursor = list.cursor_mut_at(50, &mut pool).unwrap();
            back_half = cursor.split_before(&mut pool).unwrap();
        }
        // list = [50..100), back_half = [0..50)
        assert_eq!(list.len(), 50);
        assert_eq!(back_half.len(), 50);
        assert_eq!(pool.list_count(), 2);
        assert_eq!(pool.len(), n as usize);

        let list_vec: Vec<_> = list.iter(&pool).copied().collect();
        let back_vec: Vec<_> = back_half.iter(&pool).copied().collect();
        assert_eq!(list_vec, (50..100).collect::<Vec<_>>());
        assert_eq!(back_vec, (0..50).collect::<Vec<_>>());

        // Splice back_half into the middle of list (before index 25, which is element 75).
        {
            let mut cursor = list.cursor_mut_at(25, &mut pool).unwrap();
            cursor.splice_before(&mut back_half, &mut pool).unwrap();
        }
        assert!(back_half.is_empty());
        assert_eq!(list.len(), n as usize);
        assert_eq!(pool.list_count(), 2); // back_half sentinel still alive
        assert_eq!(pool.len(), n as usize);

        // Verify: [50..75, 0..50, 75..100]
        let result: Vec<_> = list.iter(&pool).copied().collect();
        let mut expected: Vec<i32> = (50..75).collect();
        expected.extend(0..50);
        expected.extend(75..100);
        assert_eq!(result, expected);

        list.clear(&mut pool);
        back_half.clear(&mut pool);
    }

    /// Repeated split/splice cycles must not leak pool resources.
    #[test]
    fn test_split_splice_repeated_no_leak() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &(0..40).collect::<Vec<_>>());

        for _ in 0..20 {
            let mut tail;
            {
                let mut cursor = list.cursor_mut_at(20, &mut pool).unwrap();
                tail = cursor.split_before(&mut pool).unwrap();
            }
            assert_eq!(list.len(), 20);
            assert_eq!(tail.len(), 20);

            // Splice back at front.
            {
                let mut cursor = list.cursor_mut(&mut pool);
                cursor.move_to_front(&pool);
                cursor.splice_before(&mut tail, &mut pool).unwrap();
            }
            assert_eq!(list.len(), 40);
            assert!(tail.is_empty());
            tail.clear(&mut pool);
        }

        assert_eq!(pool.len(), 40);
        list.clear(&mut pool);
    }

    #[test]
    fn test_insert_after() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 30]);

        // Insert after first element.
        {
            let mut cursor = list.cursor_mut(&mut pool);
            // cursor starts at index 0 (value 10)
            cursor.insert_after(20, &mut pool).unwrap();
            assert_eq!(cursor.index(), Some(0));
            assert_eq!(*cursor.peek(&pool).unwrap(), 10);
        }
        let items: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(items, vec![10, 20, 30]);

        // Insert after last element.
        {
            let mut cursor = list.cursor_mut(&mut pool);
            cursor.move_to_back(&pool);
            cursor.insert_after(40, &mut pool).unwrap();
        }
        let items: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(items, vec![10, 20, 30, 40]);

        // Insert after "Before Start" -> prepend.
        {
            let mut cursor = list.cursor_mut(&mut pool);
            // move_prev from front to Before Start
            cursor.move_prev(&pool);
            assert_eq!(cursor.index(), None);
            cursor.insert_after(5, &mut pool).unwrap();
        }
        let items: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(items, vec![5, 10, 20, 30, 40]);

        assert_eq!(list.len(), 5);
        list.clear(&mut pool);
    }

    #[test]
    fn test_remove_current_edges() {
        let mut pool = ElemPool::new();
        let mut list = list_with_items(&mut pool, &[10, 20, 30]);

        // Remove from Before Start returns None.
        {
            let mut cursor = list.cursor_mut(&mut pool);
            cursor.move_prev(&pool); // Before Start
            assert_eq!(cursor.remove_current(&mut pool), None);
        }
        assert_eq!(list.len(), 3);

        // Remove last element; cursor moves to After End.
        {
            let mut cursor = list.cursor_mut_at(2, &mut pool).unwrap();
            assert_eq!(*cursor.peek(&pool).unwrap(), 30);
            let removed = cursor.remove_current(&mut pool);
            assert_eq!(removed, Some(30));
            assert_eq!(cursor.index(), None); // After End
        }
        let items: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(items, vec![10, 20]);

        // Remove first element; cursor moves to next.
        {
            let mut cursor = list.cursor_mut(&mut pool);
            let removed = cursor.remove_current(&mut pool);
            assert_eq!(removed, Some(10));
            assert_eq!(cursor.index(), Some(0));
            assert_eq!(*cursor.peek(&pool).unwrap(), 20);
        }
        assert_eq!(list.len(), 1);
        list.clear(&mut pool);
    }
}
