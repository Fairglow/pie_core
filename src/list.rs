//! Main implementation of the PieList type
// Allow unsafe for the performance-critical iterator implementation.
#![allow(unsafe_code)]

use crate::cursor::CursorMut;
use crate::index::Index;
use crate::pool::{ElemPool, IndexError};
use std::marker::PhantomData;

/// A handle to a doubly-linked list within a shared `ElemPool`.
///
/// A `PieList` itself is a lightweight struct containing only an `Index` to a
/// sentinel node and the list's length. All list elements are stored and managed
/// by a separate `ElemPool`. This design allows for many `PieList`s to share
/// memory from a single pool.
///
/// All operations that modify or access the list's elements, such as `push_back`
/// or `front`, require a mutable or immutable reference to the `ElemPool` where
/// the data is stored.
///
/// # Important: Memory Management
///
/// When a `PieList` is dropped, the elements it references are **not** automatically
/// returned to the pool. This is a deliberate design choice to allow lists to be
/// moved and managed without unintended side effects on the pool.
///
/// To prevent memory leaks within the pool, you **must** call [`clear()`] on a list
/// when you are finished with it. This will iterate through all its elements and
/// return them to the pool's free list, making them available for reuse.
///
/// [`clear()`]: PieList::clear
#[derive(Debug)]
pub struct PieList<T> {
    /// The index of the sentinel node for this list. The sentinel's `next`
    /// points to the head of the list, and its `prev` points to the tail.
    pub(crate) sentinel: Index<T>,
    /// The number of elements in the list.
    pub(crate) len: usize,
}

impl<T> PieList<T> {
    /// Creates a new, empty list handle.
    ///
    /// This operation allocates a single sentinel node from the provided pool.
    /// The sentinel acts as a fixed entry point for the list, simplifying the
    /// logic for insertions and removals at the boundaries.
    ///
    /// # Panics
    ///
    /// Panics if the `ElemPool` cannot allocate a new element for the sentinel,
    /// which would typically only happen in an out-of-memory situation.
    pub fn new(pool: &mut ElemPool<T>) -> Self {
        let sentinel = pool
            .index_new()
            .expect("Pool failed to allocate sentinel for new list");
        // The list is created empty, so the sentinel initially points to itself.
        Self { sentinel, len: 0 }
    }

    /// Returns the number of elements in the list.
    ///
    /// # Complexity
    /// O(1)
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the list contains no elements.
    ///
    /// # Complexity
    /// O(1)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Provides a reference to the front element's data, or `None` if the list is empty.
    ///
    /// # Complexity
    /// O(1)
    pub fn front<'a>(&self, pool: &'a ElemPool<T>) -> Option<&'a T> {
        if self.is_empty() {
            return None;
        }
        pool.data(pool.next(self.sentinel))
    }

    /// Provides a mutable reference to the front element's data, or `None` if empty.
    ///
    /// # Complexity
    /// O(1)
    pub fn front_mut<'a>(&mut self, pool: &'a mut ElemPool<T>) -> Option<&'a mut T> {
        if self.is_empty() {
            return None;
        }
        let front_idx = pool.next(self.sentinel);
        pool.data_mut(front_idx)
    }

    /// Provides a reference to the back element's data, or `None` if the list is empty.
    ///
    /// # Complexity
    /// O(1)
    pub fn back<'a>(&self, pool: &'a ElemPool<T>) -> Option<&'a T> {
        if self.is_empty() {
            return None;
        }
        pool.data(pool.prev(self.sentinel))
    }

    /// Provides a mutable reference to the back element's data, or `None` if empty.
    ///
    /// # Complexity
    /// O(1)
    pub fn back_mut<'a>(&mut self, pool: &'a mut ElemPool<T>) -> Option<&'a mut T> {
        if self.is_empty() {
            return None;
        }
        let back_idx = pool.prev(self.sentinel);
        pool.data_mut(back_idx)
    }

    /// Adds an element to the front of the list.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns an `IndexError` if the pool is unable to allocate a new element.
    pub fn push_front(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_after(new_idx, self.sentinel)?;
        self.len += 1;
        Ok(())
    }

    /// Adds an element to the back of the list.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns an `IndexError` if the pool is unable to allocate a new element.
    pub fn push_back(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<(), IndexError> {
        let new_idx = pool.index_new()?;
        pool.data_swap(new_idx, Some(data));
        pool.index_link_before(new_idx, self.sentinel)?;
        self.len += 1;
        Ok(())
    }

    /// Removes the first element and returns its data, or `None` if the list is empty.
    ///
    /// The removed element's node is returned to the pool's free list.
    ///
    /// # Complexity
    /// O(1)
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

    /// Removes the last element and returns its data, or `None` if the list is empty.
    ///
    /// The removed element's node is returned to the pool's free list.
    ///
    /// # Complexity
    /// O(1)
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

    /// Removes all elements from the list, returning them to the pool's free list.
    ///
    /// This is a critical method for memory management. Failure to call `clear`
    /// on a list that is no longer needed will result in its elements being
    /// leaked within the pool, as they will never be added to the free list for reuse.
    ///
    /// # Complexity
    /// O(n), where n is the number of elements in the list.
    pub fn clear(&mut self, pool: &mut ElemPool<T>) {
        while self.pop_front(pool).is_some() {}
    }

    /// Returns an iterator that provides immutable references to the elements
    /// from front to back.
    pub fn iter<'a>(&self, pool: &'a ElemPool<T>) -> Iter<'a, T> {
        Iter {
            pool,
            front: pool.next(self.sentinel),
            back: pool.prev(self.sentinel),
            len: self.len,
            _phantom: PhantomData,
        }
    }

    /// Returns an iterator that provides mutable references to the elements
    /// from front to back.
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
    ///
    /// The cursor provides an efficient API for arbitrary insertion, deletion,
    /// and moving through the list.
    pub fn cursor_mut<'a>(&'a mut self, pool: &mut ElemPool<T>) -> CursorMut<'a, T> {
        let first_elem = pool.next(self.sentinel);
        CursorMut::new(self, first_elem, 0)
    }

    /// Returns a mutable cursor pointing to the element at the given logical index.
    ///
    /// # Complexity
    /// O(min(k, n-k)), where `k` is the index and `n` is the list's length.
    /// The method traverses from the nearest end of the list to find the element.
    ///
    /// # Errors
    /// Returns `Err(IndexError::IndexOutOfBounds)` if `index >= self.len()`.
    pub fn cursor_mut_at<'a>(
        &'a mut self,
        index: usize,
        pool: &mut ElemPool<T>,
    ) -> Result<CursorMut<'a, T>, IndexError> {
        if index >= self.len {
            return Err(IndexError::IndexOutOfBounds);
        }
        // To be efficient, we traverse from the closer end of the list.
        let mut current_idx;
        if index < self.len / 2 {
            // Traverse from the front
            current_idx = self.sentinel;
            for _ in 0..=index {
                current_idx = pool.next(current_idx);
            }
        } else {
            // Traverse from the back
            current_idx = self.sentinel;
            for _ in 0..(self.len - index) {
                current_idx = pool.prev(current_idx);
            }
        }
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
        // We convert the mutable reference to a raw pointer to bypass the borrow
        // checker's limitation on splitting borrows within a single method call.
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
        // SAFETY: Same reasoning as in `next()`. The exclusive borrow on `self.pool`
        // and the iterator's logic ensure that we do not create aliased mutable
        // references.
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
