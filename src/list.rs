//! Main implementation of the PieList type
// Allow unsafe for the performance-critical iterator implementation.
#![allow(unsafe_code)]

use crate::pool::{ElemPool, IndexError};
use crate::index::Index;
use crate::cursor::CursorMut;
use std::marker::PhantomData;

/// A handle to a doubly-linked list within a shared `ElemPool`.
///
/// A `PieList` holds a sentinel node index and tracks its length.
/// All list operations require a mutable reference to the pool where the
/// elements are actually stored.
///
/// # Important
///
/// When a `PieList` is no longer needed, you **must** call `clear()` on it
/// to return its elements to the pool's free list. Failure to do so
/// will result in a memory leak within the pool.
#[derive(Debug)]
pub struct PieList<T> {
    pub(crate) sentinel: Index<T>,
    pub(crate) len: usize,
}

impl<T> PieList<T> {
    /// Creates a new, empty list handle.
    ///
    /// This allocates a sentinel node from the pool to represent the head/tail
    /// of the list.
    ///
    /// # Panics
    ///
    /// Panics if the pool fails to allocate a new element, which should
    /// only happen in out-of-memory conditions.
    pub fn new(pool: &mut ElemPool<T>) -> Self {
        let sentinel = pool.index_new().expect("Pool failed to allocate sentinel for new list");
        Self {
            sentinel,
            len: 0,
        }
    }

    /// Returns the number of elements in the list. O(1).
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the list contains no elements. O(1).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Provides a reference to the front element's data, or `None` if the list is empty.
    pub fn front<'a>(&self, pool: &'a ElemPool<T>) -> Option<&'a T> {
        if self.is_empty() {
            return None;
        }
        pool.data(pool.next(self.sentinel))
    }

    /// Provides a mutable reference to the front element's data, or `None` if empty.
    pub fn front_mut<'a>(&self, pool: &'a mut ElemPool<T>) -> Option<&'a mut T> {
        if self.is_empty() {
            return None;
        }
        let front_idx = pool.next(self.sentinel);
        pool.data_mut(front_idx)
    }

    /// Provides a reference to the back element's data, or `None` if the list is empty.
    pub fn back<'a>(&self, pool: &'a ElemPool<T>) -> Option<&'a T> {
        if self.is_empty() {
            return None;
        }
        pool.data(pool.prev(self.sentinel))
    }

    /// Provides a mutable reference to the back element's data, or `None` if empty.
    pub fn back_mut<'a>(&self, pool: &'a mut ElemPool<T>) -> Option<&'a mut T> {
        if self.is_empty() {
            return None;
        }
        let back_idx = pool.prev(self.sentinel);
        pool.data_mut(back_idx)
    }

    /// Adds an element to the front of the list. O(1).
    pub fn push_front(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_after(new_idx, self.sentinel)?;
        self.len += 1;
        Ok(())
    }

    /// Adds an element to the back of the list. O(1).
    pub fn push_back(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_before(new_idx, self.sentinel)?;
        self.len += 1;
        Ok(())
    }

    /// Removes the first element and returns its data, or `None` if the list is empty. O(1).
    pub fn pop_front(&mut self, pool: &mut ElemPool<T>) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let front_idx = pool.next(self.sentinel);
        pool.index_linkout(front_idx).ok()?;
        self.len -= 1;
        let data = pool.data_swap(front_idx, None);
        pool.index_del(front_idx).ok()?;
        data
    }

    /// Removes the last element and returns its data, or `None` if the list is empty. O(1).
    pub fn pop_back(&mut self, pool: &mut ElemPool<T>) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let back_idx = pool.prev(self.sentinel);
        pool.index_linkout(back_idx).ok()?;
        self.len -= 1;
        let data = pool.data_swap(back_idx, None);
        pool.index_del(back_idx).ok()?;
        data
    }

    /// Removes all elements from the list, returning them to the pool's free list. O(n).
    pub fn clear(&mut self, pool: &mut ElemPool<T>) {
        while self.pop_front(pool).is_some() {}
    }

    /// Returns an iterator that provides immutable references to the elements.
    pub fn iter<'a>(&self, pool: &'a ElemPool<T>) -> Iter<'a, T> {
        Iter {
            pool,
            front: pool.next(self.sentinel),
            back: pool.prev(self.sentinel),
            len: self.len,
            _phantom: PhantomData,
        }
    }

    /// Returns an iterator that provides mutable references to the elements.
    pub fn iter_mut<'a>(&mut self, pool: &'a mut ElemPool<T>) -> IterMut<'a, T> {
        let front = pool.next(self.sentinel);
        let back = pool.prev(self.sentinel);
        IterMut {
            pool,
            front,
            back,
            len: self.len,
            _phantom: PhantomData,
        }
    }

    /// Returns a mutable cursor pointing to the first element of the list.
    pub fn cursor_mut<'a>(&'a mut self, pool: &mut ElemPool<T>) -> CursorMut<'a, T> {
        let first_elem = pool.next(self.sentinel);
        // Pass only the list, index, and logical position.
        CursorMut::new(self, first_elem, 0)
    }

    /// Returns a mutable cursor pointing to the element at the given logical index.
    ///
    /// Returns `Err(IndexError::IndexOutOfBounds)` if the index is out of bounds.
    pub fn cursor_mut_at<'a>(&'a mut self, index: usize, pool: &mut ElemPool<T>
    ) -> Result<CursorMut<'a, T>, IndexError> {
        if index >= self.len {
            return Err(IndexError::IndexOutOfBounds);
        }
        // To be efficient, we traverse from the closer end of the list.
        let mut current_idx = self.sentinel;
        if index < self.len / 2 {
            // Traverse from the front
            for _ in 0..=index {
                current_idx = pool.next(current_idx);
            }
        } else {
            // Traverse from the back
            for _ in 0..(self.len - index) {
                current_idx = pool.prev(current_idx);
            }
        }
        // Pass only the list, index, and logical position.
        Ok(CursorMut::new(self, current_idx, index))
    }
}

