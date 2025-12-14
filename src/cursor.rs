//! A cursor for navigating a `PieList`.

use crate::{ElemPool, Index, PieList};

/// A cursor with immutable access to a `PieList`.
///
/// A `Cursor` provides read-only navigation through a `PieList`. Unlike an
/// iterator, a cursor can move back and forth freely.
///
/// The cursor has two special states:
/// 1. "Before Start": Positioned before the first element.
/// 2. "After End": Positioned after the last element.
///
/// # Example
/// ```
/// use pie_core::{ElemPool, PieList};
/// let mut pool = ElemPool::new();
/// let mut list = PieList::new(&mut pool);
/// list.push_back(10, &mut pool).unwrap();
/// list.push_back(20, &mut pool).unwrap();
///
/// let mut cursor = list.cursor(&pool);
///
/// // Starts at first element (10)
/// assert_eq!(cursor.peek(&pool), Some(&10));
///
/// // Move to next
/// cursor.move_next(&pool);
/// assert_eq!(cursor.peek(&pool), Some(&20));
///
/// // Move back
/// cursor.move_prev(&pool);
/// assert_eq!(cursor.peek(&pool), Some(&10));
///
/// // Clean up
/// list.clear(&mut pool);
/// ```
#[derive(Debug)]
pub struct Cursor<'a, T> {
    pub(crate) list: &'a PieList<T>,
    /// The current element index. If this is `list.sentinel`, we are either
    /// "Before Start" or "After End".
    pub(crate) current: Index<T>,
    /// The logical position.
    /// `0` to `len-1` are valid elements.
    /// `len` indicates "After End".
    /// `usize::MAX` indicates "Before Start".
    pub(crate) logical_index: usize,
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
    ///
    /// Returns `None` if the cursor is pointing to the sentinel (i.e., is
    /// "Before Start" or "After End").
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(10, &mut pool).unwrap();
    /// let mut cursor = list.cursor(&pool); // Starts at index 0
    /// assert_eq!(cursor.index(), Some(0));
    ///
    /// cursor.move_next(&pool); // Moves to After End
    /// assert_eq!(cursor.index(), None);
    /// # list.clear(&mut pool);
    /// ```
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
    /// let mut cursor = list.cursor(&pool);
    /// assert_eq!(cursor.peek(&pool), Some(&10));
    /// # list.clear(&mut pool);
    /// ```
    pub fn peek<'p>(&self, pool: &'p ElemPool<T>) -> Option<&'p T> {
        if self.current == self.list.sentinel {
            None
        } else {
            pool.data(self.current)
        }
    }

    /// Moves the cursor to the next element.
    ///
    /// If the cursor is "Before Start", it moves to the first element (or "After End" if empty).
    /// If the cursor is at the last element, it moves to "After End".
    /// If the cursor is "After End", it stays there (no-op).
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(1, &mut pool).unwrap();
    /// let mut cursor = list.cursor(&pool); // At 0
    /// cursor.move_next(&pool); // Now After End
    /// assert!(cursor.peek(&pool).is_none());
    /// # list.clear(&mut pool);
    /// ```
    pub fn move_next(&mut self, pool: &ElemPool<T>) {
        // Case 1: Already at "After End" -> No-op
        if self.current == self.list.sentinel && self.logical_index == self.list.len() {
            return;
        }

        // Case 2: At "Before Start" (usize::MAX) -> Move to Head (0)
        // Note: If list is empty, Head is Sentinel, and logical becomes 0 (which matches len).
        if self.current == self.list.sentinel && self.logical_index == usize::MAX {
            self.current = pool.next(self.list.sentinel);
            self.logical_index = 0;
            return;
        }

        // Case 3: On a valid element
        self.current = pool.next(self.current);
        if self.current == self.list.sentinel {
            // We just fell off the end
            self.logical_index = self.list.len();
        } else {
            // Moved to next valid element
            self.logical_index += 1;
        }
    }

    /// Moves the cursor to the previous element.
    ///
    /// If the cursor is "After End", it moves to the last element.
    /// If the cursor is at the first element, it moves to "Before Start".
    /// If the cursor is "Before Start", it stays there (no-op).
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(1, &mut pool).unwrap();
    /// let mut cursor = list.cursor(&pool); // At 0
    /// cursor.move_prev(&pool); // Now Before Start
    /// assert!(cursor.peek(&pool).is_none());
    /// # list.clear(&mut pool);
    /// ```
    pub fn move_prev(&mut self, pool: &ElemPool<T>) {
        // Case 1: Already at "Before Start" -> No-op
        if self.current == self.list.sentinel && self.logical_index == usize::MAX {
            return;
        }

        // Case 2: At "After End" (len) -> Move to Tail (len-1)
        // Note: If list is empty, Tail is Sentinel, logic falls through to set usize::MAX
        if self.current == self.list.sentinel && self.logical_index == self.list.len() {
            if self.list.is_empty() {
                // Empty list: After End -> Before Start
                self.logical_index = usize::MAX;
                return;
            }
            self.current = pool.prev(self.list.sentinel);
            self.logical_index = self.list.len() - 1;
            return;
        }

        // Case 3: On a valid element
        self.current = pool.prev(self.current);
        if self.current == self.list.sentinel {
            // We moved backward from the first element -> Before Start
            self.logical_index = usize::MAX;
        } else {
            // Moved to previous valid element
            self.logical_index -= 1;
        }
    }

    /// Moves the cursor to the first element of the list.
    ///
    /// If the list is empty, the cursor ends up in the "After End" state (index = 0, len = 0).
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(1, &mut pool).unwrap();
    /// # list.push_back(2, &mut pool).unwrap();
    /// let mut cursor = list.cursor(&pool);
    /// cursor.move_to_back(&pool);
    /// cursor.move_to_front(&pool);
    /// assert_eq!(cursor.peek(&pool), Some(&1));
    /// # list.clear(&mut pool);
    /// ```
    pub fn move_to_front(&mut self, pool: &ElemPool<T>) {
        self.current = pool.next(self.list.sentinel);
        self.logical_index = 0;
    }

    /// Moves the cursor to the last element of the list.
    ///
    /// If the list is empty, the cursor ends up in the "After End" state.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// # list.push_back(1, &mut pool).unwrap();
    /// # list.push_back(2, &mut pool).unwrap();
    /// let mut cursor = list.cursor(&pool);
    /// cursor.move_to_back(&pool);
    /// assert_eq!(cursor.peek(&pool), Some(&2));
    /// # list.clear(&mut pool);
    /// ```
    pub fn move_to_back(&mut self, pool: &ElemPool<T>) {
        self.current = pool.prev(self.list.sentinel);
        if self.list.is_empty() {
            self.logical_index = 0;
        } else {
            self.logical_index = self.list.len() - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ElemPool, PieList};

    fn setup_list(len: usize) -> (ElemPool<i32>, PieList<i32>) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for i in 0..len {
            list.push_back(i as i32, &mut pool).unwrap();
        }
        (pool, list)
    }

    #[test]
    fn new_cursor_on_empty_list() {
        let mut pool = ElemPool::new();
        let mut list: PieList<i32> = PieList::new(&mut pool);
        let cursor = list.cursor(&pool);
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);
        list.clear(&mut pool);
    }

    #[test]
    fn new_cursor_on_non_empty_list() {
        let (mut pool, mut list) = setup_list(3);
        let cursor = list.cursor(&pool);
        assert_eq!(cursor.index(), Some(0));
        assert_eq!(cursor.peek(&pool), Some(&0));
        list.clear(&mut pool);
    }

    #[test]
    fn move_next_and_peek() {
        let (mut pool, mut list) = setup_list(3);
        let mut cursor = list.cursor(&pool);

        assert_eq!(cursor.peek(&pool), Some(&0));
        cursor.move_next(&pool);
        assert_eq!(cursor.peek(&pool), Some(&1));
        cursor.move_next(&pool);
        assert_eq!(cursor.peek(&pool), Some(&2));
        cursor.move_next(&pool);
        assert_eq!(cursor.peek(&pool), None); // Past the end
        list.clear(&mut pool);
    }

    #[test]
    fn move_prev_and_peek() {
        let (mut pool, mut list) = setup_list(3);
        let mut cursor = list.cursor(&pool);
        cursor.move_to_back(&pool);

        assert_eq!(cursor.peek(&pool), Some(&2));
        cursor.move_prev(&pool);
        assert_eq!(cursor.peek(&pool), Some(&1));
        cursor.move_prev(&pool);
        assert_eq!(cursor.peek(&pool), Some(&0));
        cursor.move_prev(&pool);
        assert_eq!(cursor.peek(&pool), None); // Before the start
        list.clear(&mut pool);
    }

    #[test]
    fn move_next_and_index() {
        let (mut pool, mut list) = setup_list(3);
        let mut cursor = list.cursor(&pool);

        assert_eq!(cursor.index(), Some(0));
        cursor.move_next(&pool);
        assert_eq!(cursor.index(), Some(1));
        cursor.move_next(&pool);
        assert_eq!(cursor.index(), Some(2));
        cursor.move_next(&pool);
        assert_eq!(cursor.index(), None); // Past the end
        list.clear(&mut pool);
    }

    #[test]
    fn move_prev_and_index() {
        let (mut pool, mut list) = setup_list(3);
        let mut cursor = list.cursor(&pool);
        cursor.move_to_back(&pool);

        assert_eq!(cursor.index(), Some(2));
        cursor.move_prev(&pool);
        assert_eq!(cursor.index(), Some(1));
        cursor.move_prev(&pool);
        assert_eq!(cursor.index(), Some(0));
        cursor.move_prev(&pool);
        assert_eq!(cursor.index(), None); // Before the start
        list.clear(&mut pool);
    }

    #[test]
    fn move_next_at_end_should_be_noop() {
        let (mut pool, mut list) = setup_list(2);
        let mut cursor = list.cursor(&pool);
        cursor.move_next(&pool); // at index 1
        cursor.move_next(&pool); // at end
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);

        cursor.move_next(&pool); // should do nothing
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);
        list.clear(&mut pool);
    }

    #[test]
    fn move_prev_at_start_should_be_noop() {
        let (mut pool, mut list) = setup_list(2);
        let mut cursor = list.cursor(&pool); // at index 0
        cursor.move_prev(&pool); // before start
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);

        cursor.move_prev(&pool); // should do nothing
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);
        list.clear(&mut pool);
    }

    #[test]
    fn move_from_before_start_to_first() {
        let (mut pool, mut list) = setup_list(2);
        let mut cursor = list.cursor(&pool); // at index 0
        cursor.move_prev(&pool); // before start
        assert_eq!(cursor.index(), None);

        cursor.move_next(&pool);
        assert_eq!(cursor.index(), Some(0));
        assert_eq!(cursor.peek(&pool), Some(&0));
        list.clear(&mut pool);
    }

    #[test]
    fn move_to_front_and_back() {
        let (mut pool, mut list) = setup_list(3);
        let mut cursor = list.cursor(&pool);

        cursor.move_to_back(&pool);
        assert_eq!(cursor.index(), Some(2));
        assert_eq!(cursor.peek(&pool), Some(&2));

        cursor.move_to_front(&pool);
        assert_eq!(cursor.index(), Some(0));
        assert_eq!(cursor.peek(&pool), Some(&0));
        list.clear(&mut pool);
    }

    #[test]
    fn move_to_front_and_back_on_empty_list() {
        let mut pool = ElemPool::new();
        let mut list: PieList<i32> = PieList::new(&mut pool);
        let mut cursor = list.cursor(&pool);

        cursor.move_to_front(&pool);
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);

        cursor.move_to_back(&pool);
        assert_eq!(cursor.index(), None);
        assert_eq!(cursor.peek(&pool), None);
        list.clear(&mut pool);
    }
}
