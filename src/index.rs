//! Definition of the `Index<T>` type — type-safe generational handles.
//!
//! # Internal Architecture
//!
//! ## Handle Design
//!
//! ```text
//! Index<T> (16 bytes on 64-bit, 12 bytes on 32-bit)
//! ┌─────────────────┬─────────────────┬───────────────┐
//! │   slot: u32     │   vers: u32     │ PhantomData<T>│
//! │   (position)    │   (generation)  │ (type safety) │
//! └─────────────────┴─────────────────┴───────────────┘
//! ```
//!
//! ## Sentinel Value Pattern
//!
//! Instead of `Option<Index<T>>`, we use `slot = u32::MAX` as the "none"
//! sentinel. This gives us:
//! - Same size as the non-optional type
//! - No double-wrapping overhead
//! - Simple `is_some()`/`is_none()` checks
//! - Clean API without Option ergonomic issues
//!
//! ## Why Not NonMaxU32?
//!
//! We considered using `NonMaxU32` for niche optimization so that
//! `Option<Index<T>>` would be the same size. However:
//! - Analysis shows `Option<Index<T>>` is never used in the codebase
//! - All internal APIs use the `Index::NONE` sentinel pattern
//! - No size benefit from the optimization
//!
//! ## Type Safety via PhantomData
//!
//! The `PhantomData<T>` marker ensures `Index<Foo>` and `Index<Bar>` are
//! incompatible types at compile time. This prevents bugs where handles
//! from one pool are accidentally used with another.
//!
//! ## Generation Matching
//!
//! The `vers` field must match the element's current generation for a
//! handle to be valid. When an element is freed and reused, its generation
//! increments, invalidating old handles. This provides ABA protection
//! without runtime overhead beyond a simple u32 comparison.

use core::{cmp::Ordering, convert::TryFrom, fmt, hash::{Hash, Hasher}, marker::PhantomData};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// A type-safe, compact handle to an element in an `ElemPool`.
///
/// An `Index<T>` is essentially a wrapper around a `u32`, providing a cheap,
/// `Copy`-able way to reference list elements without relying on raw pointers
/// or garbage collection.
///
/// # Rationale
///
/// Using a custom `Index` type instead of a raw `usize` or `u32` provides
/// several benefits:
/// - **Type Safety:** `Index<Foo>` is a different type from `Index<Bar>`. This
///   prevents accidentally using an index from a pool of `Foo`s to access a
///   pool of `Bar`s. The `PhantomData<T>` marker enforces this at compile time
///   with zero runtime cost.
/// - **"None" State:** The maximum value of `u32` is reserved for `Index::NONE`,
///   creating a clear, efficient "null" or "invalid" state, similar to
///   `Option<T>` but without the added size overhead.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(bound = ""))] // T does not need to be Serializable
pub struct Index<T> {
    // The slot number within the ElemPool's internal storage.
    pub(crate) slot: u32,
    // The generation/version number for this index.
    pub(crate) vers: u32,
    #[cfg_attr(feature = "serde", serde(skip))] // Don't serialize PhantomData
    _marker: PhantomData<T>,
}

// Manually implement Clone and Copy to avoid trait bounds on `T`.
impl<T> Clone for Index<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Index<T> {}

impl<T> fmt::Debug for Index<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.slot {
            u32::MAX => write!(f, "Index(-)"),
            _ => write!(f, "{}@{}", self.slot, self.vers),
        }
    }
}

impl<T> PartialOrd for Index<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Index<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.slot.cmp(&other.slot)
            .then(self.vers.cmp(&other.vers))
    }
}

impl<T> Default for Index<T> {
    fn default() -> Self {
        Self::NONE
    }
}

impl<T> PartialEq for Index<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot && self.vers == other.vers
    }
}

impl<T> Eq for Index<T> {}

impl<T> Hash for Index<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.slot.hash(state);
        self.vers.hash(state);
    }
}

impl<T> Index<T> {
    #[inline(always)]
    pub(crate) fn new(slot: u32, vers: u32) -> Self {
        Self { slot, vers, _marker: PhantomData }
    }