// --- Iterators ---

/// An immutable iterator over the elements of a `PieList`.
pub struct Iter<'a, T: 'a> {
    pool: &'a ElemPool<T>,
    front: Index<T>,
    back: Index<T>,
    len: usize,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        let current = self.front;
        self.front = self.pool.next(current);
        self.len -= 1;
        self.pool.data(current)
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        let current = self.back;
        self.back = self.pool.prev(current);
        self.len -= 1;
        self.pool.data(current)
    }
}

/// A mutable iterator over the elements of a `PieList`.
pub struct IterMut<'a, T: 'a> {
    pool: &'a mut ElemPool<T>,
    front: Index<T>,
    back: Index<T>,
    len: usize,
    _phantom: PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        let current = self.front;
        self.front = self.pool.next(current);
        self.len -= 1;
        // SAFETY: The lifetime 'a ties the output reference to the exclusive
        // borrow of the pool. The iterator's internal logic guarantees that we
        // never yield the same index twice, preventing aliased mutable references.
        let pool_ptr = self.pool as *mut ElemPool<T>;
        unsafe { (*pool_ptr).data_mut(current) }
    }
}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        let current = self.back;
        self.back = self.pool.prev(current);
        self.len -= 1;
        // SAFETY: The lifetime 'a ties the output reference to the exclusive
        // borrow of the pool. The iterator's internal logic guarantees that we
        // never yield the same index twice, preventing aliased mutable references.
        let pool_ptr = self.pool as *mut ElemPool<T>;
        unsafe { (*pool_ptr).data_mut(current) }
    }
}


// --- Test Suite for PieList ---

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::ElemPool;

    #[test]
    fn test_new_list() {
        let mut pool = ElemPool::<i32>::new();
        let list = PieList::new(&mut pool);
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        // Pool should have allocated one element for the sentinel
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.capacity(), 1);
    }

    #[test]
    fn test_push_and_pop() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);

        list.push_back("A", &mut pool).unwrap();
        list.push_front("B", &mut pool).unwrap(); // B, A
        list.push_back("C", &mut pool).unwrap(); // B, A, C

        assert_eq!(list.len(), 3);
        assert_eq!(*list.front(&pool).unwrap(), "B");
        assert_eq!(*list.back(&pool).unwrap(), "C");

        assert_eq!(list.pop_front(&mut pool), Some("B")); // A, C
        assert_eq!(list.len(), 2);
        assert_eq!(*list.front(&pool).unwrap(), "A");

        assert_eq!(list.pop_back(&mut pool), Some("C")); // A
        assert_eq!(list.len(), 1);
        assert_eq!(*list.front(&pool).unwrap(), "A");
        assert_eq!(*list.back(&pool).unwrap(), "A");

        assert_eq!(list.pop_front(&mut pool), Some("A"));
        assert!(list.is_empty());

        assert_eq!(list.pop_front(&mut pool), None);
        assert_eq!(list.pop_back(&mut pool), None);
    }

    #[test]
    fn test_clear() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(1, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(pool.len(), 2);

        list.clear(&mut pool);
        assert!(list.is_empty());
        assert_eq!(pool.len(), 0); // Elements returned to the pool
    }

    #[test]
    fn test_front_back_mut() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();

        *list.front_mut(&mut pool).unwrap() = 15;
        *list.back_mut(&mut pool).unwrap() = 25;

        assert_eq!(list.pop_front(&mut pool), Some(15));
        assert_eq!(list.pop_front(&mut pool), Some(25));
    }

    #[test]
    fn test_iter() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(1, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        list.push_back(3, &mut pool).unwrap();

        let mut iter = list.iter(&pool);
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next_back(), Some(&3));
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);

        // Test collection
        let vec: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_iter_mut() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();

        for item in list.iter_mut(&mut pool) {
            *item *= 2;
        }

        let vec: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(vec, vec![20, 40]);
    }

    #[test]
    fn test_multiple_lists_in_one_pool() {
        let mut pool = ElemPool::new();
        let mut list1 = PieList::new(&mut pool);
        let mut list2 = PieList::new(&mut pool);

        list1.push_back("a", &mut pool).unwrap();
        list1.push_back("b", &mut pool).unwrap();

        list2.push_back("x", &mut pool).unwrap();
        list2.push_back("y", &mut pool).unwrap();
        list2.push_back("z", &mut pool).unwrap();

        // Check lists are independent
        assert_eq!(list1.len(), 2);
        assert_eq!(list2.len(), 3);
        assert_eq!(pool.len(), 5); // Total elements in pool

        assert_eq!(list1.pop_front(&mut pool), Some("a"));
        assert_eq!(list2.pop_front(&mut pool), Some("x"));

        assert_eq!(pool.len(), 3);

        // Clear list2, elements should be freed
        list2.clear(&mut pool);
        assert!(list2.is_empty());
        assert_eq!(list1.len(), 1);
        assert_eq!(pool.len(), 1); // Only list1's "b" remains

        // Now list2 can reuse freed elements
        list2.push_back("new", &mut pool).unwrap();
        assert_eq!(list2.len(), 1);
        assert_eq!(pool.len(), 2);
    }
}