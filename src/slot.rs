//! Definition of the `Slot` type — a compact optional slot index.
//!
//! # Overview
//!
//! `Slot` is a newtype wrapper around `u32` that represents an optional
//! slot index in the element pool. It uses `u32::MAX` as a sentinel value
//! to represent "no slot" (similar to a null pointer).
//!
//! # Design Rationale
//!
//! Instead of using `Option<u32>` (which would be 8 bytes due to alignment),
//! `Slot` maintains a 4-byte representation while providing an ergonomic
//! Option-like API through its `get()` method.
//!
//! ## Key Benefits
//!
//! - **Compact**: Only 4 bytes, same as raw `u32`
//! - **Ergonomic**: `get()` returns `Option<usize>` for direct array indexing
//! - **Safe**: Cannot forget to check for the "none" case
//! - **Zero-cost**: All methods are `#[inline]` and optimize away
//!
//! # Example
//!
//! ```
//! use pie_core::slot::Slot;
//!
//! let slot = Slot::new(42);
//! assert!(slot.is_some());
//! assert_eq!(slot.get(), Some(42));
//!
//! let none = Slot::NONE;
//! assert!(none.is_none());
//! assert_eq!(none.get(), None);
//!
//! // Ready for array indexing:
//! // if let Some(idx) = slot.get() {
//! //     array[idx].do_something();
//! // }
//! ```

use core::fmt;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize, Serializer, Deserializer};

/// A compact optional slot index.
///
/// Represents either a valid slot index (0 to `u32::MAX - 1`) or "no slot"
/// (`Slot::NONE`). The `get()` method returns `Option<usize>` for ergonomic
/// use with the `?` operator and direct array indexing.
///
/// # Size
///
/// Exactly 4 bytes, same as `u32`.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot(u32);

impl Slot {
    /// The sentinel value representing "no slot".
    ///
    /// This is analogous to `None` for `Option<usize>`.
    pub const NONE: Slot = Slot(u32::MAX);

    /// Creates a new `Slot` from a raw slot index.
    ///
    /// # Panics
    ///
    /// Debug-asserts that `slot != u32::MAX` (reserved for `NONE`).
    /// In release builds, passing `u32::MAX` will create a `NONE` slot.
    #[inline]
    pub fn new(slot: u32) -> Self {
        debug_assert!(slot != u32::MAX, "slot value u32::MAX is reserved for NONE");
        Slot(slot)
    }

    /// Creates a `Slot` from a `usize`, returning `NONE` if out of range.
    ///
    /// Valid range is `0..u32::MAX` (i.e., 0 to 4,294,967,294).
    #[inline]
    pub fn from_usize(slot: usize) -> Self {
        if slot < u32::MAX as usize {
            Slot(slot as u32)
        } else {
            Slot::NONE
        }
    }

    /// Returns the slot index as `Option<usize>`, ready for array indexing.
    ///
    /// Returns `None` if this is `Slot::NONE`.
    ///
    /// # Example
    ///
    /// ```
    /// use pie_core::slot::Slot;
    ///
    /// let slot = Slot::new(5);
    /// if let Some(idx) = slot.get() {
    ///     // idx is usize, ready for array indexing
    ///     assert_eq!(idx, 5);
    /// }
    ///
    /// // Works with the ? operator
    /// fn example(slot: Slot) -> Option<usize> {
    ///     let idx = slot.get()?;
    ///     Some(idx * 2)
    /// }
    /// ```
    #[inline]
    pub fn get(self) -> Option<usize> {
        if self.0 != u32::MAX {
            Some(self.0 as usize)
        } else {
            None
        }
    }

    /// Returns the raw `u32` value.
    ///
    /// Returns `u32::MAX` for `Slot::NONE`.
    #[inline]
    pub fn as_raw(self) -> u32 {
        self.0
    }

    /// Creates a `Slot` from a raw `u32` value.
    ///
    /// Unlike `new()`, this does not assert that the value is not `u32::MAX`.
    /// Use this for deserialization or when the value may be `NONE`.
    #[inline]
    pub fn from_raw(raw: u32) -> Self {
        Slot(raw)
    }

    /// Returns `true` if this slot contains a valid index.
    #[inline]
    pub fn is_some(self) -> bool {
        self.0 != u32::MAX
    }

    /// Returns `true` if this is `Slot::NONE`.
    #[inline]
    pub fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    /// Maps the slot value using the given function.
    ///
    /// If this is `NONE`, returns `None`. Otherwise, applies `f` to the
    /// slot index and returns `Some(result)`.
    #[inline]
    pub fn map<U, F: FnOnce(usize) -> U>(self, f: F) -> Option<U> {
        self.get().map(f)
    }

    /// Returns the slot index or a default value.
    #[inline]
    pub fn unwrap_or(self, default: usize) -> usize {
        self.get().unwrap_or(default)
    }

    /// Returns the slot index, panicking if `NONE`.
    ///
    /// # Panics
    ///
    /// Panics if this is `Slot::NONE`.
    #[inline]
    pub fn unwrap(self) -> usize {
        self.get().expect("called unwrap() on Slot::NONE")
    }
}

impl Default for Slot {
    #[inline]
    fn default() -> Self {
        Slot::NONE
    }
}

