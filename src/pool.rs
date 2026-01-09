//! A generic pool allocator for multi-headed doubly-linked lists.
//!
//! # Internal Architecture
//!
//! `ElemPool<T>` is a generational arena that provides memory for all data
//! structures in this library. The key design decisions are:
//!
//! ## Memory Layout
//!
//! Each `Elem<T>` contains the element metadata (links + generation/state) and
//! the user data inline, providing optimal cache locality:
//!
//! ```text
//! ElemPool<T>
//! +-------------------------------------------------------------+
//! | elems: Vec<Elem<T>>                                         |
//! | +-----+-----+-----+-----+-----+-----+-----+-----+-----+     |
//! | |  0  |  1  |  2  |  3  |  4  |  5  |  6  |  7  |  8  |     |
//! | |Free | S1  | A   | B   | S2  | X   | Y   |Free |Free |     |
//! | |Sent.|     |     |     |     |     |     |     |     |     |
//! | +-----+-----+-----+-----+-----+-----+-----+-----+-----+     |
//! |                                                             |
//! | freed: 2 (slots 7, 8 are free)                              |
//! | used: 5  (slots 2,3,5,6 have data; S1,S2 are sentinels)     |
//! +-------------------------------------------------------------+
//!
//! Free List:  [0] <-> [7] <-> [8] <-> [0]  (circular, slot 0 is sentinel)
//! List 1:     [1] <-> [2] <-> [3] <-> [1]  (S1 sentinel, A<->B data)
//! List 2:     [4] <-> [5] <-> [6] <-> [4]  (S2 sentinel, X<->Y data)
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
//! Time 0: alloc slot 5 -> handle {slot:5, vers:3}
//! Time 1: free slot 5  -> element 5 generation becomes 4
//! Time 2: alloc slot 5 -> handle {slot:5, vers:5}
//!
//! Old handle {slot:5, vers:3} -> get() returns None (stale)
//! New handle {slot:5, vers:5} -> get() returns Some(&data)
//! ```
//!
//! ## Two-Phase Deletion
//!
//! Deletion is split into data removal and element recycling:
//!
//! 1. `data_swap(handle, None)` — Take data out (element -> Zombie state)
//! 2. `index_del(handle)` — Return element to free list (Zombie -> Free)
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
//! 1. Build remap: old_slot -> new_slot (O(capacity) scan)
//! 2. Swap-remove free elements from the end
//! 3. Update all next/prev links using remap
//! 4. Return remap for external handle updates

use crate::elem::{Elem, STATE_MASK, STATE_USED};
use crate::generation::{Generation, ElemState};
use crate::slot::Slot;
use crate::Index;
use crate::IndexMap;
use alloc::{vec, vec::Vec};
use core::{error, fmt};

#[cfg(feature = "serde")]
use serde::{
    Serialize, Deserialize, Serializer, Deserializer,
    ser::SerializeStruct,
    de::{self, Visitor, MapAccess, SeqAccess},
};

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

/// A pool of elements that provides memory for multiple data structures.
///
/// # Struct-of-Arrays Design
///
/// Each element stores its metadata (links + generation/state) and user data
/// inline, providing optimal cache locality for linked list operations.
///
/// # Rationale
///
/// The `ElemPool` is the cornerstone of this library's design. It acts as a
/// specialized memory allocator. By pre-allocating memory in Vecs and managing
/// its own free list, it avoids the performance cost of frequent calls to the
/// global allocator. This makes creating and destroying elements extremely fast.
///
/// Its public API is minimal, as most interactions are performed through `PieList`,
/// `CursorMut`, or `FibHeap` methods, which take the pool as an argument.
pub struct ElemPool<T> {
    /// Elements containing metadata (links + generation/state) and user data.
    elems: Vec<Elem<T>>,
    /// A count of elements currently in the free list.
    freed: usize,
    /// The number of elements that contain user data.
    /// This count excludes all sentinel nodes and free elements.
    used: usize,
}

impl<T: Clone> Clone for ElemPool<T> {
    fn clone(&self) -> Self {
        Self {
            elems: self.elems.clone(),
            freed: self.freed,
            used: self.used,
        }
    }
}

