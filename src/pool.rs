//! A generic pool allocator for a multi-headed doubly-linked index-list.

use std::fmt;
use crate::Index;
use crate::elem::ListElem;

const FREE_SENTINEL_NDX: u32 = 0;

/// An error type for the detailed validation of an `Index<T>`.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexError {
    IndexIsNone,
    IndexOutOfBounds,
    ElementIsFree,
    ElementIsFreeSentinel,
    BrokenPrevLink,
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

/// A pool of `ListElem<T>` nodes for building multiple lists of type `T`.
#[derive(Clone, Debug)]
pub struct ElemPool<T> {
    elems: Vec<ListElem<T>>,
    freed: usize,
}

impl<T> Default for ElemPool<T> {
    fn default() -> Self {
        let sentinel_index = Index::from(FREE_SENTINEL_NDX);
        let mut sentinel_elem = ListElem::default();
        let _ = sentinel_elem.new_links(sentinel_index, sentinel_index);

        Self {
            elems: vec![sentinel_elem],
            freed: 0,
        }
    }
}

impl<T> ElemPool<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.elems.len() - self.freed - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.elems.len() - 1
    }

    #[inline]
    pub fn contains(&self, index: Index<T>) -> bool {
        index.get()
            .and_then(|n| self.elems.get(n))
            .map(|e| e.is_used())
            .unwrap_or(false)
    }
    #[inline]
    pub fn validate_index(&self, index: Index<T>) -> Result<(), IndexError> {
        let ndx = index.get().ok_or(IndexError::IndexIsNone)?;
        let elem = self.elems.get(ndx).ok_or(IndexError::IndexOutOfBounds)?;
        if !elem.is_used() {
            return Err(IndexError::ElementIsFree);
        }
        let prev_ndx = elem.prev.get().ok_or(IndexError::BrokenPrevLink)?;
        let prev_elem = self.elems.get(prev_ndx).ok_or(IndexError::BrokenPrevLink)?;
        if prev_elem.next != index {
            return Err(IndexError::BrokenPrevLink);
        }
        let next_ndx = elem.next.get().ok_or(IndexError::BrokenNextLink)?;
        let next_elem = self.elems.get(next_ndx).ok_or(IndexError::BrokenNextLink)?;
        if next_elem.prev != index {
            return Err(IndexError::BrokenNextLink);
        }
        Ok(())
    }
    pub fn index_new(&mut self) -> Result<Index<T>, IndexError> {
        let free_sentinel_ndx = Index::from(FREE_SENTINEL_NDX);
        let ndx_to_reuse = self.next(free_sentinel_ndx);
        assert!(ndx_to_reuse.is_some());
        if ndx_to_reuse != free_sentinel_ndx {
            self.index_linkout(ndx_to_reuse)?;
            self.freed -= 1;
            Ok(ndx_to_reuse)
        } else {
            // The free list was empty, so we allocate a new element.
            // Any previously freed elements have now been reused.
            self.freed = 0;
            let ndx = Index::from(self.elems.len());
            let mut new_elem = ListElem::default();
            let _ = new_elem.new_links(ndx, ndx);
            self.elems.push(new_elem);
            Ok(ndx)
        }
    }
    pub fn index_del(&mut self, index: Index<T>) -> Result<(), IndexError> {
        let free_sentinel_ndx = Index::from(FREE_SENTINEL_NDX);
        if index == free_sentinel_ndx {
            return Err(IndexError::ElementIsFreeSentinel);
        }
        self.validate_index(index)?; // TODO: Maybe just contains?
        self.index_linkout(index)?;
        let _ = self.data_swap(index, None);
        self.index_link_after(index, free_sentinel_ndx)?;
        self.freed += 1;
        Ok(())
    }
    #[inline]
    pub fn get(&self, index: Index<T>) -> Result<&ListElem<T>, IndexError> {
        let n = index.get().ok_or(IndexError::IndexIsNone)?;
        self.elems.get(n).ok_or(IndexError::IndexOutOfBounds)
    }
    #[inline]
    pub fn get_mut(&mut self, index: Index<T>) -> Result<&mut ListElem<T>, IndexError> {
        let n = index.get().ok_or(IndexError::IndexIsNone)?;
        self.elems.get_mut(n).ok_or(IndexError::IndexOutOfBounds)
    }
    #[inline]
    pub fn next(&self, index: Index<T>) -> Index<T> {
        self.get(index).map(|i| i.next).unwrap_or_default()
    }
    #[inline]
    pub fn prev(&self, index: Index<T>) -> Index<T> {
        self.get(index).map(|i| i.prev).unwrap_or_default()
    }
    #[inline]
    pub fn data(&self, index: Index<T>) -> Option<&T> {
        self.get(index).ok().and_then(|i| i.data.as_ref())
    }
    #[inline]
    pub fn data_mut(&mut self, index: Index<T>) -> Option<&mut T> {
        self.get_mut(index).ok().and_then(|i| i.data.as_mut())
    }
    #[inline]
    pub fn data_swap(&mut self, index: Index<T>, data: Option<T>) -> Option<T> {
        self.get_mut(index).ok().and_then(|i| i.new_data(data))
    }
    #[inline]
    pub fn index_linkout(&mut self, index: Index<T>) -> Result<(), IndexError> {
        let (prev_ndx, next_ndx) = self.get_mut(index)?.new_links(index, index);
        let opn = self.get_mut(prev_ndx)?.new_next(next_ndx);
        let onp = self.get_mut(next_ndx)?.new_prev(prev_ndx);
        assert_eq!(opn, onp);
        Ok(())
    }
    #[inline]
    pub fn index_link_after(&mut self, this: Index<T>, after: Index<T>) -> Result<(), IndexError> {
        let next_ndx = self.get_mut(after)?.new_next(this);
        let _ = self.get_mut(this)?.new_links(after, next_ndx);
        let onp = self.get_mut(next_ndx)?.new_prev(this);
        assert_eq!(onp, after);
        Ok(())
    }
    #[inline]
    pub fn index_link_before(&mut self, this: Index<T>, before: Index<T>) -> Result<(), IndexError> {
        let prev_ndx = self.get_mut(before)?.new_prev(this);
        let _ = self.get_mut(this)?.new_links(prev_ndx, before);
        let opn = self.get_mut(prev_ndx)?.new_next(this);
        assert_eq!(opn, before);
        Ok(())
    }
}

