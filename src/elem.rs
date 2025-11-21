//! Definition of the generic `ListElem<T>` type.

use crate::index::Index;
use std::{fmt, mem};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// The fundamental node structure for a doubly-linked list.
///
/// Each `ListElem` contains optional data and indices pointing to the
/// `next` and `prev` elements in its list. This struct is the unit of
/// allocation within the `ElemPool`.
///
/// # Rationale
///
/// By embedding `Option<T>` directly, we colocate the list's structural
/// information with the user's data. This improves cache performance for
/// lists where `T` is reasonably small. A `data` value of `None` signifies
/// that this element is either a sentinel node for a list or is currently
/// on the pool's free list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ListElem<T> {
    /// The index of the next element in the list.
    pub(crate) next: Index<T>,
    /// The index of the previous element in the list.
    pub(crate) prev: Index<T>,
    /// The data stored in this element.
    /// `None` signifies a free or a sentinel list node.
    pub(crate) data: Option<T>,
}

impl<T> Default for ListElem<T> {
    /// Creates a default `ListElem` with no data and invalid links.
    ///
    /// This is the initial state for a newly allocated element before it is
    /// linked into a list or the free list.
    fn default() -> Self {
        Self {
            next: Index::NONE,
            prev: Index::NONE,
            data: None,
        }
    }
}

impl<T> ListElem<T> {
    /// A builder-style method to set the data for this element.
    #[inline]
    pub fn with_data(mut self, data: T) -> Self {
        self.data = Some(data);
        self
    }

    /// A builder-style method to set both `next` and `prev` links to the same index.
    /// This is useful for initializing a self-referential sentinel or a standalone element.
    #[inline]
    pub fn with_both(mut self, index: Index<T>) -> Self {
        self.next = index;
        self.prev = index;
        self
    }

    /// Checks if the element is in use (i.e., contains user data).
    /// Returns `false` for free nodes and list sentinels.
    #[inline]
    pub fn is_used(&self) -> bool {
        self.data.is_some()
    }

    /// Replaces the `next` index with a new one, returning the old index.
    #[inline]
    pub fn new_next(&mut self, next: Index<T>) -> Index<T> {
        mem::replace(&mut self.next, next)
    }

    /// Replaces the `prev` index with a new one, returning the old index.
    #[inline]
    pub fn new_prev(&mut self, prev: Index<T>) -> Index<T> {
        mem::replace(&mut self.prev, prev)
    }

    /// Replaces the `data` with a new `Option<T>`, returning the old data.
    #[inline]
    pub fn new_data(&mut self, data: Option<T>) -> Option<T> {
        mem::replace(&mut self.data, data)
    }

    /// Replaces both `prev` and `next` links, returning the old pair.
    #[inline]
    pub fn new_links(&mut self, prev: Index<T>, next: Index<T>) -> (Index<T>, Index<T>) {
        let old_prev = mem::replace(&mut self.prev, prev);
        let old_next = mem::replace(&mut self.next, next);
        (old_prev, old_next)
    }

    /// Returns a tuple of the `(prev, next)` links.
    #[inline]
    pub fn links(&self) -> (Index<T>, Index<T>) {
        (self.prev, self.next)
    }
}