// =========================================================================
// Custom Serde Implementation
// =========================================================================
//
// We derive Serialize/Deserialize directly since Elem<T> now handles
// its own serialization with inline data.

#[cfg(feature = "serde")]
impl<T: Serialize> Serialize for ElemPool<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as a struct with: elems, freed, used
        let mut state = serializer.serialize_struct("ElemPool", 3)?;
        state.serialize_field("elems", &self.elems)?;
        state.serialize_field("freed", &self.freed)?;
        state.serialize_field("used", &self.used)?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ElemPool<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field { Elems, Freed, Used }

        struct ElemPoolVisitor<T>(core::marker::PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ElemPoolVisitor<T> {
            type Value = ElemPool<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct ElemPool")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<ElemPool<T>, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let elems: Vec<Elem<T>> = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let freed: usize = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let used: usize = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;

                Ok(ElemPool { elems, freed, used })
            }

            fn visit_map<V>(self, mut map: V) -> Result<ElemPool<T>, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut elems: Option<Vec<Elem<T>>> = None;
                let mut freed: Option<usize> = None;
                let mut used: Option<usize> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Elems => {
                            if elems.is_some() {
                                return Err(de::Error::duplicate_field("elems"));
                            }
                            elems = Some(map.next_value()?);
                        }
                        Field::Freed => {
                            if freed.is_some() {
                                return Err(de::Error::duplicate_field("freed"));
                            }
                            freed = Some(map.next_value()?);
                        }
                        Field::Used => {
                            if used.is_some() {
                                return Err(de::Error::duplicate_field("used"));
                            }
                            used = Some(map.next_value()?);
                        }
                    }
                }

                let elems = elems.ok_or_else(|| de::Error::missing_field("elems"))?;
                let freed = freed.ok_or_else(|| de::Error::missing_field("freed"))?;
                let used = used.ok_or_else(|| de::Error::missing_field("used"))?;

                Ok(ElemPool { elems, freed, used })
            }
        }

        const FIELDS: &[&str] = &["elems", "freed", "used"];
        deserializer.deserialize_struct("ElemPool", FIELDS, ElemPoolVisitor(core::marker::PhantomData))
    }
}

impl<T> Default for ElemPool<T> {
    /// Creates a new `ElemPool`, initialized with a single sentinel element
    /// for its internal free list.
    fn default() -> Self {
        // Free list sentinel at slot 0, pointing to itself
        let sentinel_elem = Elem::new_self_ref(Slot::new(0), ElemState::Sentinel);
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
    #[allow(dead_code)]
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
        self.validate_vers(index)
            .map(|slot| self.elems[slot].is_used())
            .unwrap_or(false)
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
        let slot = index.get().ok_or(IndexError::IndexIsNone)?;
        let elem = self.elems.get(slot).ok_or(IndexError::IndexOutOfBounds)?;

        // 1. Generation Check (ABA Protection)
        if elem.vers.as_raw() != index.vers {
            return Err(IndexError::IndexIsStale);
        }
        if elem.is_free() {
            return Err(IndexError::ElementIsFree);
        }

        // 2. Check prev's next points back to us
        let prev_slot = elem.prev.get().ok_or(IndexError::BrokenPrevLink)?;
        let prev_elem = self.elems.get(prev_slot).ok_or(IndexError::BrokenPrevLink)?;
        if prev_elem.next.get() != Some(slot) {
            return Err(IndexError::BrokenPrevLink);
        }

        // 3. Check next's prev points back to us
        let next_slot = elem.next.get().ok_or(IndexError::BrokenNextLink)?;
        let next_elem = self.elems.get(next_slot).ok_or(IndexError::BrokenNextLink)?;
        if next_elem.prev.get() != Some(slot) {
            return Err(IndexError::BrokenNextLink);
        }

        Ok(())
    }

