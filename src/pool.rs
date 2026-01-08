//! A generic pool allocator for multi-headed doubly-linked lists.
//!
//! # Internal Architecture
//!
//! `ElemPool<T>` is a generational arena that provides memory for all data
//! structures in this library. The key design decisions are:
//!
//! ## Memory Layout
//!
//! ```text
//! ElemPool<T>
//! ┌─────────────────────────────────────────────────────────────┐
//! │ elems: Vec<Elem<T>>                                         │
//! │ ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┐    │
//! │ │  0  │  1  │  2  │  3  │  4  │  5  │  6  │  7  │  8  │    │
//! │ │Free │ S₁  │ A   │ B   │ S₂  │ X   │ Y   │Free │Free │    │
//! │ │Sent.│     │     │     │     │     │     │     │     │    │
//! │ └─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┘    │
//! │                                                             │
//! │ freed: 2 (slots 7, 8 are free)                              │
//! │ used: 5  (slots 2,3,5,6 have data; S₁,S₂ are sentinels)     │
//! └─────────────────────────────────────────────────────────────┘
//!
//! Free List:  [0] ↔ [7] ↔ [8] ↔ [0]  (circular, slot 0 is sentinel)
//! List 1:     [1] ↔ [2] ↔ [3] ↔ [1]  (S₁ sentinel, A↔B data)
//! List 2:     [4] ↔ [5] ↔ [6] ↔ [4]  (S₂ sentinel, X↔Y data)
//! ```
//!
//! ## Slot 0 is Reserved
//!
//! The element at index 0 is always the **free list sentinel**. It never
//! holds user data. Its `next`/`prev` links form the circular free list.
//! This simplifies allocation/deallocation (no empty-list edge cases).
//!
//! ## Generational Indexing (ABA Protection)
//!
//! Each element tracks a generation counter. When freed and reused, the
//! generation increments. Old handles become "stale" — they point to the
//! right slot but have the wrong generation, so `get()` fails safely:
//!
//! ```text
//! Time 0: alloc slot 5 → handle {slot:5, vers:3}
//! Time 1: free slot 5  → element 5 generation becomes 4
//! Time 2: alloc slot 5 → handle {slot:5, vers:5}
//!
//! Old handle {slot:5, vers:3} → get() returns None (stale)
//! New handle {slot:5, vers:5} → get() returns Some(&data)
//! ```
//!
//! ## Two-Phase Deletion
//!
//! Deletion is split into data removal and element recycling:
//!
//! 1. `data_swap(handle, None)` — Take data out (element → Zombie state)
//! 2. `index_del(handle)` — Return element to free list (Zombie → Free)
//!
//! This separation enables complex operations like FibHeap's `pop()` which
//! must manipulate node links after extracting data but before freeing.
//!
//! ## Shrink-to-Fit Strategy
//!
//! When `shrink_to_fit()` is called, free elements at the end of the Vec
//! are removed. Since this invalidates slot numbers, a remapping table is
//! returned so data structures can update their handles. The algorithm:
//!
//! 1. Build remap: old_slot → new_slot (O(capacity) scan)
//! 2. Swap-remove free elements from the end
//! 3. Update all next/prev links using remap
//! 4. Return remap for external handle updates

use crate::elem::Elem;
use crate::Index;
use crate::IndexMap;
use alloc::{slice, vec, vec::Vec};
use core::{error, fmt, mem::MaybeUninit};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// An error type representing failures in list operations.
///
/// These errors typically arise from providing an invalid `Index` to a pool
/// method, such as one that is out of bounds or points to an already-freed element.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexError {
    /// A consistency check failed: an element's `next` link does not point back correctly.
    BrokenNextLink,
    /// A consistency check failed: an element's `prev` link does not point back correctly.
    BrokenPrevLink,
    /// The element at the index is on the pool's free list and cannot be used.
    ElementIsFree,
    /// An attempt was made to operate on the free list's own sentinel node.
    ElementIsFreeSentinel,
    /// The provided index was `Index::NONE`.
    IndexIsNone,
    /// The index generation does not match the element's generation.
    IndexIsStale,
    /// The provided index exceeds the bounds of the pool's element vector.
    IndexOutOfBounds,
}

impl error::Error for IndexError {}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BrokenNextLink => write!(f, "Element's next link is inconsistent"),
            Self::BrokenPrevLink => write!(f, "Element's previous link is inconsistent"),
            Self::ElementIsFree => write!(f, "Element is on the free list"),
            Self::ElementIsFreeSentinel => write!(f, "Element is the free list sentinel"),
            Self::IndexIsNone => write!(f, "Index is NONE"),
            Self::IndexIsStale => write!(f, "Index is stale (generation mismatch)"),
            Self::IndexOutOfBounds => write!(f, "Index is out of bounds"),
        }
    }
}