impl fmt::Debug for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.get() {
            Some(idx) => write!(f, "Slot({})", idx),
            None => write!(f, "Slot::NONE"),
        }
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.get() {
            Some(idx) => write!(f, "{}", idx),
            None => write!(f, "-"),
        }
    }
}

impl From<u32> for Slot {
    #[inline]
    fn from(slot: u32) -> Self {
        Slot::new(slot)
    }
}

impl From<usize> for Slot {
    #[inline]
    fn from(slot: usize) -> Self {
        Slot::from_usize(slot)
    }
}

// Serde support: serialize as raw u32
#[cfg(feature = "serde")]
impl Serialize for Slot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Slot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u32::deserialize(deserializer).map(Slot::from_raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_size() {
        assert_eq!(std::mem::size_of::<Slot>(), 4);
    }

    #[test]
    fn test_new_and_get() {
        let slot = Slot::new(42);
        assert_eq!(slot.get(), Some(42));
        assert!(slot.is_some());
        assert!(!slot.is_none());
    }

    #[test]
    fn test_none() {
        let slot = Slot::NONE;
        assert_eq!(slot.get(), None);
        assert!(!slot.is_some());
        assert!(slot.is_none());
    }

    #[test]
    fn test_default_is_none() {
        let slot: Slot = Default::default();
        assert!(slot.is_none());
    }

    #[test]
    fn test_from_usize() {
        assert_eq!(Slot::from_usize(0).get(), Some(0));
        assert_eq!(Slot::from_usize(100).get(), Some(100));
        assert_eq!(Slot::from_usize(u32::MAX as usize - 1).get(), Some(u32::MAX as usize - 1));
        assert!(Slot::from_usize(u32::MAX as usize).is_none());
        assert!(Slot::from_usize(usize::MAX).is_none());
    }

    #[test]
    fn test_as_raw_and_from_raw() {
        let slot = Slot::new(42);
        assert_eq!(slot.as_raw(), 42);
        assert_eq!(Slot::from_raw(42).get(), Some(42));

        let none = Slot::NONE;
        assert_eq!(none.as_raw(), u32::MAX);
        assert!(Slot::from_raw(u32::MAX).is_none());
    }

    #[test]
    fn test_map() {
        let slot = Slot::new(10);
        assert_eq!(slot.map(|x| x * 2), Some(20));

        let none = Slot::NONE;
        assert_eq!(none.map(|x| x * 2), None);
    }

    #[test]
    fn test_unwrap_or() {
        assert_eq!(Slot::new(42).unwrap_or(0), 42);
        assert_eq!(Slot::NONE.unwrap_or(99), 99);
    }

    #[test]
    fn test_unwrap() {
        assert_eq!(Slot::new(42).unwrap(), 42);
    }

    #[test]
    #[should_panic(expected = "called unwrap() on Slot::NONE")]
    fn test_unwrap_none_panics() {
        Slot::NONE.unwrap();
    }

    #[test]
    fn test_debug_format() {
        assert_eq!(format!("{:?}", Slot::new(42)), "Slot(42)");
        assert_eq!(format!("{:?}", Slot::NONE), "Slot::NONE");
    }

    #[test]
    fn test_display_format() {
        assert_eq!(format!("{}", Slot::new(42)), "42");
        assert_eq!(format!("{}", Slot::NONE), "-");
    }

    #[test]
    fn test_equality() {
        assert_eq!(Slot::new(1), Slot::new(1));
        assert_ne!(Slot::new(1), Slot::new(2));
        assert_eq!(Slot::NONE, Slot::NONE);
        assert_ne!(Slot::new(0), Slot::NONE);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Slot::new(1));
        set.insert(Slot::new(2));
        set.insert(Slot::NONE);

        assert!(set.contains(&Slot::new(1)));
        assert!(set.contains(&Slot::new(2)));
        assert!(set.contains(&Slot::NONE));
        assert!(!set.contains(&Slot::new(3)));
    }

    #[test]
    fn test_from_u32() {
        let slot: Slot = 42u32.into();
        assert_eq!(slot.get(), Some(42));
    }

    #[test]
    fn test_from_usize_trait() {
        let slot: Slot = 42usize.into();
        assert_eq!(slot.get(), Some(42));
    }

    #[test]
    fn test_zero_is_valid() {
        // Slot 0 is valid (unlike NonZeroU32)
        let slot = Slot::new(0);
        assert_eq!(slot.get(), Some(0));
        assert!(slot.is_some());
    }

    #[test]
    fn test_max_valid_slot() {
        // u32::MAX - 1 is the maximum valid slot
        let slot = Slot::new(u32::MAX - 1);
        assert_eq!(slot.get(), Some((u32::MAX - 1) as usize));
        assert!(slot.is_some());
    }

    #[test]
    fn test_question_mark_operator() {
        fn use_slot(slot: Slot) -> Option<usize> {
            let idx = slot.get()?;
            Some(idx + 1)
        }

        assert_eq!(use_slot(Slot::new(10)), Some(11));
        assert_eq!(use_slot(Slot::NONE), None);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serde_roundtrip() {
        let slot = Slot::new(42);
        let json = serde_json::to_string(&slot).unwrap();
        assert_eq!(json, "42");
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, slot);

        let none = Slot::NONE;
        let json = serde_json::to_string(&none).unwrap();
        assert_eq!(json, format!("{}", u32::MAX));
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, none);
    }
}