    /// Returns an iterator over the pool's raw element metadata.
    ///
    /// This is primarily used internally (e.g. by `FibHeap`) to perform operations
    /// that require traversing the entire pool structure, such as remapping internal
    /// pointers after a `shrink_to_fit` operation.
    pub fn iter_elems(&self) -> core::slice::Iter<'_, Elem<T>> {
        self.elems.iter()
    }

    /// Returns a mutable iterator over the pool's raw elements.
    ///
    /// This is primarily used internally (e.g. by `FibHeap`) to perform operations
    /// that require traversing the entire pool structure, such as remapping internal
    /// pointers after a `shrink_to_fit` operation.
    pub fn iter_elems_mut(&mut self) -> core::slice::IterMut<'_, Elem<T>> {
        self.elems.iter_mut()
    }

    // --- Internal Helper: Version Validation ---

    /// Validates an index and returns the slot if valid.
    #[inline]
    fn validate_vers(&self, index: Index<T>) -> Result<usize, IndexError> {
        let slot = index.get().ok_or(IndexError::IndexIsNone)?;
        let elem = self.elems.get(slot).ok_or(IndexError::IndexOutOfBounds)?;
        if elem.vers.as_raw() != index.vers {
            return Err(IndexError::IndexIsStale);
        }
        Ok(slot)
    }

    /// Allocates a new index, reusing a free element if available or creating a new one.
    ///
    /// This is the primary method for acquiring a new node from the pool. It
    /// first checks the free list. If the free list is not empty, it unlinks
    /// and returns the first available node. If the free list is empty, it
    /// pushes a new element to the end of the internal Vec.
    pub(crate) fn index_new(&mut self) -> Result<Index<T>, IndexError> {
        // Check free list (slot 0 is free sentinel)
        let next_free_slot = self.elems[0].next;
        // Free list is empty when sentinel points to itself (slot 0)
        if let Some(slot_idx) = next_free_slot.get()
            && slot_idx != 0 {
                // Free list is not empty, reuse an element.
                self.slot_linkout(next_free_slot);
                let elem = &mut self.elems[slot_idx];
                // Bump generation and mark as ZOMBIE (allocated but no data yet).
                let new_vers = elem.bump_gen(ElemState::Zombie);
                self.freed -= 1;
                return Ok(Index::new(next_free_slot.as_raw(), new_vers));
        }
        // Free list is empty, allocate a new element.
        let slot = self.elems.len() as u32;
        let slot_ref = Slot::new(slot);
        let mut new_elem = Elem::default();
        // It starts as Free (default). Transition to ZOMBIE.
        let new_vers = new_elem.bump_gen(ElemState::Zombie);
        // Self-reference initially
        new_elem.set_links(slot_ref, slot_ref);
        self.elems.push(new_elem);
        Ok(Index::new(slot, new_vers))
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
        let slot = self.validate_vers(index)?;
        let elem = &mut self.elems[slot];

        // Element should be ZOMBIE (just allocated via index_new)
        if !elem.is_zombie() {
            return Err(IndexError::ElementIsFree);
        }

        // Transition state from ZOMBIE to SENTINEL
        let new_vers = elem.make_sentinel();

        // Sentinel points to itself (empty list)
        let slot_ref = Slot::new(slot as u32);
        elem.set_links(slot_ref, slot_ref);

        Ok(Index::new(index.slot, new_vers))
    }

    /// Allocates a new index and initializes it with data, returning the correct Index
    /// with the USED state version.
    ///
    /// This combines `index_new()` and data initialization to ensure the returned Index
    /// has the correct version for a USED element.
    pub(crate) fn index_new_with_data(&mut self, data: T) -> Result<Index<T>, IndexError> {
        let new_idx = self.index_new()?;
        let slot = new_idx.slot as usize;

        let elem = &mut self.elems[slot];
        if !elem.is_zombie() {
            return Err(IndexError::ElementIsFree);
        }

        // Store data and transition to USED
        elem.write_data(data);
        let new_vers = elem.make_used();
        self.used += 1;

        Ok(Index::new(new_idx.slot, new_vers))
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

        let slot = index.slot as usize;
        let elem = self.elems.get(slot).ok_or(IndexError::IndexOutOfBounds)?;

        // Strict check: version must match, OR element became zombie (data was taken)
        let is_zombie_match = elem.is_zombie() &&
             elem.vers.same_counter(Generation::from_raw(index.vers)) &&
             ((index.vers & STATE_MASK) == STATE_USED);

        if elem.vers.as_raw() != index.vers && !is_zombie_match {
            return Err(IndexError::IndexIsStale);
        }

        // Transition State: Zombie/Used -> Free (Increments Generation)
        let elem = &mut self.elems[slot];
        let new_vers = elem.make_free();

        // Link into Free List (after the sentinel at slot 0)
        self.slot_link_after(Slot::new(slot as u32), Slot::new(0));
        self.freed += 1;
        // Suppress unused warning - new_vers is for consistency
        let _ = new_vers;
        Ok(())
    }

    /// Gets an immutable reference to the element metadata at the given index.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn get_elem(&self, index: Index<T>) -> Result<&Elem<T>, IndexError> {
        let slot = self.validate_vers(index)?;
        Ok(&self.elems[slot])
    }

    /// Gets a mutable reference to the element metadata at the given index.
    #[inline]
    pub(crate) fn get_elem_mut(&mut self, index: Index<T>) -> Result<&mut Elem<T>, IndexError> {
        let slot = self.validate_vers(index)?;
        Ok(&mut self.elems[slot])
    }

    /// Gets the `next` index for the element at the given index.
    /// Returns Index with the version of the next element.
    #[inline]
    pub(crate) fn next(&self, index: Index<T>) -> Index<T> {
        if let Ok(slot) = self.validate_vers(index) {
            if let Some(next_slot) = self.elems[slot].next.get() {
                let next_vers = self.elems[next_slot].vers.as_raw();
                Index::new(self.elems[slot].next.as_raw(), next_vers)
            } else {
                Index::NONE
            }
        } else {
            Index::NONE
        }
    }

    /// Gets the `prev` index for the element at the given index.
    /// Returns Index with the version of the prev element.
    #[inline]
    pub(crate) fn prev(&self, index: Index<T>) -> Index<T> {
        if let Ok(slot) = self.validate_vers(index) {
            if let Some(prev_slot) = self.elems[slot].prev.get() {
                let prev_vers = self.elems[prev_slot].vers.as_raw();
                Index::new(self.elems[slot].prev.as_raw(), prev_vers)
            } else {
                Index::NONE
            }
        } else {
            Index::NONE
        }
    }

    /// Gets an immutable reference to the data inside the element at the given index.
    #[inline]
    pub(crate) fn data(&self, index: Index<T>) -> Option<&T> {
        let slot = self.validate_vers(index).ok()?;
        let elem = &self.elems[slot];
        if elem.is_used() {
            #[allow(unsafe_code)]
            Some(unsafe { elem.data_ref_unchecked() })
        } else {
            None
        }
    }

    /// Gets a mutable reference to the data inside the element at the given index.
    #[inline]
    pub(crate) fn data_mut(&mut self, index: Index<T>) -> Option<&mut T> {
        let slot = self.validate_vers(index).ok()?;
        let elem = &mut self.elems[slot];
        if elem.is_used() {
            #[allow(unsafe_code)]
            Some(unsafe { elem.data_mut_unchecked() })
        } else {
            None
        }
    }

    /// Swaps the data in an element and updates the pool's `used` count accordingly.
    ///
    /// This is the sole method responsible for modifying an element's data, as it
    /// correctly maintains the pool's `used` counter.
    #[inline]
    pub(crate) fn data_swap(&mut self, index: Index<T>, new_data: Option<T>) -> Option<T> {
        let slot = self.validate_vers(index).ok()?;
        let elem = &mut self.elems[slot];

        if let Some(data) = new_data {
            // Storing new data
            let old_data = if elem.is_used() {
                #[allow(unsafe_code)]
                Some(unsafe { elem.take_data_unchecked() })
            } else {
                None
            };

            elem.write_data(data);

            // If transitioning from zombie to used, update state
            if elem.is_zombie() {
                elem.vers = elem.vers.with_state(ElemState::Used);
            }

            if old_data.is_none() && elem.is_used() {
                self.used += 1;
            }
            old_data
        } else {
            // Taking data out (Zombie transition)
            if elem.is_used() {
                #[allow(unsafe_code)]
                let old_data = unsafe { elem.take_data_unchecked() };
                elem.make_zombie();
                self.used -= 1;
                Some(old_data)
            } else {
                None
            }
        }
    }

    // =========================================================================
    // Internal Slot-Based Link Operations (no version checks)
    // =========================================================================

    /// Unlinks a slot from its neighbors (internal, no version check).
    #[inline]
    fn slot_linkout(&mut self, slot: Slot) {
        let slot_idx = slot.unwrap();
        let elem = &self.elems[slot_idx];
        let (prev_slot, next_slot) = (elem.prev, elem.next);

        // Update prev's next
        self.elems[prev_slot.unwrap()].next = next_slot;
        // Update next's prev
        self.elems[next_slot.unwrap()].prev = prev_slot;
        // Self-reference
        let elem = &mut self.elems[slot_idx];
        elem.set_links(slot, slot);
    }

    /// Links slot immediately after `after_slot` (internal, no version check).
    #[inline]
    fn slot_link_after(&mut self, slot: Slot, after_slot: Slot) {
        let next_slot = self.elems[after_slot.unwrap()].next;

        // Update after's next
        self.elems[after_slot.unwrap()].next = slot;
        // Update next's prev
        self.elems[next_slot.unwrap()].prev = slot;
        // Set this element's links
        self.elems[slot.unwrap()].set_links(after_slot, next_slot);
    }

    /// Links slot immediately before `before_slot` (internal, no version check).
    #[inline]
    fn slot_link_before(&mut self, slot: Slot, before_slot: Slot) {
        let prev_slot = self.elems[before_slot.unwrap()].prev;

        // Update before's prev
        self.elems[before_slot.unwrap()].prev = slot;
        // Update prev's next
        self.elems[prev_slot.unwrap()].next = slot;
        // Set this element's links
        self.elems[slot.unwrap()].set_links(prev_slot, before_slot);
    }

    // =========================================================================
    // Fast Slot-Based Accessors (no generation lookup - internal use only)
    // =========================================================================
    //
    // These methods provide O(1) access to element data using raw slot indices.
    // They skip generation validation and the expensive lookup of the next
    // element's version. Use these for internal traversals where the slot
    // is known to be valid.

    /// Gets the next slot for an element (no generation lookup).
    ///
    /// This is much faster than `next()` which must look up the next element's
    /// generation to construct an `Index<T>`.
    #[inline]
    pub(crate) fn next_slot(&self, slot: usize) -> Slot {
        self.elems[slot].next
    }

    /// Gets the prev slot for an element (no generation lookup).
    #[inline]
    pub(crate) fn prev_slot(&self, slot: usize) -> Slot {
        self.elems[slot].prev
    }

    #[allow(dead_code)]
    /// Gets an immutable reference to element by slot (unchecked).
    #[inline]
    pub(crate) fn elem(&self, slot: usize) -> &Elem<T> {
        &self.elems[slot]
    }

    /// Gets a mutable reference to element by slot (unchecked).
    #[inline]
    pub(crate) fn elem_mut(&mut self, slot: usize) -> &mut Elem<T> {
        &mut self.elems[slot]
    }

    /// Gets an immutable reference to user data by slot (unchecked).
    ///
    /// # Safety
    /// Caller must ensure the slot is valid and contains USED data.
    #[inline]
    pub(crate) fn data_at(&self, slot: usize) -> Option<&T> {
        let elem = &self.elems[slot];
        if elem.is_used() {
            #[allow(unsafe_code)]
            Some(unsafe { elem.data_ref_unchecked() })
        } else {
            None
        }
    }

    /// Gets a mutable reference to user data by slot (unchecked).
    ///
    /// # Safety
    /// Caller must ensure the slot is valid and contains USED data.
    #[inline]
    pub(crate) fn data_at_mut(&mut self, slot: usize) -> Option<&mut T> {
        let elem = &mut self.elems[slot];
        if elem.is_used() {
            #[allow(unsafe_code)]
            Some(unsafe { elem.data_mut_unchecked() })
        } else {
            None
        }
    }

    /// Constructs an Index<T> from a slot by reading its current generation.
    ///
    /// Use this when you need to return an Index<T> to external code after
    /// doing internal slot-based operations.
    #[inline]
    pub(crate) fn index_from_slot(&self, slot: usize) -> Index<T> {
        Index::new(slot as u32, self.elems[slot].vers.as_raw())
    }

    // =========================================================================
    // Public Index-Based Link Operations (with version checks)
    // =========================================================================

    /// Unlinks an element from its current position in a list.
    /// After this operation, the element points to itself.
    #[inline]
    pub(crate) fn index_linkout(&mut self, index: Index<T>) -> Result<(), IndexError> {
        let slot = self.validate_vers(index)?;
        self.slot_linkout(Slot::new(slot as u32));
        Ok(())
    }

    /// Links `this` element immediately after the `after` element.
    #[inline]
    pub(crate) fn index_link_after(
        &mut self,
        this: Index<T>,
        after: Index<T>,
    ) -> Result<(), IndexError> {
        let this_slot = self.validate_vers(this)?;
        let after_slot = self.validate_vers(after)?;
        self.slot_link_after(Slot::new(this_slot as u32), Slot::new(after_slot as u32));
        Ok(())
    }

    /// Links `this` element immediately before the `before` element.
    #[inline]
    pub(crate) fn index_link_before(
        &mut self,
        this: Index<T>,
        before: Index<T>,
    ) -> Result<(), IndexError> {
        let this_slot = self.validate_vers(this)?;
        let before_slot = self.validate_vers(before)?;
        self.slot_link_before(Slot::new(this_slot as u32), Slot::new(before_slot as u32));
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

        // Start from the first free element directly
        let mut current_free_slot = self.elems[0].next;

        // Loop terminates when we reach slot 0 (sentinel pointing to itself)
        // or Slot::NONE (which shouldn't happen in a well-formed free list)
        while let Some(idx) = current_free_slot.get() {
            if idx == 0 {
                // Reached the sentinel, free list traversal complete
                break;
            }
            if idx < target_len {
                // This is a hole in the preserved region. We must fill it.
                vacancies.push(idx);
            } else {
                // This is a hole in the region we are cutting off.
                // We mark it so the tail-scanner knows to ignore it.
                is_free_tail[idx - target_len] = true;
            }
            current_free_slot = self.elems[idx].next;
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
            let vers = self.elems[source].vers.as_raw();

            // Swap elems (which now includes data)
            self.elems.swap(dest, source);

            let old_idx = Index::new(source as u32, vers);
            let new_idx = Index::new(dest as u32, vers);
            remapping.insert(old_idx, new_idx);

            // 4. Fix Neighbors (The Graph Patching)
            // The node at `dest` currently thinks its neighbors are pointing to `source`.
            // We must update those neighbors to point to `dest`.
            let (prev_slot, next_slot) = self.elems[dest].links();

            // Helper: resolve a slot to its effective slot after remapping.
            // Uses O(1) array lookup for tail items instead of hash map.
            let resolve = |slot: Slot, remap: &[u32], tgt_len: usize, old_len: usize| -> usize {
                if let Some(slot_usize) = slot.get() {
                    if slot_usize >= tgt_len && slot_usize < old_len {
                        let new_slot = remap[slot_usize - tgt_len];
                        if new_slot != u32::MAX {
                            return new_slot as usize;
                        }
                    }
                    slot_usize
                } else {
                    0 // NONE slot, should not happen for valid links
                }
            };

            // Fix prev's next pointer
            let effective_prev = resolve(prev_slot, &slot_remap, target_len, old_len);
            self.elems[effective_prev].next = Slot::new(dest as u32);

            // Fix next's prev pointer
            let effective_next = resolve(next_slot, &slot_remap, target_len, old_len);
            self.elems[effective_next].prev = Slot::new(dest as u32);
        }

        // 5. Final Cleanup
        // Truncate the vector
        self.elems.truncate(target_len);
        // Reset pool state
        self.freed = 0;
        // Reset the free list sentinel to point to itself (empty list)
        self.elems[0].set_links(Slot::new(0), Slot::new(0));

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
            if elem.is_used() {
                #[allow(unsafe_code)]
                let data = unsafe { elem.data_ref_unchecked() };
                writeln!(f, "  [{}]: {} = {}", i, elem, data)?;
            } else {
                writeln!(f, "  [{}]: {}", i, elem)?;
            }
        }
        Ok(())
    }
}

