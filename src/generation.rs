//! Generation counter with embedded element state.
//!
//! This module provides type-safe encapsulation of the generation/version system
//! used for ABA protection in the pool allocator.
//!
//! # Encoding
//!
//! A `Generation` is a 32-bit value that packs two pieces of information:
//!
//! ```text
//! ┌────────────────────────────────────┬───────────────────┐
//! │     Generation Counter (30 bits)   │   State (2 bits)  │
//! │     0 - 1,073,741,823              │   00 01 10 11     │
//! └────────────────────────────────────┴───────────────────┘
//! ```
//!
//! # States
//!
//! Each element transitions through a well-defined state machine:
//!
//! ```text
//!     ┌───────────────────────────────────────────┐
//!     │                                           │
//!     ▼                                           │
//!  ┌──────┐  index_new()    ┌────────┐            │
//!  │ Free │ ──────────────► │ Zombie │            │
//!  └──────┘                 └────────┘            │
//!     ▲                          │                │
//!     │                          │ make_used()    │
//!     │ make_free()              ▼                │
//!     │                     ┌────────┐            │
//!     └──────────────────── │  Used  │ ───────────┘
//!                           └────────┘   make_zombie()
//!                                │
//!                                │ make_sentinel() (from Zombie)
//!                                ▼
//!                           ┌──────────┐
//!                           │ Sentinel │
//!                           └──────────┘
//! ```
//!
//! # Size Guarantee
//!
//! The `Generation` type is guaranteed to be exactly 32 bits:
//!
//! ```rust
//! # use pie_core::generation::Generation;
//! assert_eq!(core::mem::size_of::<Generation>(), 4);
//! ```

use core::fmt;

// ============================================================================
// Constants (private)
// ============================================================================

/// Mask to extract the state bits (low 2 bits).
const STATE_MASK: u32 = 0b11;

/// Amount to add for incrementing generation (skips state bits).
const GEN_INCREMENT: u32 = 0b100;

// ============================================================================
// ElemState Enum
// ============================================================================

/// The lifecycle state of a pool element.
///
/// Elements transition through these states during their lifetime:
/// - `Free` → `Zombie` (allocation)
/// - `Zombie` → `Used` (data added)
/// - `Used` → `Zombie` (data removed)
/// - `Zombie` → `Free` (returned to free list)
/// - `Zombie` → `Sentinel` (special: list sentinel creation)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ElemState {
    /// Element is on the free list, available for allocation.
    /// Links point to adjacent free list elements.
    Free = 0b00,

    /// Element contains user data and is linked in a PieList.
    Used = 0b01,

    /// Element is a list sentinel (no data, but has links).
    /// Each PieList has exactly one sentinel.
    Sentinel = 0b10,

    /// Element had its data removed but hasn't been returned to free list.
    /// Used as an intermediate state during deletion.
    Zombie = 0b11,
}

impl ElemState {
    /// Converts from the raw 2-bit encoding.
    #[inline]
    const fn from_bits(bits: u32) -> Self {
        match bits & STATE_MASK {
            0b00 => Self::Free,
            0b01 => Self::Used,
            0b10 => Self::Sentinel,
            _ => Self::Zombie, // 0b11
        }
    }

    /// Returns the raw 2-bit encoding.
    #[inline]
    pub const fn as_bits(self) -> u32 {
        self as u32
    }
}

impl fmt::Display for ElemState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Free => write!(f, "Free"),
            Self::Used => write!(f, "Used"),
            Self::Sentinel => write!(f, "Sentinel"),
            Self::Zombie => write!(f, "Zombie"),
        }
    }
}

// ============================================================================
// Generation Type
// ============================================================================

/// A 32-bit value encoding a generation counter (30 bits) and element state (2 bits).
///
/// This type provides type-safe access to the generation/state encoding used
/// throughout the pool allocator. It is `#[repr(transparent)]` to guarantee
/// the same memory layout as a raw `u32`.
///
/// # Examples
///
/// ```rust
/// use pie_core::generation::{Generation, ElemState};
///
/// let g = Generation::new(ElemState::Free);
/// assert!( g.is_free());
/// assert_eq!( g.state(), ElemState::Free);
/// assert_eq!( g.counter(), 0);
///
/// // Bump generation and change state
/// let gen2 = g.bump_to(ElemState::Zombie);
/// assert!(gen2.is_zombie());
/// assert_eq!(gen2.counter(), 1);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Generation(u32);

// Compile-time size verification
const _: () = assert!(core::mem::size_of::<Generation>() == 4);

impl Generation {
    // ========================================================================
    // Construction
    // ========================================================================

