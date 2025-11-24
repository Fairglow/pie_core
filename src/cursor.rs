//! A cursor for navigating and modifying a `PieList`.

use crate::{ElemPool, Index, PieList};

/// A cursor with immutable access to a `PieList`.
///
/// A `Cursor` provides read-only navigation through a `PieList`. Unlike an
/// iterator, a cursor can move back and forth freely.
///
/// # Example
/// ```
/// # use pie_core::{ElemPool, PieList};
/// # let mut pool = ElemPool::new();
/// # let mut list = PieList::new(&mut pool);
/// # list.push_back(10, &mut pool).unwrap();
/// # list.push_back(20, &mut pool).unwrap();
/// let mut cursor = list.cursor(&pool);
/// cursor.move_next(&pool);
/// assert_eq!(cursor.peek(&pool), Some(&20));
/// cursor.move_prev(&pool);
/// assert_eq!(cursor.peek(&pool), Some(&10));
/// ```
#[derive(Debug)]
pub struct Cursor<'a, T> {
    list: &'a PieList<T>,
    current: Index<T>,
    logical_index: usize,
}

impl<'a, T> Cursor<'a, T> {
    /// Creates a new cursor.
    pub(crate) fn new(list: &'a PieList<T>, current: Index<T>, logical_index: usize) -> Self {
        Self {
            list,
            current,
            logical_index,
        }
    }

    /// Returns the logical index of the cursor's current position.
    /// Returns `None` if the cursor is pointing to the sentinel (past end/before start).
    pub fn index(&self) -> Option<usize> {
        if self.current == self.list.sentinel {
            None
        } else {
            Some(self.logical_index)
        }
    }

    /// Provides a reference to the element at the cursor's current position.
    pub fn peek<'p>(&self, pool: &'p ElemPool<T>) -> Option<&'p T> {
        if self.current == self.list.sentinel {
            None
        } else {
            pool.data(self.current)
        }
    }

    /// Moves the cursor to the next element.
    pub fn move_next(&mut self, pool: &ElemPool<T>) {
        if self.current == self.list.sentinel && !self.list.is_empty() && self.logical_index == self.list.len() {
            // Already at end, do nothing
            return;
        }

        let next = pool.next(self.current);
        self.current = next;

        if self.current == self.list.sentinel {
            self.logical_index = self.list.len();
        } else {
            // If we were at sentinel (start), we are now at 0.
            // If we were at 0, we are now at 1.
            // However, our internal logical_index at sentinel(start) is 0.
            // We need to handle the wrapped case carefully.
            // Simplified logic: just increment unless we wrapped to start.
            self.logical_index += 1;
        }

        // Correction for wrapping from End Sentinel back to Start
        // (If your list allows circular iteration, otherwise standard cursors stop at sentinel)
        // Assuming standard cursor behavior (stops at sentinel):
        if self.current != self.list.sentinel && self.logical_index > self.list.len() {
            self.logical_index = 0;
        }
    }

    /// Moves the cursor to the previous element.
    pub fn move_prev(&mut self, pool: &ElemPool<T>) {
        if self.current == self.list.sentinel && !self.list.is_empty() && self.logical_index == 0 {
            // Already at start (logical 0 but pointing to sentinel), do nothing?
            // Actually sentinel usually represents "past end".
            // Let's stick to the logic used in CursorMut.
        }

        let prev = pool.prev(self.current);
        self.current = prev;

        if self.current == self.list.sentinel {
            self.logical_index = 0;
        } else if self.logical_index > 0 {
            self.logical_index -= 1;
        } else {
            // Wrapped around?
            self.logical_index = self.list.len() - 1;
        }
    }

    /// Moves the cursor to the first element of the list.
    pub fn move_to_front(&mut self, pool: &ElemPool<T>) {
        self.current = pool.next(self.list.sentinel);
        self.logical_index = 0;
    }

    /// Moves the cursor to the last element of the list.
    pub fn move_to_back(&mut self, pool: &ElemPool<T>) {
        self.current = pool.prev(self.list.sentinel);
        if self.list.is_empty() {
            self.logical_index = 0;
        } else {
            self.logical_index = self.list.len() - 1;
        }
    }
}