impl<T: fmt::Debug> fmt::Debug for ElemPool<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElemPool")
            .field("elems", &self.elems)
            .field("freed", &self.freed)
            .field("used", &self.used)
            .finish()
    }
}

impl<T> Drop for ElemPool<T> {
    fn drop(&mut self) {
        // Drop data for all USED elements
        for elem in self.elems.iter_mut() {
            if elem.is_used() {
                // SAFETY: We are in the Drop impl, so no one else can access this data.
                // Take the data out to trigger its Drop impl.
                #[allow(unsafe_code)]
                unsafe {
                    let _ = elem.take_data_unchecked();
                }
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
        assert_eq!(first_free, Slot::new(deleted_index.slot));

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

        // Initially, new elements are self-referential (point to themselves)
        assert_eq!(pool.next(i1), i1);
        assert_eq!(pool.prev(i1), i1);

        // Link i2 after i1: Creates circular i1 <-> i2
        pool.index_link_after(i2, i1).unwrap();
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.prev(i2), i1);
        assert_eq!(pool.next(i2), i1); // circular back to i1
        assert_eq!(pool.prev(i1), i2); // circular back from i2

        // Link i3 after i2: Creates circular i1 <-> i2 <-> i3 <-> i1
        pool.index_link_after(i3, i2).unwrap();
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.prev(i3), i2);
        assert_eq!(pool.next(i3), i1); // circular back to i1
        assert_eq!(pool.prev(i1), i3); // circular back from i3

        // Check full circular chain: i1 -> i2 -> i3 -> i1
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.next(i3), i1);

