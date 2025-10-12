//! A mutable cursor for navigating and modifying a `PieList`.
#![allow(unsafe_code)] // Unsafe is no longer used, but we keep the allow attribute for now.

use crate::index::Index;
use crate::list::PieList;
use crate::pool::{ElemPool, IndexError};

/// A cursor with mutable access to a `PieList`.
///
/// A `CursorMut` holds a mutable reference to its `PieList`, locking it from
/// other modifications, but does *not* lock the underlying `ElemPool`. The pool
/// is borrowed ephemerally by each method, allowing multiple lists in the same
/// pool to be manipulated by their own cursors simultaneously.
pub struct CursorMut<'a, T> {
    list: &'a mut PieList<T>,
    current: Index<T>,
    logical_index: usize,
}

impl<'a, T> CursorMut<'a, T> {
    /// Creates a new cursor. This is intended for internal use.
    pub(crate) fn new(list: &'a mut PieList<T>, current: Index<T>, logical_index: usize) -> Self {
        Self {
            list,
            current,
            logical_index,
        }
    }

    /// Returns the logical index of the cursor's current position.
    /// Returns `None` if the cursor is at an invalid position (e.g., past the end).
    pub fn index(&self) -> Option<usize> {
        if self.current == self.list.sentinel {
            None
        } else {
            Some(self.logical_index)
        }
    }

    /// Provides a mutable reference to the element at the cursor's current position.
    /// Returns `None` if the cursor is not pointing at a valid element.
    pub fn peek_mut<'p>(&mut self, pool: &'p mut ElemPool<T>) -> Option<&'p mut T> {
        if self.current == self.list.sentinel {
            None
        } else {
            pool.data_mut(self.current)
        }
    }

    /// Moves the cursor to the next element in the list.
    pub fn move_next(&mut self, pool: &mut ElemPool<T>) {
        if self.current == self.list.sentinel {
            return;
        }
        self.current = pool.next(self.current);
        if self.current == self.list.sentinel {
            self.logical_index = self.list.len();
        } else {
            self.logical_index += 1;
        }
    }

    /// Moves the cursor to the previous element in the list.
    pub fn move_prev(&mut self, pool: &mut ElemPool<T>) {
        if self.current == self.list.sentinel {
            return;
        }
        self.current = pool.prev(self.current);
        if self.current == self.list.sentinel {
            self.logical_index = 0;
        } else {
            self.logical_index -= 1;
        }
    }

    /// Inserts a new element *before* the cursor's current position.
    /// The cursor will point to the newly inserted element.
    pub fn insert_before(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_before(new_idx, self.current)?;
        self.list.len += 1;
        self.current = new_idx;
        Ok(())
    }

    /// Inserts a new element *after* the cursor's current position.
    /// The cursor's position does not change.
    pub fn insert_after(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_after(new_idx, self.current)?;
        self.list.len += 1;
        Ok(())
    }

    /// Removes the element at the cursor's current position and returns its data.
    /// The cursor moves to the next element.
    pub fn remove_current(&mut self, pool: &mut ElemPool<T>) -> Option<T> {
        if self.current == self.list.sentinel {
            return None;
        }
        let old_current = self.current;
        self.current = pool.next(old_current);

        pool.index_linkout(old_current).unwrap();
        self.list.len -= 1;
        let data = pool.data_swap(old_current, None);
        pool.index_del(old_current).unwrap();

        data
    }

    /// Splits the list into two at the cursor's current position.
    /// The original list will contain all elements *after* the split point.
    /// A new list containing all elements *before* the split point is returned.
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

        // 1. Form the new list: new_sentinel <-> original_front <-> ... <-> element_before_split <-> new_sentinel
        pool.get_mut(new_list.sentinel)?.new_links(element_before_split, original_front);
        pool.get_mut(original_front)?.new_prev(new_list.sentinel);
        pool.get_mut(element_before_split)?.new_next(new_list.sentinel);

        // 2. Form the original, now-shortened list: old_sentinel <-> self.current <-> ... <-> original_back <-> old_sentinel
        pool.get_mut(self.list.sentinel)?.new_links(original_back, self.current);
        pool.get_mut(self.current)?.new_prev(self.list.sentinel);
        pool.get_mut(original_back).unwrap().new_next(self.list.sentinel);

        // --- Update lengths and cursor state ---
        self.list.len = original_len - split_point_idx;
        new_list.len = split_point_idx;
        self.logical_index = 0; // The cursor is now at the start of the modified list.
        Ok(new_list)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
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

        cursor.move_prev(&mut pool);
        assert_eq!(cursor.index(), None);

        let mut cursor2 = list.cursor_mut_at(2, &mut pool).unwrap();
        cursor2.move_prev(&mut pool);
        assert_eq!(cursor2.index(), Some(1));
        assert_eq!(*cursor2.peek_mut(&mut pool).unwrap(), 20);
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

            // FIX: Access list properties through the cursor.
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

            // FIX: Access list properties through the cursor.
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

        // FIX: Access list properties through the cursor.
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

            // FIX: Assert on state via the cursor while it's alive.
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

        // 3. Mutate list1 directly is now impossible because cursor1 holds a borrow.
        // Instead, we access it through the cursor.
        assert_eq!(cursor1.list.len(), 4);

        // 4. Mutate list2 directly while cursor1 exists (and vice-versa).
        // This is safe because they borrow different lists.
        assert_eq!(list2.pop_front(&mut pool), Some(100)); // list2: 250, 300

        // Final verification
        assert_eq!(list1.len(), 4); // `cursor1` went out of scope, so we can access `list1`
        assert_eq!(list2.len(), 2); // `cursor2` also went out of scope
        assert_eq!(pool.len(), 6); // 4 + 2 = 6 used elements

        let vec1: Vec<_> = list1.iter(&pool).copied().collect();
        assert_eq!(vec1, vec![10, 15, 20, 30]);

        let vec2: Vec<_> = list2.iter(&pool).copied().collect();
        assert_eq!(vec2, vec![250, 300]);
    }
}