/// A pool of `ListElem<T>` nodes that provides memory for multiple data structures.
///
/// # Rationale
///
/// The `ElemPool` is the cornerstone of this library's design. It acts as a
/// specialized memory allocator. By pre-allocating memory in a `Vec` and managing
/// its own free list, it avoids the performance cost of frequent calls to the
/// global allocator. This makes creating and destroying elements extremely fast.
///
/// All list elements, regardless of which `PieList` or `FibHeap` they belong to,
/// are stored contiguously within this single structure, leading to better cache
/// locality during traversals compared to traditional node-based linked lists.
///
/// Its public API is minimal, as most interactions are performed through `PieList`,
/// `CursorMut`, or `FibHeap` methods, which take the pool as an argument.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ElemPool<T> {
    /// The contiguous storage for all list elements (nodes).
    elems: Vec<Elem<T>>,
    /// A count of elements currently in the free list.
    freed: usize,
    /// The number of elements that contain user data (`Some(T)`).
    /// This count excludes all sentinel nodes and free elements.
    used: usize,
}

impl<T> Default for ElemPool<T> {
    /// Creates a new `ElemPool`, initialized with a single sentinel element
    /// for its internal free list.
    fn default() -> Self {
        let sentinel_index = Self::free_sentinel_index();
        let mut sentinel_elem = Elem::default();
        // Force the sentinel state (state 10)
        sentinel_elem.force_sentinel();
        let _ = sentinel_elem.new_links(sentinel_index, sentinel_index);
        Self {
            elems: vec![sentinel_elem],
            freed: 0,
            used: 0,
        }
    }
}

// --- Public API ---
impl<T> ElemPool<T> {
    /// Creates a new, empty element pool.
    ///
    /// The pool is initialized with a capacity for zero elements but contains
    /// one internal node to act as the sentinel for the free list.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the fixed index for the free list's sentinel node, which is always 0.
    #[inline(always)]
    fn free_sentinel_index() -> Index<T> {
        Index::from(0u32)
    }

    /// Returns the number of elements holding user data in the pool.
    ///
    /// This is a semantic count that excludes sentinel nodes for active lists
    /// and any elements on the free list. It provides a clear measure of how
    /// many items are actually being stored across all lists.
    #[inline]
    pub fn len(&self) -> usize {
        self.used
    }