    /// Creates a new generation with counter = 0 and the given state.
    #[inline]
    pub const fn new(state: ElemState) -> Self {
        Self(state.as_bits())
    }

    /// Creates a generation from a raw u32 value.
    ///
    /// This is primarily for deserialization and internal use.
    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw u32 value.
    ///
    /// This is primarily for serialization and internal use.
    #[inline]
    pub const fn as_raw(self) -> u32 {
        self.0
    }

    // ========================================================================
    // State Accessors
    // ========================================================================

    /// Returns the current element state.
    #[inline]
    pub const fn state(self) -> ElemState {
        ElemState::from_bits(self.0)
    }

    /// Returns the generation counter (0 to ~1 billion).
    #[inline]
    pub const fn counter(self) -> u32 {
        self.0 >> 2
    }

    /// Checks if the element is free (on the free list).
    #[inline]
    pub const fn is_free(self) -> bool {
        (self.0 & STATE_MASK) == ElemState::Free as u32
    }

    /// Checks if the element is used (contains user data).
    #[inline]
    pub const fn is_used(self) -> bool {
        (self.0 & STATE_MASK) == ElemState::Used as u32
    }

    /// Checks if the element is a sentinel (list head/tail marker).
    #[inline]
    pub const fn is_sentinel(self) -> bool {
        (self.0 & STATE_MASK) == ElemState::Sentinel as u32
    }

    /// Checks if the element is a zombie (data removed, awaiting free).
    #[inline]
    pub const fn is_zombie(self) -> bool {
        (self.0 & STATE_MASK) == ElemState::Zombie as u32
    }

    // ========================================================================
    // State Transitions
    // ========================================================================

    /// Bumps the generation counter and sets a new state.
    ///
    /// Returns the new generation value.
    #[inline]
    pub const fn bump_to(self, new_state: ElemState) -> Self {
        let clean = self.0 & !STATE_MASK;
        Self(clean.wrapping_add(GEN_INCREMENT) | new_state.as_bits())
    }

    /// Changes state without bumping generation.
    ///
    /// Used for state transitions that don't require ABA protection
    /// (e.g., Used → Zombie when data is removed but element stays linked).
    #[inline]
    pub const fn with_state(self, new_state: ElemState) -> Self {
        Self((self.0 & !STATE_MASK) | new_state.as_bits())
    }

    // ========================================================================
    // Comparison
    // ========================================================================

    /// Checks if two generations match (same counter and state).
    #[inline]
    pub const fn matches(self, other: Self) -> bool {
        self.0 == other.0
    }

    /// Checks if two generations have the same counter (ignoring state).
    ///
    /// Used for zombie matching during deletion.
    #[inline]
    pub const fn same_counter(self, other: Self) -> bool {
        (self.0 & !STATE_MASK) == (other.0 & !STATE_MASK)
    }
}

impl Default for Generation {
    /// Returns a Free generation with counter = 0.
    #[inline]
    fn default() -> Self {
        Self::new(ElemState::Free)
    }
}

impl fmt::Debug for Generation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Gen({}, {:?})", self.counter(), self.state())
    }
}