    pub const NONE: Self = Index {
        slot: u32::MAX,
        vers: 0,
        _marker: PhantomData,
    };

    #[inline]
    pub fn is_some(&self) -> bool {
        self.slot != u32::MAX
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        self.slot == u32::MAX
    }

    #[inline]
    pub(crate) fn get(&self) -> Option<usize> {
        if self.is_some() {
            Some(self.slot as usize)
        } else {
            None
        }
    }
}

impl<T> From<u32> for Index<T> {
    #[inline]
    fn from(ndx: u32) -> Index<T> {
        Self {
            slot: ndx,
            vers: 0,
            _marker: PhantomData,
        }
    }
}

impl<T> From<usize> for Index<T> {
    #[inline]
    fn from(index: usize) -> Index<T> {
        Index::from(u32::try_from(index).unwrap_or(u32::MAX))
    }
}

impl<T> fmt::Display for Index<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            Some(n) => write!(f, "{n}"),
            None => write!(f, "-"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(PartialEq, Eq)]
    struct MyData;

    #[test]
    fn test_creation_and_constants() {
        let index_from_u32 = Index::<MyData>::from(42_u32);
        assert_eq!(index_from_u32.slot, 42);
        assert_eq!(Index::<MyData>::NONE.slot, u32::MAX);
    }

    #[test]
    fn test_default() {
        let default_index = Index::<MyData>::default();
        assert_eq!(default_index, Index::NONE);
    }

    #[test]
    fn test_state_checks() {
        let valid_index = Index::<MyData>::from(10_u32);
        let none_index = Index::<MyData>::NONE;
        assert!(valid_index.is_some());
        assert!(!none_index.is_some());
    }

    #[test]
    fn test_get() {
        let valid_index = Index::<MyData>::from(99_u32);
        let none_index = Index::<MyData>::NONE;
        assert_eq!(valid_index.get(), Some(99_usize));
        assert_eq!(none_index.get(), None);
    }

    #[test]
    fn test_equality() {
        let index1a = Index::<MyData>::from(1_u32);
        let index1b = Index::<MyData>::from(1_u32);
        let index2 = Index::<MyData>::from(2_u32);
        assert_eq!(index1a, index1b);
        assert_ne!(index1a, index2);
    }

    #[test]
    fn test_hash() {
        let mut set = HashSet::new();
        let index1 = Index::<MyData>::from(1_u32);
        let index1_dup = Index::<MyData>::from(1_u32);
        set.insert(index1);
        assert!(!set.insert(index1_dup));
        assert!(set.contains(&index1));
    }

    #[test]
    fn test_from_usize_truncation() {
        // Values that fit in u32 should work normally
        let index = Index::<MyData>::from(42_usize);
        assert_eq!(index.get(), Some(42));

        // u32::MAX - 1 is the largest valid slot
        let index = Index::<MyData>::from((u32::MAX - 1) as usize);
        assert_eq!(index.get(), Some((u32::MAX - 1) as usize));

        // Values > u32::MAX should map to NONE (u32::MAX sentinel)
        #[cfg(target_pointer_width = "64")]
        {
            let index = Index::<MyData>::from(u32::MAX as usize + 1);
            assert_eq!(index, Index::NONE);
            assert_eq!(index.get(), None);
        }
    }

    #[test]
    fn test_ord_includes_vers() {
        // Same slot, different vers: vers should be the tiebreaker
        let a = Index::<MyData>::new(5, 1);
        let b = Index::<MyData>::new(5, 2);
        assert!(a < b, "same slot, lower vers should sort first");

        // Different slot: slot is primary key
        let c = Index::<MyData>::new(3, 100);
        let d = Index::<MyData>::new(7, 0);
        assert!(c < d, "lower slot should sort first regardless of vers");

        // Equal slot and vers
        let e = Index::<MyData>::new(5, 5);
        let f = Index::<MyData>::new(5, 5);
        assert_eq!(e.cmp(&f), core::cmp::Ordering::Equal);
    }
}
