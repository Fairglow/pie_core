//! Mutable view implementation.

use crate::list::{Iter, IterMut};
use crate::{ElemPool, IndexError, PieList};
use core::iter::Extend;

/// A mutable view that bundles a `PieList` and its `ElemPool`.
///
/// This struct allows you to perform mutable operations (push, pop, clear, modify elements)
/// without repeatedly passing the pool as an argument. It holds exclusive mutable access
/// to both the list and the pool for its lifetime.
///
/// # Example
///
/// ```
/// use pie_core::{ElemPool, PieList};
///
/// let mut pool = ElemPool::new();
/// let mut list = PieList::new(&mut pool);
///
/// {
///     let mut view = list.view_mut(&mut pool);
///     view.push_back(10);
///     view.push_back(20);
///     
///     // Standard mutable iteration
///     for item in &mut view {
///         *item *= 2;
///     }
///     
///     view.clear();
/// }
///
/// assert!(list.is_empty());
/// ```
#[must_use]
pub struct PieViewMut<'a, T> {
    pub(crate) list: &'a mut PieList<T>,
    pub(crate) pool: &'a mut ElemPool<T>,
}

impl<'a, T> PieViewMut<'a, T> {
    /// Creates a new mutable view.
    pub fn new(list: &'a mut PieList<T>, pool: &'a mut ElemPool<T>) -> Self {
        Self { list, pool }
    }

    /// Returns the number of elements in the list.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(1);
    /// assert_eq!(view.len(), 1);
    /// # view.clear();
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
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// assert!(view.is_empty());
    /// view.push_back(1);
    /// assert!(!view.is_empty());
    /// # view.clear();
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Appends an element to the back of the list.
    ///
    /// # Panics
    /// Panics if the pool is out of memory (capacity overflow).
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(10);
    /// assert_eq!(view.back(), Some(&10));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn push_back(&mut self, elt: T) {
        self.list
            .push_back(elt, self.pool)
            .expect("Pool allocation failed");
    }

    /// Appends an element to the front of the list.
    ///
    /// # Panics
    /// Panics if the pool is out of memory (capacity overflow).
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_front(10);
    /// assert_eq!(view.front(), Some(&10));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn push_front(&mut self, elt: T) {
        self.list
            .push_front(elt, self.pool)
            .expect("Pool allocation failed");
    }

    /// Removes the last element from the list and returns it, or `None` if empty.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(10);
    /// assert_eq!(view.pop_back(), Some(10));
    /// assert_eq!(view.pop_back(), None);
    /// ```
    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        self.list.pop_back(self.pool)
    }

    /// Removes the first element from the list and returns it, or `None` if empty.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(10);
    /// assert_eq!(view.pop_front(), Some(10));
    /// assert_eq!(view.pop_front(), None);
    /// ```
    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        self.list.pop_front(self.pool)
    }

    /// Returns a reference to the front element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(42);
    /// assert_eq!(view.front(), Some(&42));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn front(&self) -> Option<&T> {
        self.list.front(self.pool)
    }

    /// Returns a mutable reference to the front element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(42);
    /// if let Some(x) = view.front_mut() {
    ///     *x = 100;
    /// }
    /// assert_eq!(view.front(), Some(&100));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.list.front_mut(self.pool)
    }

    /// Returns a reference to the back element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(99);
    /// assert_eq!(view.back(), Some(&99));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn back(&self) -> Option<&T> {
        self.list.back(self.pool)
    }

    /// Returns a mutable reference to the back element.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(99);
    /// *view.back_mut().unwrap() = 0;
    /// assert_eq!(view.back(), Some(&0));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.list.back_mut(self.pool)
    }

    /// Clears the list, returning all elements to the pool's free list.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(1);
    /// view.clear();
    /// assert!(view.is_empty());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.list.clear(self.pool);
    }

    /// Inserts a new element at the given logical index.
    ///
    /// # Errors
    /// Returns `IndexError` if `index > len`.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(10);
    /// view.push_back(30);
    /// view.insert(1, 20).unwrap();
    ///
    /// let items: Vec<_> = view.iter().copied().collect();
    /// assert_eq!(items, vec![10, 20, 30]);
    /// # view.clear();
    /// ```
    pub fn insert(&mut self, index: usize, element: T) -> Result<(), IndexError> {
        if index == self.len() {
            self.push_back(element);
            Ok(())
        } else {
            let mut cursor = self.list.cursor_mut_at(index, self.pool)?;
            cursor.insert_before(element, self.pool)
        }
    }

    /// Removes the element at the given logical index and returns it.
    ///
    /// # Errors
    /// Returns `IndexError` if `index >= len`.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(10);
    /// view.push_back(20);
    /// view.push_back(30);
    ///
    /// assert_eq!(view.remove(1).unwrap(), 20);
    /// assert_eq!(view.len(), 2);
    /// # view.clear();
    /// ```
    pub fn remove(&mut self, index: usize) -> Result<T, IndexError> {
        let mut cursor = self.list.cursor_mut_at(index, self.pool)?;
        // cursor.remove_current returns Option, but we know index is valid if cursor creation succeeded
        // and index < len. cursor_mut_at checks bounds.
        Ok(cursor
            .remove_current(self.pool)
            .expect("Logic error: Cursor valid but no element"))
    }

    /// Creates an immutable iterator over the list's elements.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.extend([1, 2, 3]);
    ///
    /// let vec: Vec<_> = view.iter().copied().collect();
    /// assert_eq!(vec, vec![1, 2, 3]);
    /// # view.clear();
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.list.iter(self.pool)
    }

    /// Creates a mutable iterator over the list's elements.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::new();
    /// # let mut list = PieList::new(&mut pool);
    /// let mut view = list.view_mut(&mut pool);
    /// view.push_back(1);
    ///
    /// for item in view.iter_mut() {
    ///     *item += 10;
    /// }
    /// assert_eq!(view.front(), Some(&11));
    /// # view.clear();
    /// ```
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        self.list.iter_mut(self.pool)
    }
}

