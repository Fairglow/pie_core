//! Main implementation of the `PieList<T>` type.
//!
//! # Internal Architecture
//!
//! ## Sentinel-Based Design
//!
//! Each `PieList` has a dedicated sentinel node that:
//! - Contains no user data (state = SENTINEL)
//! - Has `next` pointing to the head, `prev` pointing to the tail
//! - Points to itself when the list is empty
//! - Eliminates null checks and edge cases in all operations
//!
//! ```text
//! Empty List:           List with [A, B, C]:
//!
//!   ┌───┐                 ┌───┐
//!   │ S │◄──────────►     │ S │
//!   └───┘                 └─┬─┘
//!                           │
//!           ┌───────────────┼───────────────┐
//!           ▼               │               ▼
//!         ┌───┐           ┌───┐           ┌───┐
//!         │ A │ ◄───────► │ B │ ◄───────► │ C │
//!         └───┘           └───┘           └───┘
//!           ▲                               │
//!           └───────────────────────────────┘
//! ```
//!
//! ## Pool-Centric API
//!
//! All operations require passing the `ElemPool` explicitly:
//! - `list.push_back(data, &mut pool)` not `list.push_back(data)`
//! - Clear ownership semantics, no hidden state
//! - Multiple lists can share the same pool efficiently
//!
//! ## Leak Detection (Debug Only)
//!
//! In debug builds, dropping a non-empty list triggers a panic. This catches
//! memory leaks where elements weren't returned to the pool. The sentinel
//! itself is allowed to leak (it gets cleaned up with the pool).
//!
//! ## Stable Sort Implementation
//!
//! The `sort_by()` method uses bottom-up iterative merge sort:
//!
//! 1. **Cascade Phase**: Build power-of-2 sorted runs bottom-up
//!    - Uses O(log n) temporary sentinels for merge lists
//!    - Merges same-sized runs immediately
//!
//! 2. **Final Merge**: Combine remaining runs into the original list
//!    - Iterates high→low slots to maintain stability
//!    - O(n log n) time, O(log n) space (for sentinels only)
//!
//! ## Memory Notes
//!
//! - `PieList<T>` is only 24 bytes (Index + len + debug flag)
//! - Creating a list allocates one sentinel element from the pool
//! - Moving a list is cheap (just copies the handle)
//! - Cloning a list creates a shallow copy sharing the same sentinel

// Allow unsafe for the performance-critical iterator implementation.
#![allow(unsafe_code)]

extern crate alloc;

use crate::{Cursor, CursorMut, ElemPool, Index, IndexError, IndexMap,
            PieView, PieViewMut};
use core::{cmp, marker::PhantomData};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

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
/// ⚠️ **WARNING: MEMORY LEAK RISK** ⚠️
///
/// When a `PieList` is dropped, the elements it references are **not** automatically
/// returned to the pool. This is a deliberate design choice to allow lists to be
/// moved and managed without unintended side effects on the pool.
///
/// To prevent memory leaks within the pool, you **must** call [`clear()`] or
/// [`drain()`] on a list when you are finished with it. This will iterate through
/// all its elements and return them to the pool's free list, making them
/// available for reuse.
///
/// [`clear()`]: PieList::clear
/// [`drain()`]: PieList::drain
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(bound = ""))]
pub struct PieList<T> {
    /// The index of the sentinel node for this list. The sentinel's `next`
    /// points to the head of the list, and its `prev` points to the tail.
    pub(crate) sentinel: Index<T>,
    /// The number of data elements in this list (excludes the sentinel).
    pub(crate) len: usize,
    #[cfg(debug_assertions)]
    check_leak: bool,
}

impl<T> Clone for PieList<T> {
    #[inline]
    fn clone(&self) -> Self {
        PieList {
            sentinel: self.sentinel,
            len: self.len,
            #[cfg(debug_assertions)]
            check_leak: self.check_leak,
        }
    }
}