        // Unlink i2
        pool.index_linkout(i2).unwrap();

        // i1 <-> i3 (circular)
        assert_eq!(pool.next(i1), i3);
        assert_eq!(pool.prev(i3), i1);
        assert_eq!(pool.next(i3), i1);
        assert_eq!(pool.prev(i1), i3);

        // i2 self-referenced (linkout does this)
        assert_eq!(pool.next(i2), i2);
        assert_eq!(pool.prev(i2), i2);

        // Link i2 before i3 (i3's prev is i1, so inserts between i1 and i3)
        pool.index_link_before(i2, i3).unwrap();

        // Now: i1 <-> i2 <-> i3 <-> i1
        assert_eq!(pool.next(i1), i2);
        assert_eq!(pool.next(i2), i3);
        assert_eq!(pool.next(i3), i1);
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
             let _zombie_idx = Index::<i32>::new(i2.slot, zombie_ver.as_raw());
             // But validate_index returns ElementIsFree if elem.is_free().
             // Zombie is NOT free.
             // So we must fully free it (index_del). But we can't easily call index_del here without proper setup.
             // Let's manually set state to Free.
             pool.elems[i2.slot as usize].vers = zombie_ver.with_state(crate::generation::ElemState::Free);
             let free_ver = pool.elems[i2.slot as usize].vers;
             let free_idx = Index::<i32>::new(i2.slot, free_ver.as_raw());
             assert_eq!(pool.validate_index(free_idx), Err(IndexError::ElementIsFree));

             // Restore - need to reconstruct the original version with Used state
             pool.elems[i2.slot as usize].vers = crate::generation::Generation::from_raw(i2.vers);
             pool.data_swap(i2, Some(20));
        }

        // Manually break a link to test for inconsistency
        // i1's next now points to i3, but i3's prev still points to i2
        pool.elems[i1.slot as usize].next = Slot::new(i3.slot);

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
        let sent_elem = &pool.elems[list.sentinel.slot as usize];
        assert!(sent_elem.next != Slot::new(list.sentinel.slot), "Sentinel should point to Data");
        assert!(sent_elem.prev != Slot::new(list.sentinel.slot), "Sentinel should point to Data");
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