    /// Returns `true` if the pool contains no user data.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    /// Returns the total number of elements (used, sentinels, or free) that the pool
    /// can hold without reallocating its internal vector. This count excludes the
    /// pool's own free-list sentinel.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.elems.len() - 1
    }

    /// Returns the number of free elements in the pool.
    #[inline]
    pub fn free_len(&self) -> usize {
        self.freed
    }

    /// Returns the number of active lists associated with this pool.
    ///
    /// This is calculated by subtracting the number of data-holding elements and
    /// free elements from the total capacity, with the remainder being the
    /// sentinel nodes for active lists.
    #[inline]
    pub fn list_count(&self) -> usize {
        self.capacity() - self.used - self.freed
    }

    /// Reserves capacity for at least `additional` more elements to be
    /// allocated in the pool.
    ///
    /// The pool's underlying storage may reallocate if its capacity is
    /// less than the current length plus `additional`. If the capacity is
    /// already sufficient, this does nothing.
    ///
    /// This is useful to avoid multiple reallocations when a large number
    /// of elements are expected to be added.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity overflows `isize::MAX`.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.elems.reserve(additional);
    }

    /// Checks if a given index points to an element that contains user data.
    ///
    /// Returns `false` if the index is `NONE`, out of bounds, or points to a
    /// free/sentinel element.
    #[inline]
    pub fn contains(&self, index: Index<T>) -> bool {
        index
            .get()
            .and_then(|n| self.elems.get(n))
            .is_some_and(|e| e.is_used())
    }

    /// Performs a detailed validation of an index and its surrounding links.
    ///
    /// This is a powerful debugging tool to verify the structural integrity of a list.
    /// It checks that:
    /// 1. The index is valid and in bounds.
    /// 2. The element at the index contains data.
    /// 3. The `prev` element's `next` link points back to this index.
    /// 4. The `next` element's `prev` link points back to this index.
    ///
    /// # Errors
    /// Returns `Ok(())` on success, or an `IndexError` variant describing the
    /// first validation failure encountered.
    #[inline]
    pub fn validate_index(&self, index: Index<T>) -> Result<(), IndexError> {
           let ndx = index.get().ok_or(IndexError::IndexIsNone)?;
           let elem = self.elems.get(ndx).ok_or(IndexError::IndexOutOfBounds)?;
           // 1. Generation Check (ABA Protection)
           if elem.vers != index.vers {
               return Err(IndexError::IndexIsStale);
           }
        if elem.is_free() {
            return Err(IndexError::ElementIsFree);
        }
        let prev_ndx = elem.prev.get().ok_or(IndexError::BrokenPrevLink)?;
        let prev_elem = self
            .elems
            .get(prev_ndx)
            .ok_or(IndexError::BrokenPrevLink)?;
        if prev_elem.next != index {
            return Err(IndexError::BrokenPrevLink);
        }
        let next_ndx = elem.next.get().ok_or(IndexError::BrokenNextLink)?;
        let next_elem = self
            .elems
            .get(next_ndx)
            .ok_or(IndexError::BrokenNextLink)?;
        if next_elem.prev != index {
            return Err(IndexError::BrokenNextLink);
        }
        Ok(())
    }

    /// Returns an iterator over the pool's raw elements.
    ///
    /// This is primarily used internally (e.g. by `FibHeap`) to perform operations
    /// that require traversing the entire pool structure, such as remapping internal
    /// pointers after a `shrink_to_fit` operation.
    pub fn iter(&self) -> slice::Iter<'_, Elem<T>> {
        self.elems.iter()
    }

    /// Returns a mutable iterator over the pool's raw elements.
    ///
    /// This is primarily used internally (e.g. by `FibHeap`) to perform operations
    /// that require traversing the entire pool structure, such as remapping internal
    /// pointers after a `shrink_to_fit` operation.
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, Elem<T>> {
        self.elems.iter_mut()
    }

    /// Allocates a new index, reusing a free element if available or creating a new one.
    ///
    /// This is the primary method for acquiring a new node from the pool. It
    /// first checks the free list. If the free list is not empty, it unlinks
    /// and returns the first available node. If the free list is empty, it
    /// pushes a new `ListElem` to the end of the internal `Vec`.
    pub(crate) fn index_new(&mut self) -> Result<Index<T>, IndexError> {
        let free_sentinel_ndx = Self::free_sentinel_index();
        // Use raw access to sentinel to avoid version checks on the sentinel itself
        let next_free = self.elems[0].next;
        if next_free != free_sentinel_ndx {
            // Free list is not empty, reuse an element.
            self.index_linkout(next_free)?;
            let slot = next_free.slot;
            let elem = &mut self.elems[slot as usize];
            // Bump generation and mark as ZOMBIE (Used but no data).
            let new_vers = elem.bump_gen(crate::elem::STATE_ZOMBIE);            self.freed -= 1;
            Ok(Index::new(slot, new_vers))
        } else {
            // Free list is empty, allocate a new element.
            let slot = self.elems.len() as u32;
            let mut new_elem = Elem::default();
            // It starts as Free (default). Transition to ZOMBIE.
            let new_vers = new_elem.bump_gen(crate::elem::STATE_ZOMBIE);            let ndx = Index::new(slot, new_vers);
            let _ = new_elem.new_links(Index::NONE, Index::NONE);
            self.elems.push(new_elem);
            Ok(ndx)
        }
    }

    /// Converts a ZOMBIE element to a SENTINEL element.
    ///
    /// This is used when allocating a sentinel for a list. The element must
    /// be in ZOMBIE state (just allocated via `index_new()`). The element's
    /// state is changed to SENTINEL and the index's version is updated to match.
    ///
    /// Returns a new Index with the updated version, or an error if the
    /// provided index does not match the element at that location.
    pub(crate) fn index_make_sentinel(&mut self, index: Index<T>) -> Result<Index<T>, IndexError> {
        let elem = self.get_mut(index)?;
        // Element should be ZOMBIE (just allocated via index_new)
        if !elem.is_zombie() {
            return Err(IndexError::ElementIsFree);
        }
        // Transition state from ZOMBIE to SENTINEL
        // Keep the generation, just change the state bits
        elem.vers = (elem.vers & !crate::elem::STATE_MASK) | crate::elem::STATE_SENTINEL;
        let new_vers = elem.vers;
        // Update the sentinel's self-references to use the new version
        let sentinel_idx = Index::new(index.slot, new_vers);
        elem.next = sentinel_idx;
        elem.prev = sentinel_idx;
        Ok(sentinel_idx)
    }

    /// Allocates a new index and initializes it with data, returning the correct Index
    /// with the USED state version.
    ///
    /// This combines `index_new()` and `data_swap()` to ensure the returned Index
    /// has the correct version for a USED element.
    pub(crate) fn index_new_with_data(&mut self, data: T) -> Result<Index<T>, IndexError> {
        let new_idx = self.index_new()?;
        // Get the element and set it to USED state
        let slot = new_idx.slot as usize;
        let elem = self.elems.get_mut(slot).ok_or(IndexError::IndexOutOfBounds)?;
        if !elem.is_zombie() {
            return Err(IndexError::ElementIsFree);
        }
        elem.data = MaybeUninit::new(data);
        elem.vers = (elem.vers & !crate::elem::STATE_MASK) | crate::elem::STATE_USED;
        let vers = elem.vers;
        self.used += 1;
        Ok(Index::new(new_idx.slot, vers))
    }

    /// Returns an index to the free list.
    ///
    /// The caller must ensure the element has already been unlinked from any
    /// active list and that its data has been taken. This method links the
    /// element at the given `index` to the front of the free list.
    pub(crate) fn index_del(&mut self, index: Index<T>) -> Result<(), IndexError> {
        if index.slot == 0 {
            return Err(IndexError::ElementIsFreeSentinel);
        }
        // 1. Validate & Get Mutable (Checks Generation)
        let slot = index.slot;
        let elem = self.elems.get_mut(slot as usize).ok_or(IndexError::IndexOutOfBounds)?;
        // Strict check:
        // elem.vers must match index.vers (Normal case)
        // OR elem.vers must match index.vers converted to Zombie (Data swapped case)
        let is_zombie_match = elem.is_zombie() &&
             ((elem.vers & !crate::elem::STATE_MASK) == (index.vers & !crate::elem::STATE_MASK)) &&
             ((index.vers & crate::elem::STATE_MASK) == crate::elem::STATE_USED);

        if elem.vers != index.vers && !is_zombie_match {

            return Err(IndexError::IndexIsStale);
        }
        // 2. Transition State: Used -> Free (Increments Generation)
        let new_vers = elem.make_free();
        // 3. Link into Free List
        // We must create a new Index handle because the version just changed!
        let new_free_idx = Index::new(slot, new_vers);
        let free_sentinel_ndx = Self::free_sentinel_index();
        // Pass our *new* valid index to be linked.
        self.index_link_after(new_free_idx, free_sentinel_ndx)?;
        self.freed += 1;
        Ok(())
    }

    /// Gets an immutable reference to the `ListElem` at the given index.
    #[inline]
    pub(crate) fn get(&self, index: Index<T>) -> Result<&Elem<T>, IndexError> {
        let n = index.get().ok_or(IndexError::IndexIsNone)?;
        let elem = self.elems.get(n).ok_or(IndexError::IndexOutOfBounds)?;
        if elem.vers != index.vers {
            return Err(IndexError::IndexIsStale);
        }
        Ok(elem)
    }

    /// Gets a mutable reference to the `ListElem` at the given index.
    #[inline]
    pub(crate) fn get_mut(&mut self, index: Index<T>) -> Result<&mut Elem<T>, IndexError> {
        let n = index.get().ok_or(IndexError::IndexIsNone)?;
        let elem = self.elems.get_mut(n).ok_or(IndexError::IndexOutOfBounds)?;
        if elem.vers != index.vers {
            return Err(IndexError::IndexIsStale);
        }
        Ok(elem)
    }

    /// Gets the `next` index for the element at the given index.
    #[inline]
    pub(crate) fn next(&self, index: Index<T>) -> Index<T> {
        // `unwrap_or_default` returns `Index::NONE` on error.
        self.get(index).map(|i| i.next).unwrap_or_default()
    }

    /// Gets the `prev` index for the element at the given index.
    #[inline]
    pub(crate) fn prev(&self, index: Index<T>) -> Index<T> {
        self.get(index).map(|i| i.prev).unwrap_or_default()
    }

    /// Gets an immutable reference to the data inside the element at the given index.
    #[inline]
    pub(crate) fn data(&self, index: Index<T>) -> Option<&T> {
        self.get(index).ok().and_then(|i|
            if i.is_used() {
                #[allow(unsafe_code)]
                Some(unsafe { i.data.assume_init_ref() })
            } else { None }
        )
    }

    /// Gets a mutable reference to the data inside the element at the given index.
    #[inline]
    pub(crate) fn data_mut(&mut self, index: Index<T>) -> Option<&mut T> {
        self.get_mut(index).ok().and_then(|i|
            if i.is_used() {
                #[allow(unsafe_code)]
                Some(unsafe { i.data.assume_init_mut() })
            } else { None }
        )
    }

    /// Swaps the data in an element and updates the pool's `used` count accordingly.
    ///
    /// This is the sole method responsible for modifying an element's data, as it
    /// correctly maintains the pool's `used` counter.
    #[inline]
    pub(crate) fn data_swap(&mut self, index: Index<T>, data: Option<T>) -> Option<T> {
        let elem = self.get_mut(index).ok()?;
        if let Some(new_data) = data {
            // Replacing data
            let prev = elem.replace_data(new_data);
            if prev.is_none() { self.used += 1; }
            prev
        } else {
            // Taking data out (Zombie transition)
            let prev = elem.take_data();
            if prev.is_some() { self.used -= 1; }
            prev
        }
    }

    /// Unlinks an element from its current position in a list.
    /// After this operation, the element points to itself.
    #[inline]
    pub(crate) fn index_linkout(&mut self, index: Index<T>) -> Result<(), IndexError> {
        let (prev_ndx, next_ndx) = self.get_mut(index)?.new_links(index, index);
        let free_sentinel_ndx = Self::free_sentinel_index();

        if prev_ndx.slot == free_sentinel_ndx.slot {
            self.elems[0].next = next_ndx;
        } else {
            self.get_mut(prev_ndx)?.new_next(next_ndx);
        }

        if next_ndx.slot == free_sentinel_ndx.slot {
            self.elems[0].prev = prev_ndx;
        } else {
            self.get_mut(next_ndx)?.new_prev(prev_ndx);
        }
        Ok(())
    }

    /// Links `this` element immediately after the `after` element.
    #[inline]
    pub(crate) fn index_link_after(
        &mut self,
        this: Index<T>,
        after: Index<T>,
    ) -> Result<(), IndexError> {
        let free_sentinel_ndx = Self::free_sentinel_index();

        // Special case: if 'after' is the free sentinel, use raw access to avoid version checks
        let next_ndx = if after.slot == free_sentinel_ndx.slot {
            let next = self.elems[0].next;
            self.elems[0].next = this;  // Update sentinel's next pointer
            next
        } else {
            self.get_mut(after)?.new_next(this)
        };

        let _ = self.get_mut(this)?.new_links(after, next_ndx);

        // Fix neighbor pointing back. Handle sentinel case explicitly.
        if next_ndx.slot == free_sentinel_ndx.slot {
            self.elems[0].prev = this;
        } else if next_ndx.is_some() {
            self.get_mut(next_ndx)?.new_prev(this);
        }
        Ok(())
    }

    /// Links `this` element immediately before the `before` element.
    #[inline]
    pub(crate) fn index_link_before(
        &mut self,
        this: Index<T>,
        before: Index<T>,
    ) -> Result<(), IndexError> {
        let free_sentinel_ndx = Self::free_sentinel_index();

        // Special case: if 'before' is the free sentinel, use raw access to avoid version checks
        let prev_ndx = if before.slot == free_sentinel_ndx.slot {
            let prev = self.elems[0].prev;
            self.elems[0].prev = this;  // Update sentinel's prev pointer
            prev
        } else {
            self.get_mut(before)?.new_prev(this)
        };

        let _ = self.get_mut(this)?.new_links(prev_ndx, before);

        // Fix neighbor pointing back. Handle sentinel case explicitly.
        if prev_ndx.slot == free_sentinel_ndx.slot {
            self.elems[0].next = this;
        } else if prev_ndx.is_some() {
            self.get_mut(prev_ndx)?.new_next(this);
        }
        Ok(())
    }

    /// Compacts the pool by moving elements from the end of the internal vector
    /// into free slots at the beginning.
    ///
    /// This implementation relies on the internal free list to identify holes
    /// efficiently, avoiding a full scan of the pool's lower bounds.
    ///
    /// # Performance
    ///
    /// The algorithm is O(f) where f is the number of freed elements, plus O(m)
    /// for fixing neighbor pointers where m is the number of moved elements.
    /// Memory usage is O(f) for the temporary data structures.
    ///
    /// For large pools with many freed elements, the implementation uses:
    /// - A compact boolean array for tracking free slots in the tail region
    /// - Pre-sized collections to minimize allocations
    /// - A slot-indexed remapping array (instead of hash lookups) for O(1) neighbor resolution
    pub fn shrink_to_fit(&mut self) -> IndexMap<Index<T>, Index<T>> {
        let old_len = self.elems.len();
        // The target length is simply total items minus the count of free items.
        // Note: self.len() is used items, self.freed is free items.
        // old_len includes both PLUS sentinels.
        // So target_len = old_len - self.freed.
        let target_len = old_len - self.freed;
        // If we are already compact, return empty map.
        if target_len == old_len {
            return IndexMap::new();
        }

        let tail_len = old_len - target_len; // == self.freed

        // 1. Identify Vacancies and Tag Tail-Free items.
        // We need a way to quickly check if an item in the tail is free.
        // Since the tail size is exactly equal to self.freed, we can allocate
        // a boolean map for just the tail section.
        // Map index i -> vector index (target_len + i)
        let mut is_free_tail = vec![false; tail_len];
        let mut vacancies = Vec::with_capacity(tail_len);
        let free_sentinel = Self::free_sentinel_index();

        // Start from the first free element directly to avoid version check on Stale sentinel (0@0 vs 0@2)
        let mut current_free = self.elems[0].next;

        while current_free != free_sentinel {
            let idx = current_free.get().unwrap();
            if idx < target_len {
                // This is a hole in the preserved region. We must fill it.
                vacancies.push(idx);
            } else {
                // This is a hole in the region we are cutting off.
                // We mark it so the tail-scanner knows to ignore it.
                is_free_tail[idx - target_len] = true;
            }
            current_free = self.next(current_free);
        }

        // 2. Build slot-indexed remapping array for O(1) lookups.
        // This is more efficient than hash map lookups for neighbor resolution.
        // For elements in the tail region, store their new destination slot.
        // u32::MAX means "not remapped" (either free or not in tail).
        let mut slot_remap = vec![u32::MAX; tail_len];

        // Pre-size the result map with expected capacity.
        let num_moved = tail_len - vacancies.len().min(tail_len);
        let mut remapping = IndexMap::with_capacity(num_moved);

        // 3. Move Used Items from Tail to Head
        // We iterate the tail region. Any item NOT marked as free is implicitly
        // a "Used" item (either User Data or a List Sentinel).
        for source in target_len..old_len {
            let tail_idx = source - target_len;
            if is_free_tail[tail_idx] {
                continue; // It's a free node in the tail; it will be truncated.
            }
            // It is a used node. Pop a vacancy to move it to.
            // Safety: The math guarantees vacancies.len() > 0 because
            // number of used items in tail == number of free items in head.
            let dest = vacancies.pop().expect("Logic Error: Mismatch in free/used counts");

            // Record in the slot remap array for O(1) neighbor resolution.
            slot_remap[tail_idx] = dest as u32;

            // Capture the version from the element being moved.
            let vers = self.elems[source].vers;
            self.elems.swap(dest, source);

            let old_idx = Index::new(source as u32, vers);
            let new_idx = Index::new(dest as u32, vers);
            remapping.insert(old_idx, new_idx);

            // 4. Fix Neighbors (The Graph Patching)
            // The node at `dest` currently thinks its neighbors are pointing to `source`.
            // We must update those neighbors to point to `dest`.
            let (prev_idx, next_idx) = self.elems[dest].links();

            // Helper: resolve an index to its effective slot after remapping.
            // Uses O(1) array lookup for tail items instead of hash map.
            let resolve = |idx: Index<T>, remap: &[u32], tgt_len: usize| -> Index<T> {
                let slot = idx.slot as usize;
                if slot >= tgt_len && slot < old_len {
                    let new_slot = remap[slot - tgt_len];
                    if new_slot != u32::MAX {
                        return Index::new(new_slot, idx.vers);
                    }
                }
                idx
            };

            // Fix prev's next pointer
            let effective_prev = resolve(prev_idx, &slot_remap, target_len);
            if let Ok(elem) = self.get_mut(effective_prev) {
                elem.next = new_idx;
            }

            // Fix next's prev pointer
            let effective_next = resolve(next_idx, &slot_remap, target_len);
            if let Ok(elem) = self.get_mut(effective_next) {
                elem.prev = new_idx;
            }
        }

        // 5. Final Cleanup
        // Truncate the vector
        self.elems.truncate(target_len);
        // Reset pool state
        self.freed = 0;
        // Reset the free list sentinel to point to itself (empty list)
        if let Some(sentinel) = self.elems.get_mut(0) {
             let _ = sentinel.new_links(free_sentinel, free_sentinel);
        }
        remapping
    }
}