#[cfg(debug_assertions)]
impl<T> Drop for PieList<T> {
    fn drop(&mut self) {
        // 1. Check if we are already panicking.
        // If a test fails, we don't want to trigger THIS panic,
        // as it hides the original error message.
        #[cfg(feature = "std")]
        if std::thread::panicking() {
            return;
        }
        // 2. This is a safety check for development builds. If a `PieList` is
        // dropped while it still contains elements, those elements will be
        // leaked within the `ElemPool` because they are never returned to the
        // free list. This assert helps catch such cases.
        if self.check_leak {
            debug_assert!(
                self.is_empty(),
                "PieList dropped while not empty, causing a memory leak. You must call \
                .clear() or .drain() before the list goes out of scope."
            );
        }
    }
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
        // Convert the allocated ZOMBIE element to a SENTINEL
        let sentinel = pool
            .index_make_sentinel(sentinel)
            .expect("Failed to convert element to sentinel");
        // The list is created empty, so the sentinel initially points to itself.
        #[cfg(debug_assertions)]
        { Self { sentinel, len: 0, check_leak: true } }
        #[cfg(not(debug_assertions))]
        Self { sentinel, len: 0 }
    }

    #[allow(unused_mut)]
    pub fn without_leak_check(mut self) -> Self {
        #[cfg(debug_assertions)]
        { self.check_leak = false; }
        self
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    #[inline]
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
    #[inline]
    pub fn push_front(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<Index<T>, IndexError> {
        let new_idx = pool.index_new_with_data(data)?;
        pool.index_link_after(new_idx, self.sentinel)?;
        self.len += 1;
        Ok(new_idx)
    }

    /// Adds an element to the back of the list.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns an `IndexError` if the pool is unable to allocate a new element.
    #[inline]
    pub fn push_back(&mut self, data: T, pool: &mut ElemPool<T>) -> Result<Index<T>, IndexError> {
        let new_idx = pool.index_new_with_data(data)?;
        pool.index_link_before(new_idx, self.sentinel)?;
        self.len += 1;
        Ok(new_idx)
    }

    /// Removes the first element and returns its data, or `None` if the list is empty.
    ///
    /// The removed element's node is returned to the pool's free list.
    ///
    /// # Complexity
    /// O(1)
    #[inline]
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
    #[inline]
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

    /// Enable/disable the leak check for this list in debug builds
    ///
    /// NOTE: No leak check is performed in release builds
    pub fn set_leak_check(&mut self, _leak_check: bool) {
        #[cfg(debug_assertions)]
        { self.check_leak = _leak_check; }
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

    /// Creates a draining iterator that removes all elements from the list and
    /// yields them from front to back.
    ///
    /// The removed nodes are returned to the pool's free list.
    ///
    /// # Note
    ///
    /// If the iterator is dropped before it is fully consumed, it will still
    /// remove the remaining elements from the list to ensure that the list is
    /// left empty.
    ///
    /// # Complexity
    /// O(n), where n is the number of elements in the list. Each element is
    /// visited once.
    ///
    /// # Example
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::<i32>::new();
    /// let mut list = PieList::new(&mut pool);
    /// list.push_back(1, &mut pool).unwrap();
    /// list.push_back(2, &mut pool).unwrap();
    /// list.push_back(3, &mut pool).unwrap();
    ///
    /// let drained_items: Vec<_> = list.drain(&mut pool).collect();
    ///
    /// assert_eq!(drained_items, vec![1, 2, 3]);
    /// assert!(list.is_empty());
    /// assert_eq!(pool.len(), 0); // All elements returned to pool
    /// ```
    pub fn drain<'a>(&'a mut self, pool: &'a mut ElemPool<T>) -> Drain<'a, T> {
        let front = pool.next(self.sentinel);
        let back = pool.prev(self.sentinel);
        let len = self.len;

        // Immediately clear the list's own state. The Drain iterator now owns
        // the responsibility of cleaning up the nodes.
        pool.get_mut(self.sentinel)
            .unwrap()
            .new_links(self.sentinel, self.sentinel);
        self.len = 0;

        Drain {
            pool,
            front,
            back,
            len,
            _phantom: PhantomData,
        }
    }

    /// Sorts the list in place using a stable merge sort algorithm.
    ///
    /// # Complexity
    ///
    /// O(n log n) comparisons, where `n` is the number of elements in the list.
    /// The merge operations are done in-place without new allocations from the pool.
    ///
    /// # Implementation
    ///
    /// Uses an optimized bottom-up iterative merge sort that allocates at most
    /// O(log n) temporary sentinel nodes, which are reused across all merge
    /// operations. This is significantly more efficient than a naive recursive
    /// approach that would allocate O(n) temporary sentinels.
    ///
    /// # Example
    ///
    /// ```
    /// # use pie_core::{ElemPool, PieList};
    /// # let mut pool = ElemPool::<i32>::new();
    /// let mut list = PieList::new(&mut pool);
    /// list.push_back(5, &mut pool).unwrap();
    /// list.push_back(2, &mut pool).unwrap();
    /// list.push_back(8, &mut pool).unwrap();
    /// list.push_back(1, &mut pool).unwrap();
    ///
    /// // Sort in ascending order
    /// list.sort(&mut pool, |a, b| a.cmp(b));
    ///
    /// let sorted: Vec<_> = list.iter(&pool).copied().collect();
    /// assert_eq!(sorted, vec![1, 2, 5, 8]);
    /// # list.clear(&mut pool);
    /// ```
    pub fn sort<F>(&mut self, pool: &mut ElemPool<T>, mut compare: F)
    where
        F: FnMut(&T, &T) -> cmp::Ordering,
    {
        // A list of 0 or 1 elements is already sorted.
        if self.len() < 2 {
            return;
        }

        // Bottom-up iterative merge sort using a stack of sorted runs.
        // Each slot i holds a sorted run of size 2^i, or is empty.
        // This limits allocations to O(log n) temporary sentinels that are reused.
        // 64 slots can handle lists up to 2^64 elements (practically unlimited).
        const MAX_STACK_SIZE: usize = 64;
        let mut stack: [Option<PieList<T>>; MAX_STACK_SIZE] = core::array::from_fn(|_| None);

        // Pre-allocate sentinel nodes for the stack slots we'll need.
        // For a list of length n, we need at most ceil(log2(n)) + 1 sentinels.
        let needed_sentinels = (usize::BITS - self.len().leading_zeros()) as usize;
        let mut temp_sentinels: alloc::vec::Vec<Index<T>> = alloc::vec::Vec::with_capacity(needed_sentinels);
        for _ in 0..needed_sentinels {
            let sentinel = pool.index_new().expect("Pool failed to allocate temp sentinel");
            let sentinel = pool.index_make_sentinel(sentinel).expect("Failed to make sentinel");
            temp_sentinels.push(sentinel);
        }
        let mut next_sentinel_idx = 0;

        // Process each element as a run of size 1.
        while !self.is_empty() {
            // Pop the front element into a new single-element run.
            let front_node = pool.next(self.sentinel);
            pool.index_linkout(front_node).unwrap();
            self.len -= 1;

            // Get or reuse a sentinel for this new run.
            let run_sentinel = if next_sentinel_idx < temp_sentinels.len() {
                let s = temp_sentinels[next_sentinel_idx];
                next_sentinel_idx += 1;
                s
            } else {
                // Fallback: allocate a new sentinel if we somehow need more.
                let s = pool.index_new().expect("Pool failed to allocate sentinel");
                pool.index_make_sentinel(s).expect("Failed to make sentinel")
            };

            // Create a run containing just this one element.
            pool.index_link_after(front_node, run_sentinel).unwrap();
            let mut run = PieList {
                sentinel: run_sentinel,
                len: 1,
                #[cfg(debug_assertions)]
                check_leak: false,
            };

            // Cascade merges: while the current stack slot is occupied,
            // merge and move up to the next slot.
            // For stability: `existing` contains earlier elements, `run` contains later elements.
            // Merge `run` INTO `existing` so that `existing` elements come first when equal.
            let mut slot = 0;
            while slot < MAX_STACK_SIZE {
                match stack[slot].take() {
                    None => {
                        // Empty slot - place our run here.
                        stack[slot] = Some(run);
                        break;
                    }
                    Some(mut existing) => {
                        // Merge the later run into the earlier existing run for stability.
                        // Return the current run's sentinel to the reuse pool.
                        let old_sentinel = run.sentinel;
                        existing.merge(run, pool, &mut compare);
                        run = existing;
                        // Mark the old sentinel as reusable by resetting it.
                        let _ = pool.get_mut(old_sentinel).map(|e| e.new_links(old_sentinel, old_sentinel));
                        temp_sentinels.push(old_sentinel);
                        slot += 1;
                    }
                }
            }
        }

        // Merge all remaining runs in the stack into the final sorted list.
        // Higher slots contain elements that were processed earlier (they've been
        // sitting in the stack longer). Iterate from high to low so that we
        // accumulate earlier elements first, then merge later elements into them.
        let mut result: Option<PieList<T>> = None;
        for slot in (0..MAX_STACK_SIZE).rev() {
            if let Some(run) = stack[slot].take() {
                match result.take() {
                    None => result = Some(run),
                    Some(mut existing) => {
                        // `existing` (from higher slots) contains earlier elements,
                        // `run` (from lower slots) contains later elements.
                        existing.merge(run, pool, &mut compare);
                        result = Some(existing);
                    }
                }
            }
        }

        // Move the result back into self.
        if let Some(sorted) = result {
            // Swap the sentinel and length, then clean up our temporary sentinel.
            let old_sentinel = self.sentinel;

            // Move data from sorted into self.
            self.sentinel = sorted.sentinel;
            self.len = sorted.len;

            // Return the old sentinel to the free list.
            let _ = pool.data_swap(old_sentinel, None);
            let _ = pool.index_del(old_sentinel);
        }

        // Clean up any remaining temporary sentinels.
        for &sentinel in &temp_sentinels {
            // Only delete if not currently in use (i.e., not self.sentinel).
            if sentinel != self.sentinel {
                let _ = pool.data_swap(sentinel, None);
                let _ = pool.index_del(sentinel);
            }
        }

        #[cfg(debug_assertions)]
        { self.check_leak = true; }
    }

    /// Merges two sorted lists. `self` is assumed to be one sorted list,
    /// and `other` is the second. After the operation, `self` will contain
    /// all elements from both lists in sorted order, and `other` will be empty.
    fn merge<F>(&mut self, mut other: PieList<T>, pool: &mut ElemPool<T>, compare: &mut F)
    where
        F: FnMut(&T, &T) -> cmp::Ordering,
    {
        // If the other list is empty, there's nothing to do.
        if other.is_empty() {
            return;
        }
        // If this list is empty, we can perform an O(1) splice to take other's elements.
        if self.is_empty() {
            self.splice(self.sentinel, &mut other, pool).unwrap();
            return;
        }
        // The current node in `self` that we are comparing against.
        let mut current_self_node = pool.next(self.sentinel);
        // Loop as long as there are elements to compare in both lists.
        while !other.is_empty() && current_self_node != self.sentinel {
            // These unwraps are safe because the loop conditions guarantee both lists
            // have at least one element and that current_self_node is not the sentinel.
            let self_data = pool.data(current_self_node).unwrap();
            let other_data = other.front(pool).unwrap();
            // If the `other` node is smaller or equal, move it into `self`.
            // The equality check is crucial for maintaining a stable sort.
            if compare(other_data, self_data) == cmp::Ordering::Less {
                let node_to_move = pool.next(other.sentinel);
                // Unlink the node from the front of `other`.
                pool.index_linkout(node_to_move).unwrap();
                other.len -= 1;
                // Link it into `self` right before the current node.
                pool.index_link_before(node_to_move, current_self_node)
                    .unwrap();
                self.len += 1;
            } else {
                // The `self` node is smaller, so it's in the correct place.
                // Advance to the next node in `self` for the next comparison.
                current_self_node = pool.next(current_self_node);
            }
        }
        // If `other` still has elements, they are all larger than any in `self`.
        // We can efficiently splice the remainder onto the end of `self`.
        if !other.is_empty() {
            self.splice(self.sentinel, &mut other, pool).unwrap();
        }
    }

    /// Splits the list before the given `split_node`. The original list (`self`) will
    /// contain all elements from `split_node` onwards, and a new list containing
    /// elements before `split_node` is returned.
    pub(crate) fn split_off(
        &mut self,
        split_node: Index<T>,
        split_len: usize, // The length of the new list being returned
        pool: &mut ElemPool<T>,
    ) -> Result<PieList<T>, IndexError> {
        let original_len = self.len();
        if split_len == 0 {
            return Ok(PieList::new(pool));
        }
        let mut new_list = PieList::new(pool);
        let original_front = pool.next(self.sentinel);
        let element_before_split = pool.prev(split_node);
        // Form the new list: (new_sentinel) <-> original_front <-> ... <-> element_before_split <-> (new_sentinel)
        pool.get_mut(new_list.sentinel)?
            .new_links(element_before_split, original_front);
        pool.get_mut(original_front)?.new_prev(new_list.sentinel);
        pool.get_mut(element_before_split)?
            .new_next(new_list.sentinel);
        // Form the now-shortened original list: (self.sentinel) <-> split_node <-> ...
        pool.get_mut(self.sentinel)?.new_next(split_node);
        pool.get_mut(split_node)?.new_prev(self.sentinel);
        self.len = original_len - split_len;
        new_list.len = split_len;
        Ok(new_list)
    }

    /// Splices the `other` list into `self` before `insertion_node`.
    pub(crate) fn splice(
        &mut self,
        insertion_node: Index<T>,
        other: &mut PieList<T>,
        pool: &mut ElemPool<T>,
    ) -> Result<(), IndexError> {
        let other_len = other.len;
        let other_sentinel = other.sentinel;
        let element_before_cursor = pool.prev(insertion_node);
        let other_first = pool.next(other_sentinel);
        let other_last = pool.prev(other_sentinel);
        pool.get_mut(element_before_cursor)?.new_next(other_first);
        pool.get_mut(other_first)?.new_prev(element_before_cursor);
        pool.get_mut(insertion_node)?.new_prev(other_last);
        pool.get_mut(other_last)?.new_next(insertion_node);
        self.len += other_len;
        other.len = 0;
        pool.get_mut(other_sentinel)?
            .new_links(other_sentinel, other_sentinel);
        Ok(())
    }

    /// Moves all elements from `other` to the end of `self`.
    ///
    /// After the operation, `other` is left empty.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns an `IndexError` if the pool's internal linking fails,
    /// though this is highly unlikely if the lists are valid.
    pub fn append(
        &mut self,
        other: &mut PieList<T>,
        pool: &mut ElemPool<T>,
    ) -> Result<(), IndexError> {
        // Splice the 'other' list in just before 'self's sentinel node.
        self.splice(self.sentinel, other, pool)
    }

    /// Moves all elements from `other` to the beginning of `self`.
    ///
    /// After the operation, `other` is left empty.
    ///
    /// # Complexity
    /// O(1)
    ///
    /// # Errors
    /// Returns an `IndexError` if the pool's internal linking fails,
    /// though this is highly unlikely if the lists are valid.
    pub fn prepend(
        &mut self,
        other: &mut PieList<T>,
        pool: &mut ElemPool<T>,
    ) -> Result<(), IndexError> {
        // Find the first element of 'self'
        let first_elem = pool.next(self.sentinel);
        // Splice the 'other' list in just before 'self's first element.
        self.splice(first_elem, other, pool)
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

    /// Creates an immutable view of the list using the provided pool.
    ///
    /// The view bundles the list and pool together, offering a simplified API
    /// for read-only operations.
    pub fn view<'a>(&'a self, pool: &'a ElemPool<T>) -> PieView<'a, T> {
        PieView::new(self, pool)
    }

    /// Creates a mutable view of the list using the provided pool.
    ///
    /// The view bundles the list and pool together, offering a simplified API
    /// for mutable operations (push, pop, etc.).
    pub fn view_mut<'a>(&'a mut self, pool: &'a mut ElemPool<T>) -> PieViewMut<'a, T> {
        PieViewMut::new(self, pool)
    }

    /// Returns a cursor pointing to the first element of the list.
    ///
    /// The cursor allows for bidirectional navigation.
    pub fn cursor<'a>(&'a self, pool: &'a ElemPool<T>) -> Cursor<'a, T> {
        let first_elem = pool.next(self.sentinel);
        Cursor::new(self, first_elem, 0)
    }

    /// Returns a cursor pointing to the element at the given logical index.
    ///
    /// # Complexity
    /// O(min(k, n-k)) traversal.
    ///
    /// # Errors
    /// Returns `Err(IndexError::IndexOutOfBounds)` if `index >= self.len()`.
    pub fn cursor_at<'a>(
        &'a self,
        index: usize,
        pool: &'a ElemPool<T>,
    ) -> Result<Cursor<'a, T>, IndexError> {
        if index >= self.len {
            return Err(IndexError::IndexOutOfBounds);
        }
        let mut current_idx;
        if index < self.len / 2 {
            current_idx = self.sentinel;
            for _ in 0..=index {
                current_idx = pool.next(current_idx);
            }
        } else {
            current_idx = self.sentinel;
            for _ in 0..(self.len - index) {
                current_idx = pool.prev(current_idx);
            }
        }
        Ok(Cursor::new(self, current_idx, index))
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

    /// Updates the list's internal sentinel index if it was affected by a `shrink_to_fit` operation.
    ///
    /// This method checks the provided remapping table to see if the sentinel node
    /// for this list was moved to a new index. If so, it updates the `PieList`
    /// handle to point to the new location.
    ///
    /// # Complexity
    /// O(1) - It performs a single hash map lookup.
    pub fn remap(&mut self, map: &IndexMap<Index<T>, Index<T>>) {
        // We check if our specific sentinel index is in the map of moved nodes.
        if let Some(&new_index) = map.get(&self.sentinel) {
            self.sentinel = new_index;
        }
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

impl<'a, T> ExactSizeIterator for Iter<'a, T> {
    fn len(&self) -> usize {
        self.len
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

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {
    fn len(&self) -> usize {
        self.len
    }
}

/// A draining iterator for a `PieList`.
///
/// This struct is created by the [`drain()`] method on [`PieList`].
/// See its documentation for more.
///
/// [`drain()`]: PieList::drain
pub struct Drain<'a, T: 'a> {
    pool: &'a mut ElemPool<T>,
    front: Index<T>,
    back: Index<T>,
    len: usize,
    _phantom: PhantomData<T>,
}

impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let current_idx = self.front;
        self.front = self.pool.next(current_idx);
        self.len -= 1;

        // The drain constructor already unlinked the entire chain of nodes from
        // the list's sentinel, so we don't need to `index_linkout` here. We
        // just need to consume the chain and deallocate the nodes.
        let data = self.pool.data_swap(current_idx, None);
        self.pool.index_del(current_idx).unwrap();

        data
    }
}

impl<'a, T> DoubleEndedIterator for Drain<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let current_idx = self.back;
        self.back = self.pool.prev(current_idx);
        self.len -= 1;

        let data = self.pool.data_swap(current_idx, None);
        self.pool.index_del(current_idx).unwrap();

        data
    }
}

impl<'a, T> ExactSizeIterator for Drain<'a, T> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<'a, T> Drop for Drain<'a, T> {
    fn drop(&mut self) {
        // Drain any remaining elements to prevent leaking them in the pool.
        for _ in self {}
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
    fn test_clear_debug() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(1, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        assert_eq!(list.len(), 2);
        list.clear(&mut pool);
        assert!(list.is_empty());
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
        assert!(list.is_empty());
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
        list.clear(&mut pool);
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
        list.clear(&mut pool);
    }

    #[test]
    fn test_drain() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(1, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        list.push_back(3, &mut pool).unwrap();

        assert_eq!(list.len(), 3);
        assert_eq!(pool.len(), 3);

        {
            let mut drain = list.drain(&mut pool);
            assert_eq!(drain.next(), Some(1));
            assert_eq!(drain.next_back(), Some(3));
            assert_eq!(drain.next(), Some(2));
            assert_eq!(drain.next(), None);
            assert_eq!(drain.next_back(), None);
        } // drain is dropped here

        assert!(list.is_empty());
        assert_eq!(pool.len(), 0); // Elements were freed
    }

    #[test]
    fn test_drain_drop() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();
        list.push_back(30, &mut pool).unwrap();
        list.push_back(40, &mut pool).unwrap();

        {
            let mut drain = list.drain(&mut pool);
            // Only take one element
            assert_eq!(drain.next(), Some(10));
            // Drop the drain iterator without consuming the rest.
        }

        // The Drop impl should have cleared the rest.
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        // Pool should be empty as well.
        assert_eq!(pool.len(), 0);
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
        list1.clear(&mut pool);
        list2.clear(&mut pool);
    }

    #[test]
    fn test_sort() {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);

        // Sort empty list
        list.sort(&mut pool, |a: &i32, b| a.cmp(b));
        assert!(list.is_empty());

        // Sort single-element list
        list.push_back(10, &mut pool).unwrap();
        list.sort(&mut pool, |a, b| a.cmp(b));
        assert_eq!(*list.front(&pool).unwrap(), 10);
        list.clear(&mut pool);

        // Sort multi-element list
        list.push_back(5, &mut pool).unwrap();
        list.push_back(2, &mut pool).unwrap();
        list.push_back(8, &mut pool).unwrap();
        list.push_back(1, &mut pool).unwrap();

        list.sort(&mut pool, |a, b| a.cmp(b));
        let sorted: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(sorted, vec![1, 2, 5, 8]);

        // Sort already-sorted list
        list.sort(&mut pool, |a, b| a.cmp(b));
        let sorted2: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(sorted2, vec![1, 2, 5, 8]);

        // Sort reverse-sorted list
        list.sort(&mut pool, |a, b| b.cmp(a));
        let sorted3: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(sorted3, vec![8, 5, 2, 1]);
        list.clear(&mut pool);
    }

    #[test]
    fn test_sort_stability() {
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        struct Item {
            key: i32,
            val: char,
        }
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);

        list.push_back(Item { key: 2, val: 'a' }, &mut pool).unwrap();
        list.push_back(Item { key: 1, val: 'b' }, &mut pool).unwrap();
        list.push_back(Item { key: 2, val: 'c' }, &mut pool).unwrap();
        list.push_back(Item { key: 0, val: 'd' }, &mut pool).unwrap();
        list.push_back(Item { key: 1, val: 'e' }, &mut pool).unwrap();

        // Sort by key. The relative order of items with the same key should be preserved.
        list.sort(&mut pool, |a, b| a.key.cmp(&b.key));

        let sorted: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(
            sorted,
            vec![
                Item { key: 0, val: 'd' },
                Item { key: 1, val: 'b' },
                Item { key: 1, val: 'e' }, // 'b' before 'e'
                Item { key: 2, val: 'a' },
                Item { key: 2, val: 'c' }, // 'a' before 'c'
            ]
        );
        list.clear(&mut pool);
    }
}