impl<T: fmt::Display> fmt::Display for ListElem<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.data {
            Some(data) => write!(f, "{}<-{}->{}", self.prev, data, self.next),
            None => write!(f, "{}<-()->{}", self.prev, self.next),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    // A simple struct for testing purposes.
    // Deriving Debug and PartialEq is useful for assertions.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct MyElemData {
        value: i32,
    }

    #[test]
    fn test_default_creation() {
        let elem = ListElem::<MyElemData>::default();
        assert_eq!(elem.next, Index::NONE);
        assert_eq!(elem.prev, Index::NONE);
        assert!(elem.data.is_none());
        assert!(!elem.is_used());
    }

    #[test]
    fn test_builder_methods() {
        let data = MyElemData { value: 100 };
        // Use the correct generic type for the Index to match the element.
        let index = Index::<MyElemData>::from(42_u32);

        // Test with_data
        let elem_with_data = ListElem::default().with_data(data);
        assert_eq!(elem_with_data.data, Some(data));
        assert!(elem_with_data.is_used());
        assert_eq!(elem_with_data.next, Index::NONE); // Other fields unchanged

        // Test with_both. Now the types match.
        let elem_with_links = ListElem::default().with_both(index);
        assert_eq!(elem_with_links.next, index);
        assert_eq!(elem_with_links.prev, index);
        assert!(!elem_with_links.is_used()); // Data field unchanged
    }
    #[test]
    fn test_is_used() {
        let mut elem = ListElem::<MyElemData>::default();
        assert!(!elem.is_used());

        elem.data = Some(MyElemData { value: 1 });
        assert!(elem.is_used());

        elem.data = None;
        assert!(!elem.is_used());
    }

    #[test]
    fn test_mutator_methods() {
        let mut elem = ListElem::<MyElemData>::default();
        let index1 = Index::from(1_u32);
        let index2 = Index::from(2_u32);
        let data1 = Some(MyElemData { value: 10 });
        let data2 = Some(MyElemData { value: 20 });

        // Test new_next
        let old_next = elem.new_next(index1);
        assert_eq!(old_next, Index::NONE);
        assert_eq!(elem.next, index1);

        // Test new_prev
        let old_prev = elem.new_prev(index2);
        assert_eq!(old_prev, Index::NONE);
        assert_eq!(elem.prev, index2);

        // Test new_data
        let old_data = elem.new_data(data1);
        assert_eq!(old_data, None);
        assert_eq!(elem.data, data1);

        let old_data_2 = elem.new_data(data2);
        assert_eq!(old_data_2, data1);
        assert_eq!(elem.data, data2);
    }

    #[test]
    fn test_new_links() {
        let mut elem = ListElem::<MyElemData>::default();
        let index3 = Index::from(3_u32);
        let index4 = Index::from(4_u32);

        // Setup the "old" values before the call
        let old_prev = elem.new_prev(Index::from(98_u32));
        let old_next = elem.new_next(Index::from(99_u32));
        // Sanity check: old_prev and old_next should be NONE initially
        assert!(old_prev.is_none());
        assert!(old_next.is_none());

        // Call the function under test
        let (returned_prev, returned_next) = elem.new_links(index3, index4);

        // Assert that the returned values are the ones we just replaced (98 and 99)
        assert_eq!(returned_prev, Index::from(98_u32));
        assert_eq!(returned_next, Index::from(99_u32));

        // Verify the internal state was updated correctly with the new values.
        assert_eq!(elem.prev, index3);
        assert_eq!(elem.next, index4);
    }

    #[test]
    fn test_links_getter() {
        let index1 = Index::from(10_u32);
        let index2 = Index::from(20_u32);
        let mut elem = ListElem::<MyElemData>::default();
        elem.prev = index1;
        elem.next = index2;

        let (p, n) = elem.links();
        assert_eq!(p, index1);
        assert_eq!(n, index2);
    }

    #[test]
    fn test_formatting() {
        // We need T to implement Display for this test.
        let mut elem = ListElem::<i32>::default();
        elem.prev = Index::from(1_u32);
        elem.next = Index::from(2_u32);

        // Test without data
        let display_no_data = format!("{}", elem);
        assert_eq!(display_no_data, "1<-()->2");

        // Test with data
        elem.data = Some(99);
        let display_with_data = format!("{}", elem);
        assert_eq!(display_with_data, "1<-99->2");

        // Test with NONE indices
        let mut elem2 = ListElem::<i32>::default();
        elem2.data = Some(5);
        let display_none_links = format!("{}", elem2);
        assert_eq!(display_none_links, "-<-5->-");
    }
}