impl<T> fmt::Display for ElemPool<T>
where T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "ElemPool used {}/{}, {} free:",
            self.len(),
            self.capacity(),
            self.freed
        )?;
        for (i, elem) in self.elems.iter().enumerate() {
            writeln!(f, "  [{}]: {}", i, elem)?;
        }
        Ok(())
    }
}

impl<T> Drop for ElemPool<T> {
    fn drop(&mut self) {
        for elem in self.elems.iter_mut() {
            // We only drop data if the element is effectively "Used".
            // Note: Zombies have already had their data taken/dropped.
            // Sentinels do not contain data.
            // Free nodes do not contain data.
            if elem.is_used() {
                // SAFETY: We are in the Drop impl, so no one else can access this data.
                #[allow(unsafe_code)]
                unsafe { elem.data.assume_init_drop(); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;
    use super::*;
    use crate::list::PieList;

    // Helper function to create a pool and add some elements for testing.
    fn create_pool_with_elems<T>(count: usize, default_data: T) -> (ElemPool<T>, Vec<Index<T>>)
    where T: Clone,
    {
        let mut pool = ElemPool::new();
        let mut indices = Vec::new();
        for _i in 0..count {
            let index = pool.index_new_with_data(default_data.clone()).unwrap();
            indices.push(index);
        }
        (pool, indices)
    }

    #[test]
    fn test_pool_creation_and_len() {
        let pool: ElemPool<i32> = ElemPool::new();
        assert_eq!(pool.len(), 0); // Should have 0 used elements
        assert_eq!(pool.capacity(), 0);
        assert!(pool.is_empty());
        assert_eq!(pool.elems.len(), 1); // Internal vec has free sentinel
    }

    #[test]
    fn test_index_new_and_len() {
        let (pool, indices) = create_pool_with_elems(3, 100);
        assert_eq!(pool.len(), 3); // 3 used elements
        assert_eq!(pool.capacity(), 3);
        assert!(!pool.is_empty());
        assert_eq!(indices.len(), 3);
        assert_eq!(indices[0].get(), Some(1));
        assert_eq!(indices[1].get(), Some(2));
        assert_eq!(indices[2].get(), Some(3));
    }

    #[test]
    fn test_del_and_reuse() {
        let (mut pool, indices) = create_pool_with_elems(5, 0);
        assert_eq!(pool.len(), 5);
        assert_eq!(pool.freed, 0);

        let deleted_index = indices[2];
        // To delete an element from the pool, we must first remove its data.
        let _data = pool.data_swap(deleted_index, None);
        // data_swap automatically decrements pool.len()
        assert_eq!(pool.len(), 4);

        // Now we can return the data-less element to the free list
        pool.index_del(deleted_index).unwrap();
        assert_eq!(pool.freed, 1);
        assert!(!pool.contains(deleted_index));
        // The free sentinel points to the newly freed element.
        // We access the sentinel directly to avoid version mismatch issues with the public helper.
        let first_free = pool.elems[0].next;
        assert_eq!(first_free.slot, deleted_index.slot);

        // Allocate a new element, it should reuse the deleted index.
        let reused_index = pool.index_new().unwrap();
        assert_eq!(reused_index.slot, deleted_index.slot);
        assert_ne!(reused_index.vers, deleted_index.vers);
        // The pool's used count is still 4 because the new element has no data yet
        assert_eq!(pool.len(), 4);
        assert_eq!(pool.freed, 0);

        // Add data to the reused element, length should go up
        pool.data_swap(reused_index, Some(999));
        assert_eq!(pool.len(), 5);
    }

    #[test]
    fn test_del_errors() {
        let mut pool: ElemPool<i32> = ElemPool::new();
        let index = pool.index_new().unwrap();
        pool.data_swap(index, Some(100));

        // Can't delete the sentinel
        assert_eq!(
            pool.index_del(ElemPool::free_sentinel_index()),
            Err(IndexError::ElementIsFreeSentinel)
        );

        // Deleting the same index twice should ideally fail, but our simplified
        // `index_del` doesn't have a robust double-free check. The `PieList` pop
        // logic prevents this from happening in practice. We test the boundary
        // conditions that `index_del` *does* check.
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
    fn test_linking_logic() {
        let (mut pool, indices) = create_pool_with_elems(3, 0);
        let i1 = indices[0];
        let i2 = indices[1];
        let i3 = indices[2];

        // Initially, new elements point to NONE
        assert_eq!(pool.next(i1), Index::NONE);

        // Link i1 -> i2
        pool.index_link_after(i2, i1).unwrap();
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.prev(i2), i1);

        // Link i2 -> i3
        pool.index_link_after(i3, i2).unwrap();
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.prev(i3), i2);

        // Check chain i1 -> i2 -> i3
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.next(i3), Index::NONE);

        // Close circle manually
        pool.get_mut(i1).unwrap().new_prev(i3);
        pool.get_mut(i3).unwrap().new_next(i1);

        // Unlink i2
        pool.index_linkout(i2).unwrap();

        // i1 -> i3
        assert_eq!(pool.next(i1), i3);
        assert_eq!(pool.prev(i3), i1);

        // i2 self-referenced (linkout does this)
        assert_eq!(pool.next(i2), i2);
        assert_eq!(pool.prev(i2), i2);

        // Link i2 before i3
        pool.index_link_before(i2, i3).unwrap();

        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
    }

    #[test]
    fn test_validate_index() {
        // Let's create a known structure with a real list
        let (mut pool, _) = create_pool_with_elems(0, 0);
        let mut list = PieList::new(&mut pool);
        list.push_back(10, &mut pool).unwrap();
        list.push_back(20, &mut pool).unwrap();
        list.push_back(30, &mut pool).unwrap();

        let i1 = pool.next(list.sentinel);
        let i2 = pool.next(i1);
        let i3 = pool.next(i2);

        // All indices in a valid list should validate correctly.
        assert_eq!(pool.validate_index(i1), Ok(()));
        assert_eq!(pool.validate_index(i2), Ok(()));
        assert_eq!(pool.validate_index(i3), Ok(()));

        // Test specific error cases
        assert_eq!(
            pool.validate_index(Index::NONE),
            Err(IndexError::IndexIsNone)
        );
        assert_eq!(
            pool.validate_index(Index::from(99_u32)),
            Err(IndexError::IndexOutOfBounds)
        );

        // Manually free an element to test ElementIsFree error
        pool.data_swap(i2, None);
        assert_eq!(pool.validate_index(i2), Err(IndexError::IndexIsStale));
        {
             // To test ElementIsFree, we need an Index that matches the Zombie/Free version
             // but we want to check that it IS free/zombie?
             // Actually, validate_index checks matching version.
             // If we use the Zombie index, it should pass version check.
             let zombie_ver = pool.elems[i2.slot as usize].vers;
             let _zombie_idx = Index::<i32>::new(i2.slot, zombie_ver);
             // But validate_index returns ElementIsFree if elem.is_free().
             // Zombie is NOT free.
             // So we must fully free it (index_del). But we can't easily call index_del here without proper setup.
             // Let's manually set state to Free.
             pool.elems[i2.slot as usize].vers = (zombie_ver & !crate::elem::STATE_MASK) | crate::elem::STATE_FREE;
             let free_ver = pool.elems[i2.slot as usize].vers;
             let free_idx = Index::<i32>::new(i2.slot, free_ver);
             assert_eq!(pool.validate_index(free_idx), Err(IndexError::ElementIsFree));

             // Restore
             pool.elems[i2.slot as usize].vers = i2.vers; // Start state
             pool.data_swap(i2, Some(20));
        }

        // Manually break a link to test for inconsistency
        // i1's next now points to i3, but i3's prev still points to i2
        pool.get_mut(i1).unwrap().next = i3;

        // i2 thinks its prev is i1, but i1's next is i3. So i2's prev link is broken.
        assert_eq!(pool.validate_index(i2), Err(IndexError::BrokenPrevLink));
        // i1 thinks its next is i3, but i3's prev is i2. So i1's next link is broken.
        assert_eq!(pool.validate_index(i1), Err(IndexError::BrokenNextLink));
        list.clear(&mut pool);
    }


    #[test]
    fn test_shrink_simple() {
        // Scenario: [PoolSen, ListSen, ItemA, (Free), ItemB]
        // Goal: Move ItemB to (Free).
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        list.push_back("A", &mut pool).unwrap();
        let idx_to_remove = list.push_back("RemoveMe", &mut pool).unwrap();
        list.push_back("B", &mut pool).unwrap();

        // Create hole
        // We must unlink FIRST while the element is in USED state.
        // If we swap data first, it becomes ZOMBIE, and linkout will fail (stale index).
        pool.index_linkout(idx_to_remove).unwrap();
        list.len -= 1;
        let _ = pool.data_swap(idx_to_remove, None);
        pool.index_del(idx_to_remove).unwrap();

        assert_eq!(pool.freed, 1);
        let old_cap = pool.capacity();

        let map = pool.shrink_to_fit();
        list.remap(&map);

        assert_eq!(pool.freed, 0);
        assert!(pool.capacity() < old_cap);

        // Verify list integrity
        let vec: Vec<_> = list.iter(&pool).copied().collect();
        assert_eq!(vec, vec!["A", "B"]);

        // Validate all indices
        let mut curr = pool.next(list.sentinel);
        while curr != list.sentinel {
            assert!(pool.validate_index(curr).is_ok());
            curr = pool.next(curr);
        }
        list.clear(&mut pool);
    }

    #[test]
    fn test_shrink_with_sentinel_move() {
        // Scenario: A list whose SENTINEL is at the end of the pool.
        // The sentinel itself must move to fill a hole.
        let mut pool = ElemPool::new();

        // 1. Create some noise to fill low indices
        let mut noise_list = PieList::new(&mut pool);
        noise_list.push_back("Noise", &mut pool).unwrap();

        // 2. Create the target list (High indices)
        let mut list = PieList::new(&mut pool); // Sentinel allocated high
        list.push_back("Data", &mut pool).unwrap();

        // 3. Delete the noise to create holes at the bottom
        noise_list.clear(&mut pool);
        // Now the bottom of the pool is free. `list` sentinel is at the top.

        let map = pool.shrink_to_fit();
        list.remap(&map); // Crucial! Sentinel likely moved.

        assert_eq!(list.len(), 1);
        assert_eq!(*list.front(&pool).unwrap(), "Data");

        // Check graph integrity (self-reference of sentinel)
        let sent_elem = pool.get(list.sentinel).unwrap();
        assert!(sent_elem.next != list.sentinel, "Sentinel should point to Data");
        assert!(sent_elem.prev != list.sentinel, "Sentinel should point to Data");
        list.clear(&mut pool);
    }

    #[test]
    fn test_shrink_randomized_stress() {
        // Stress test with random insertions and deletions
        let mut pool = ElemPool::<usize>::new();
        let mut lists = Vec::new();
        let mut rng = rand::rng(); // Use rng() for Rand 0.9

        // Create 10 lists
        for _ in 0..10 {
            lists.push(PieList::new(&mut pool));
        }

        // 1. Random Populate
        for _ in 0..1000 {
            let list_idx = rng.random_range(0..10);
            let val = rng.random_range(0..10000);
            lists[list_idx].push_back(val, &mut pool).unwrap();
        }

        // 2. Random Delete (Create cheese holes)
        for _ in 0..400 {
            let list_idx = rng.random_range(0..10);
            if !lists[list_idx].is_empty() {
                lists[list_idx].pop_front(&mut pool);
            }
        }

        let total_items_before: usize = lists.iter().map(|l| l.len()).sum();
        assert_eq!(pool.len(), total_items_before);
        assert!(pool.freed > 0);

        // 3. Shrink
        let map = pool.shrink_to_fit();

        // 4. Remap
        for list in lists.iter_mut() {
            list.remap(&map);
        }

        // 5. Verify
        assert_eq!(pool.freed, 0);
        assert_eq!(pool.len(), total_items_before);

        let total_items_after: usize = lists.iter().map(|l| l.len()).sum();
        assert_eq!(total_items_after, total_items_before);

        // Verify structural integrity of every list
        for list in lists.iter_mut() {
            let mut count = 0;
            let mut curr = pool.next(list.sentinel);
            while curr != list.sentinel {
                assert!(pool.validate_index(curr).is_ok());
                count += 1;
                curr = pool.next(curr);
            }
            assert_eq!(count, list.len());
            list.clear(&mut pool);
        }
    }
}