impl fmt::Display for Generation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.counter(), self.state())
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Size and Layout Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_generation_size() {
        assert_eq!(core::mem::size_of::<Generation>(), 4);
        assert_eq!(core::mem::align_of::<Generation>(), 4);
    }

    #[test]
    fn test_elem_state_size() {
        assert_eq!(core::mem::size_of::<ElemState>(), 1);
    }

    // ------------------------------------------------------------------------
    // ElemState Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_state_bits_roundtrip() {
        for state in [ElemState::Free, ElemState::Used, ElemState::Sentinel, ElemState::Zombie] {
            let bits = state.as_bits();
            let recovered = ElemState::from_bits(bits);
            assert_eq!(state, recovered);
        }
    }

    #[test]
    fn test_state_bit_values() {
        assert_eq!(ElemState::Free.as_bits(), 0b00);
        assert_eq!(ElemState::Used.as_bits(), 0b01);
        assert_eq!(ElemState::Sentinel.as_bits(), 0b10);
        assert_eq!(ElemState::Zombie.as_bits(), 0b11);
    }

    #[test]
    fn test_state_display() {
        assert_eq!(format!("{}", ElemState::Free), "Free");
        assert_eq!(format!("{}", ElemState::Used), "Used");
        assert_eq!(format!("{}", ElemState::Sentinel), "Sentinel");
        assert_eq!(format!("{}", ElemState::Zombie), "Zombie");
    }

    // ------------------------------------------------------------------------
    // Generation Construction Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_new_free() {
        let g = Generation::new(ElemState::Free);
        assert!( g.is_free());
        assert!(! g.is_used());
        assert!(! g.is_sentinel());
        assert!(! g.is_zombie());
        assert_eq!( g.counter(), 0);
        assert_eq!( g.state(), ElemState::Free);
    }

    #[test]
    fn test_new_used() {
        let g = Generation::new(ElemState::Used);
        assert!(! g.is_free());
        assert!( g.is_used());
        assert!(! g.is_sentinel());
        assert!(! g.is_zombie());
        assert_eq!( g.counter(), 0);
    }

    #[test]
    fn test_new_sentinel() {
        let g = Generation::new(ElemState::Sentinel);
        assert!(! g.is_free());
        assert!(! g.is_used());
        assert!( g.is_sentinel());
        assert!(! g.is_zombie());
        assert_eq!( g.counter(), 0);
    }

    #[test]
    fn test_new_zombie() {
        let g = Generation::new(ElemState::Zombie);
        assert!(! g.is_free());
        assert!(! g.is_used());
        assert!(! g.is_sentinel());
        assert!( g.is_zombie());
        assert_eq!( g.counter(), 0);
    }

    #[test]
    fn test_default() {
        let g = Generation::default();
        assert!( g.is_free());
        assert_eq!( g.counter(), 0);
    }

    // ------------------------------------------------------------------------
    // Raw Value Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_from_raw_roundtrip() {
        for raw in [0u32, 1, 4, 7, 100, 0xFFFF_FFFF, 0x1234_5678] {
            let g = Generation::from_raw(raw);
            assert_eq!( g.as_raw(), raw);
        }
    }

    #[test]
    fn test_raw_encoding() {
        // Counter = 0, State = Free (0b00)
        assert_eq!(Generation::new(ElemState::Free).as_raw(), 0b00);
        // Counter = 0, State = Used (0b01)
        assert_eq!(Generation::new(ElemState::Used).as_raw(), 0b01);
        // Counter = 0, State = Sentinel (0b10)
        assert_eq!(Generation::new(ElemState::Sentinel).as_raw(), 0b10);
        // Counter = 0, State = Zombie (0b11)
        assert_eq!(Generation::new(ElemState::Zombie).as_raw(), 0b11);
    }

    // ------------------------------------------------------------------------
    // State Transition Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_bump_increments_counter() {
        let g = Generation::new(ElemState::Free);
        assert_eq!( g.counter(), 0);

        let g = g.bump_to(ElemState::Zombie);
        assert_eq!( g.counter(), 1);
        assert!( g.is_zombie());

        let g = g.bump_to(ElemState::Used);
        assert_eq!( g.counter(), 2);
        assert!( g.is_used());

        let g = g.bump_to(ElemState::Free);
        assert_eq!( g.counter(), 3);
        assert!( g.is_free());
    }

    #[test]
    fn test_with_state_preserves_counter() {
        let g = Generation::new(ElemState::Free);
        let g = g.bump_to(ElemState::Zombie); // counter = 1
        let g = g.bump_to(ElemState::Used);   // counter = 2

        // Change to zombie without bumping
        let gen2 = g.with_state(ElemState::Zombie);
        assert!(gen2.is_zombie());
        assert_eq!(gen2.counter(), 2); // Counter preserved!
    }

    #[test]
    fn test_typical_lifecycle() {
        // Free → Zombie (allocation)
        let g = Generation::new(ElemState::Free);
        let g = g.bump_to(ElemState::Zombie);
        assert!( g.is_zombie());
        assert_eq!( g.counter(), 1);

        // Zombie → Used (data added)
        let g = g.bump_to(ElemState::Used);
        assert!( g.is_used());
        assert_eq!( g.counter(), 2);

        // Used → Zombie (data removed, no bump)
        let g = g.with_state(ElemState::Zombie);
        assert!( g.is_zombie());
        assert_eq!( g.counter(), 2); // Same counter

        // Zombie → Free (returned to free list)
        let g = g.bump_to(ElemState::Free);
        assert!( g.is_free());
        assert_eq!( g.counter(), 3);
    }

    #[test]
    fn test_sentinel_lifecycle() {
        // Free → Zombie (allocation)
        let g = Generation::new(ElemState::Free);
        let g = g.bump_to(ElemState::Zombie);

        // Zombie → Sentinel
        let g = g.bump_to(ElemState::Sentinel);
        assert!( g.is_sentinel());
        assert_eq!( g.counter(), 2);
    }

    #[test]
    fn test_counter_wrapping() {
        // Create a generation near the max counter value
        let max_counter = (u32::MAX >> 2) - 1;
        let raw = (max_counter << 2) | ElemState::Used.as_bits();
        let g = Generation::from_raw(raw);
        assert_eq!( g.counter(), max_counter);

        // Bump should wrap around
        let g = g.bump_to(ElemState::Free);
        // The counter wraps, but the state is correct
        assert!( g.is_free());
    }

    // ------------------------------------------------------------------------
    // Comparison Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_matches() {
        let g1 = Generation::new(ElemState::Used);
        let g2 = Generation::new(ElemState::Used);
        assert!(g1.matches(g2));

        let g3 = g1.bump_to(ElemState::Used);
        assert!(!g1.matches(g3)); // Different counter

        let g4 = g1.with_state(ElemState::Zombie);
        assert!(!g1.matches(g4)); // Different state
    }

    #[test]
    fn test_same_counter() {
        let g1 = Generation::from_raw(0b100_01); // counter=1, Used
        let g2 = Generation::from_raw(0b100_11); // counter=1, Zombie

        assert!(g1.same_counter(g2));
        assert!(!g1.matches(g2)); // Different state

        let g3 = Generation::from_raw(0b1000_01); // counter=2, Used
        assert!(!g1.same_counter(g3)); // Different counter
    }

    #[test]
    fn test_equality() {
        let g1 = Generation::new(ElemState::Free);
        let g2 = Generation::new(ElemState::Free);
        assert_eq!(g1, g2);

        let g3 = Generation::new(ElemState::Used);
        assert_ne!(g1, g3);
    }

    // ------------------------------------------------------------------------
    // Debug/Display Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_debug_format() {
        let g = Generation::new(ElemState::Free);
        assert_eq!(format!("{:?}", g), "Gen(0, Free)");

        let g = g.bump_to(ElemState::Used);
        assert_eq!(format!("{:?}", g), "Gen(1, Used)");

        let g = g.bump_to(ElemState::Zombie);
        assert_eq!(format!("{:?}", g), "Gen(2, Zombie)");
    }

    #[test]
    fn test_display_format() {
        let g = Generation::new(ElemState::Sentinel);
        assert_eq!(format!("{}", g), "0:Sentinel");

        let g = g.bump_to(ElemState::Used);
        assert_eq!(format!("{}", g), "1:Used");
    }

    // ------------------------------------------------------------------------
    // Hash Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(Generation::new(ElemState::Free));
        set.insert(Generation::new(ElemState::Used));
        set.insert(Generation::new(ElemState::Free).bump_to(ElemState::Free));

        assert_eq!(set.len(), 3);
        assert!(set.contains(&Generation::new(ElemState::Free)));
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_all_bits_set() {
        let g = Generation::from_raw(u32::MAX);
        assert!( g.is_zombie()); // 0b11
        assert_eq!( g.counter(), u32::MAX >> 2);
    }

    #[test]
    fn test_counter_does_not_affect_state() {
        // High counter value with each state
        for state in [ElemState::Free, ElemState::Used, ElemState::Sentinel, ElemState::Zombie] {
            let raw = (0x3FFF_FFFC) | state.as_bits();
            let g = Generation::from_raw(raw);
            assert_eq!( g.state(), state);
        }
    }

    #[test]
    fn test_generation_overflow_wraps() {
        // Start near the maximum counter value.
        // With 30 bits for counter, max counter = (2^30 - 1) = 0x3FFF_FFFF.
        // The raw value for max counter with USED state:
        let max_counter_raw = (0x3FFF_FFFF << 2) | ElemState::Used as u32;
        let g = Generation::from_raw(max_counter_raw);
        assert_eq!(g.counter(), 0x3FFF_FFFF);
        assert_eq!(g.state(), ElemState::Used);
        // Bump should wrap around to counter 0.
        let bumped = g.bump_to(ElemState::Free);
        assert_eq!(bumped.counter(), 0);
        assert_eq!(bumped.state(), ElemState::Free);
    }

    #[test]
    fn test_generation_overflow_cycle() {
        // Simulate many bumps to verify wrapping is stable.
        let mut g = Generation::new(ElemState::Free);
        assert_eq!(g.counter(), 0);
        // Bump many times and verify state is always correct.
        for _ in 0..100 {
            g = g.bump_to(ElemState::Used);
            assert_eq!(g.state(), ElemState::Used);
            g = g.bump_to(ElemState::Free);
            assert_eq!(g.state(), ElemState::Free);
        }
        assert_eq!(g.counter(), 200);
    }
}