// Allows `for x in view { ... }` (Consumes view, yields &mut T)
impl<'a, T> IntoIterator for PieViewMut<'a, T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.iter_mut(self.pool)
    }
}

// Allows `for x in &mut view { ... }` (Yields &mut T without consuming view)
impl<'a, 'b, T> IntoIterator for &'b mut PieViewMut<'a, T> {
    type Item = &'b mut T;
    type IntoIter = IterMut<'b, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

// Allows `view.extend(iter)`
impl<'a, T> Extend<T> for PieViewMut<'a, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ElemPool, PieList};

    #[test]
    fn test_view_mut_push_pop() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);

        view.push_back(10);
        view.push_front(20); // 20, 10
        view.push_back(30);  // 20, 10, 30

        assert_eq!(view.len(), 3);
        assert!(!view.is_empty());

        assert_eq!(view.pop_front(), Some(20));
        assert_eq!(view.pop_back(), Some(30));
        assert_eq!(view.pop_back(), Some(10));
        assert!(view.is_empty());
        
        view.clear(); // Safe clear
    }

    #[test]
    fn test_view_mut_references() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);

        view.push_back(10);
        view.push_back(20);

        if let Some(front) = view.front_mut() {
            *front = 11;
        }
        if let Some(back) = view.back_mut() {
            *back = 22;
        }

        assert_eq!(view.front(), Some(&11));
        assert_eq!(view.back(), Some(&22));
        
        view.clear();
    }

    #[test]
    fn test_view_mut_iteration() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);

        view.extend([1, 2, 3]);

        // Test non-consuming iter_mut via &mut view
        for item in &mut view {
            *item *= 10;
        }

        // Test iter()
        let vec: Vec<_> = view.iter().copied().collect();
        assert_eq!(vec, vec![10, 20, 30]);

        // Test consuming IntoIterator
        let mut count = 0;
        for _ in view {
            count += 1;
        }
        assert_eq!(count, 3);
        
        list.clear(&mut pool); // view consumed, clear list directly
    }

    #[test]
    fn test_view_mut_insert_remove() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);

        view.push_back(10);
        view.push_back(30);

        view.insert(1, 20).unwrap();
        assert_eq!(view.len(), 3);

        let removed = view.remove(1).unwrap();
        assert_eq!(removed, 20);
        assert_eq!(view.len(), 2);
        
        view.clear();
    }

    #[test]
    fn test_extend_from_vec() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);
        view.extend(vec![1, 2, 3, 4, 5]);
        assert_eq!(view.len(), 5);
        let items: Vec<_> = view.iter().copied().collect();
        assert_eq!(items, vec![1, 2, 3, 4, 5]);
        view.clear();
    }

    #[test]
    fn test_extend_from_range() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);
        view.extend(10..15);
        assert_eq!(view.len(), 5);
        let items: Vec<_> = view.iter().copied().collect();
        assert_eq!(items, vec![10, 11, 12, 13, 14]);
        view.clear();
    }

    #[test]
    fn test_extend_appends_to_existing() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);
        view.push_back(0);
        view.extend([1, 2]);
        assert_eq!(view.len(), 3);
        let items: Vec<_> = view.iter().copied().collect();
        assert_eq!(items, vec![0, 1, 2]);
        view.clear();
    }

    #[test]
    fn test_extend_empty_iterator() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        let mut view = list.view_mut(&mut pool);
        view.extend(core::iter::empty::<i32>());
        assert!(view.is_empty());
        view.clear();
    }
}