impl<T> fmt::Display for ElemPool<T>
where T: fmt::Display {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "ElemPool used {}/{}, {} free:", self.len(), self.capacity(), self.freed)?;
        for (i, elem) in self.elems.iter().enumerate() {
            writeln!(f, "  [{}]: {}", i, elem)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a pool and add some elements for testing.
    fn create_pool_with_elems<T>(count: usize, default_data: T) -> (ElemPool<T>, Vec<Index<T>>)
    where
        T: Clone,
    {
        let mut pool = ElemPool::new();
        let mut indices = Vec::new();
        for _i in 0..count {
            let index = pool.index_new().unwrap();
            let _ = pool.get_mut(index).unwrap().new_data(Some(default_data.clone()));
            indices.push(index);
        }
        (pool, indices)
    }

    #[test]
    fn test_pool_creation() {
        let pool: ElemPool<i32> = ElemPool::new();
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.capacity(), 0);
        assert!(pool.is_empty());
        // The vector should contain only the free list sentinel.
        assert_eq!(pool.elems.len(), 1);
        // The sentinel should point to itself.
        let sentinel_index = Index::from(FREE_SENTINEL_NDX);
        assert_eq!(pool.next(sentinel_index), sentinel_index);
        assert_eq!(pool.prev(sentinel_index), sentinel_index);
    }

    #[test]
    fn test_index_new_and_len() {
        let (pool, indices) = create_pool_with_elems(3, 100);
        assert_eq!(pool.len(), 3);
        assert_eq!(pool.capacity(), 3);
        assert!(!pool.is_empty());
        assert_eq!(indices.len(), 3);
        // Indices should be 1, 2, 3 since 0 is the sentinel.
        assert_eq!(indices[0].get(), Some(1));
        assert_eq!(indices[1].get(), Some(2));
        assert_eq!(indices[2].get(), Some(3));
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
    fn test_del_and_reuse() {
        let (mut pool, indices) = create_pool_with_elems(5, 0);
        assert_eq!(pool.len(), 5);
        assert_eq!(pool.freed, 0);

        // Delete an element from the middle.
        let deleted_index = indices[2]; // Index should be 3.
        pool.index_del(deleted_index).unwrap();

        assert_eq!(pool.len(), 4);
        assert_eq!(pool.freed, 1);
        assert!(!pool.contains(deleted_index));

        // The free list sentinel should now point to our deleted element.
        let free_sentinel_ndx = Index::from(FREE_SENTINEL_NDX);
        assert_eq!(pool.next(free_sentinel_ndx), deleted_index);

        // Allocate a new element, it should reuse the deleted index.
        let reused_index = pool.index_new().unwrap();
        assert_eq!(reused_index, deleted_index);
        assert_eq!(pool.len(), 5);
        assert_eq!(pool.freed, 0); // Freed count goes back down.
        assert!(pool.get(reused_index).unwrap().data.is_none()); // new_index doesn't set data.
    }

    #[test]
    fn test_del_errors() {
        let (mut pool, indices) = create_pool_with_elems(1, 0);

        // Can't delete the sentinel
        assert_eq!(
            pool.index_del(Index::from(FREE_SENTINEL_NDX)),
            Err(IndexError::ElementIsFreeSentinel)
        );

        // Can't delete an invalid index
        pool.index_del(indices[0]).unwrap();
        assert_eq!(
            pool.index_del(indices[0]), // Delete again
            Err(IndexError::ElementIsFree)
        );
    }

    #[test]
    fn test_linking_logic() {
        let (mut pool, indices) = create_pool_with_elems(3, 0);
        let i1 = indices[0];
        let i2 = indices[1];
        let i3 = indices[2];

        // Initially, all elements point to themselves.
        assert_eq!(pool.next(i1), i1);
        assert_eq!(pool.prev(i1), i1);

        // Form a list: i1 <-> i2 <-> i3
        pool.index_link_after(i2, i1).unwrap();
        pool.index_link_after(i3, i2).unwrap();

        // Check forward links
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        // To make it circular for this test, we link i3 back to i1
        // In a real list, this would point to a list head sentinel
        pool.get_mut(i3).unwrap().new_next(i1);
        pool.get_mut(i1).unwrap().new_prev(i3);

        // Check backward links
        assert_eq!(pool.prev(i1), i3);
        assert_eq!(pool.prev(i3), i2);
        assert_eq!(pool.prev(i2), i1);

        // Now, link out the middle element (i2)
        pool.index_linkout(i2).unwrap();

        // i2 should now point to itself
        assert_eq!(pool.next(i2), i2);
        assert_eq!(pool.prev(i2), i2);

        // i1 should now link directly to i3
        assert_eq!(pool.next(i1), i3);
        assert_eq!(pool.prev(i3), i1);

        // Link i2 back in, but using link_before
        pool.index_link_before(i2, i3).unwrap();

        // Should be back to i1 <-> i2 <-> i3
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.prev(i3), i2);
        assert_eq!(pool.prev(i2), i1);
    }

    #[test]
    fn test_validate_index() {
        let (mut pool, indices) = create_pool_with_elems(3, 0);
        let i1 = indices[0];
        let i2 = indices[1];
        let i3 = indices[2];

        // Link them up: i1 <-> i2 <-> i3
        pool.index_link_after(i2, i1).unwrap();
        pool.index_link_after(i3, i2).unwrap();

        // A valid, linked element should be Ok
        assert_eq!(pool.validate_index(i2), Ok(()));

        // Check various error conditions
        assert_eq!(pool.validate_index(Index::NONE), Err(IndexError::IndexIsNone));
        assert_eq!(pool.validate_index(Index::from(99_u32)), Err(IndexError::IndexOutOfBounds));

        pool.index_del(i2).unwrap();
        assert_eq!(pool.validate_index(i2), Err(IndexError::ElementIsFree));

        // Manually break a link to test for inconsistency
        let (mut pool, indices) = create_pool_with_elems(3, 0);
        let i1 = indices[0];
        let i2 = indices[1];
        let i3 = indices[2];
        pool.index_link_after(i2, i1).unwrap();
        pool.index_link_after(i3, i2).unwrap();

        // Break i1's next link (it should point to i2, let's make it point to i3)
        // This is "unsafe" but necessary to test the validator.
        pool.get_mut(i1).unwrap().next = i3;

        // Now, validating i2 should fail because its prev (i1) doesn't point back to it.
        assert_eq!(pool.validate_index(i2), Err(IndexError::BrokenPrevLink));

        // And validating i1 should fail because its next (i3) doesn't have a prev that points back to i1.
        assert_eq!(pool.validate_index(i1), Err(IndexError::BrokenNextLink));    }
}
