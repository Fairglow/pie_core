//! Definition of the generic `Elem<T>` type — the fundamental node structure.
//!
//! # Internal Architecture
//!
//! Each `Elem<T>` represents a single node that can participate in doubly-linked
//! lists. The key insight is that the element itself contains the list pointers
//! (`next`/`prev`), making this an *intrusive* linked list design.
//!
//! ## Memory Layout
//!
//! ```text
//! Elem<T>
//! +--------------------------------+
//! | next: Slot      (4 bytes)      |  <- Compact slot index (u32)
//! | prev: Slot      (4 bytes)      |  <- Compact slot index (u32)
//! | vers: Generation (4 bytes)     |  <- 30-bit counter + 2-bit state
//! | data: MaybeUninit<T> (N bytes) |  <- User data (inline)
//! +--------------------------------+
//!  Total: 12 + size_of::<T>() bytes
//! ```
//!
//! ## Compact Links with Slot Type
//!
//! Previously, `next`/`prev` were `Index<T>` (8 bytes each, including generation).
//! The generation in links was redundant for internal navigation because:
//! - Internal links are always valid (maintained by pool operations)
//! - Generation is only needed for external API (user-held handles)
//!
//! Now internal links are just `Slot` (4 bytes each), saving 8 bytes per element
//! in link storage. External `Index<T>` handles still contain the full generation.
//!
//! ## State Machine
//!
//! Each element has a 4-state lifecycle encoded in the low 2 bits of `vers`:
//!
//! ```text
//! vers: u32 (via Generation type)
//! +------------------------------------+-------------------+
//! |     Generation (30 bits)           |   State (2 bits)  |
//! |     0x00000000 - 0x3FFFFFFF        |   00 01 10 11     |
//! +------------------------------------+-------------------+
//!
//! States:
//!   FREE (00)     - On free list, no data, links to free list neighbors
//!   USED (01)     - Contains user data, linked in a PieList
//!   SENTINEL (10) - List sentinel, no data, links to head/tail
//!   ZOMBIE (11)   - Data removed but not yet returned to free list
//! ```
//!
//! ## Transitions
//!
//! ```text
//!     +-------------------------------------------+
//!     |                                           |
//!     v                                           |
//!  +------+  index_new()    +--------+            |
//!  | FREE | --------------> | ZOMBIE |            |
//!  +------+                 +--------+            |
//!     ^                          |                |
//!     |                          | data_swap(Some)|
//!     | index_del()              v                |
//!     |                     +--------+            |
//!     +-------------------- |  USED  | -----------+
//!                           +--------+   data_swap(None)
//!                                |
//!          index_make_sentinel() | (from ZOMBIE)
//!                                v
//!                           +----------+
//!                           | SENTINEL |
//!                           +----------+
//! ```
//!
//! ## Why ZOMBIE?
//!
//! The Zombie state enables two-phase deletion:
//! 1. `data_swap(None)` — Extract data, element becomes Zombie (still linked)
//! 2. `index_del()` — Unlink and return to free list (Zombie → Free)
//!
//! This separation is essential for:
//! - Safe cursor operations during iteration
//! - FibHeap's pop() which rearranges nodes before freeing
//! - Maintaining link integrity during complex operations

use core::{fmt, mem::{self, MaybeUninit}};
use crate::generation::{Generation, ElemState};
use crate::slot::Slot;

// ============================================================================
// Re-exports for backward compatibility
// ============================================================================

/// Mask to extract the state bits from a raw `u32` version.
pub(crate) const STATE_MASK: u32 = 0b11;

/// Element contains user data and is linked in a PieList.
pub(crate) const STATE_USED: u32 = ElemState::Used as u32;

/// The fundamental node structure for a doubly-linked list.
///
/// This struct contains element metadata (links + generation/state) and user data.
/// The compact `Slot` type is used for links instead of full `Index<T>`, saving
/// 8 bytes per element compared to storing full indices.
pub struct Elem<T> {
    /// Slot index of the next element (`Slot::NONE` = no link).
    pub(crate) next: Slot,
    /// Slot index of the previous element (`Slot::NONE` = no link).
    pub(crate) prev: Slot,
    /// Generation counter and element state.
    pub(crate) vers: Generation,
    /// User data (only valid when state is USED).
    pub(crate) data: MaybeUninit<T>,
}

impl<T: Clone> Clone for Elem<T> {
    fn clone(&self) -> Self {
        Self {
            next: self.next,
            prev: self.prev,
            vers: self.vers,
            data: if self.is_used() {
                #[allow(unsafe_code)]
                MaybeUninit::new(unsafe { self.data.assume_init_ref().clone() })
            } else {
                MaybeUninit::uninit()
            },
        }
    }
}

impl<T: PartialEq> PartialEq for Elem<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.vers != other.vers || self.next != other.next || self.prev != other.prev {
            return false;
        }
        if self.is_used() {
            #[allow(unsafe_code)]
            unsafe {
                self.data.assume_init_ref() == other.data.assume_init_ref()
            }
        } else {
            true
        }
    }
}

impl<T: Eq> Eq for Elem<T> {}

impl<T: fmt::Debug> fmt::Debug for Elem<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("Elem");
        dbg.field("next", &self.next);
        dbg.field("prev", &self.prev);
        dbg.field("vers", &self.vers);
        dbg.field("state", &self.state());
        if self.is_used() {
            #[allow(unsafe_code)]
            dbg.field("data", unsafe { self.data.assume_init_ref() });
        }
        dbg.finish()
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for Elem<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Elem", 4)?;
        state.serialize_field("next", &self.next)?;
        state.serialize_field("prev", &self.prev)?;
        state.serialize_field("vers", &self.vers.as_raw())?;
        if self.is_used() {
            #[allow(unsafe_code)]
            let data_ref: Option<&T> = Some(unsafe { self.data.assume_init_ref() });
            state.serialize_field("data", &data_ref)?;
        } else {
            state.serialize_field::<Option<T>>("data", &None)?;
        }
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Elem<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ElemData<T> {
            next: Slot,
            prev: Slot,
            vers: u32,
            data: Option<T>,
        }

        let helper = ElemData::deserialize(deserializer)?;
        Ok(Self {
            next: helper.next,
            prev: helper.prev,
            vers: Generation::from_raw(helper.vers),
            data: match helper.data {
                Some(d) => MaybeUninit::new(d),
                None => MaybeUninit::uninit(),
            },
        })
    }
}

impl<T> Default for Elem<T> {
    fn default() -> Self {
        Self {
            next: Slot::NONE,
            prev: Slot::NONE,
            vers: Generation::new(ElemState::Free),
            data: MaybeUninit::uninit(),
        }
    }
}

impl<T> Elem<T> {
    /// Creates a new element with the given slot as both next and prev.
    /// Used for creating self-referencing sentinels.
    #[inline]
    pub(crate) fn new_self_ref(slot: Slot, state: ElemState) -> Self {
        Self {
            next: slot,
            prev: slot,
            vers: Generation::new(state),
            data: MaybeUninit::uninit(),
        }
    }



    // --- State Checkers (delegate to Generation) ---

    #[inline(always)]
    pub(crate) fn is_sentinel(&self) -> bool {
        self.vers.is_sentinel()
    }

    /// Checks if the element is in use (i.e., contains user data).
    #[inline]
    pub(crate) fn is_used(&self) -> bool {
        self.vers.is_used()
    }

    #[inline]
    pub(crate) fn is_free(&self) -> bool {
        self.vers.is_free()
    }

    #[inline]
    pub(crate) fn is_zombie(&self) -> bool {
        self.vers.is_zombie()
    }

    /// Returns the current element state.
    #[inline]
    pub(crate) fn state(&self) -> ElemState {
        self.vers.state()
    }

    /// Returns the generation as a raw u32 (for Index compatibility).
    #[inline]
    pub(crate) fn vers_raw(&self) -> u32 {
        self.vers.as_raw()
    }

    // --- State Transitions ---

    /// Bumps the generation count and sets the state to `new_state`.
    /// Returns the new version as a raw u32.
    #[inline]
    pub(crate) fn bump_gen(&mut self, new_state: ElemState) -> u32 {
        self.vers = self.vers.bump_to(new_state);
        self.vers.as_raw()
    }

    /// Transitions to USED state and bumps generation.
    /// Returns the new version number.
    #[inline]
    pub(crate) fn make_used(&mut self) -> u32 {
        debug_assert!(self.is_free() || self.is_zombie(), "Element must be free or zombie to become used");
        self.bump_gen(ElemState::Used)
    }

    /// Transitions to FREE state and bumps generation.
    /// Returns the new version number.
    #[inline]
    pub(crate) fn make_free(&mut self) -> u32 {
        debug_assert!(!self.is_free(), "Element must not already be free");
        self.bump_gen(ElemState::Free)
    }

    /// Transitions to ZOMBIE state (data removed but not yet freed).
    /// Preserves generation, only changes state bits.
    #[inline]
    pub(crate) fn make_zombie(&mut self) {
        debug_assert!(self.is_used(), "Element must be used to become zombie");
        self.vers = self.vers.with_state(ElemState::Zombie);
    }

    /// Transitions to SENTINEL state and bumps generation.
    /// Returns the new version number.
    #[inline]
    pub(crate) fn make_sentinel(&mut self) -> u32 {
        debug_assert!(self.is_free() || self.is_zombie(), "Element must be free or zombie to become sentinel");
        self.bump_gen(ElemState::Sentinel)
    }



    // --- Link Operations ---

    /// Sets the next link and returns the old value.
    #[inline]
    pub(crate) fn set_next(&mut self, next: Slot) -> Slot {
        mem::replace(&mut self.next, next)
    }

    /// Sets the prev link and returns the old value.
    #[inline]
    pub(crate) fn set_prev(&mut self, prev: Slot) -> Slot {
        mem::replace(&mut self.prev, prev)
    }

    /// Sets both links and returns the old values as (old_prev, old_next).
    #[inline]
    pub(crate) fn set_links(&mut self, prev: Slot, next: Slot) -> (Slot, Slot) {
        let old_prev = mem::replace(&mut self.prev, prev);
        let old_next = mem::replace(&mut self.next, next);
        (old_prev, old_next)
    }

    /// Returns both links as (prev, next).
    #[inline]
    pub(crate) fn links(&self) -> (Slot, Slot) {
        (self.prev, self.next)
    }

    // --- Data Access ---

    /// Returns a reference to the data (assumes USED state, caller must verify).
    ///
    /// # Safety
    /// Caller must ensure the element is in USED state.
    #[inline]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn data_ref_unchecked(&self) -> &T {
        unsafe { self.data.assume_init_ref() }
    }

    /// Returns a mutable reference to the data (assumes USED state, caller must verify).
    ///
    /// # Safety
    /// Caller must ensure the element is in USED state.
    #[inline]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn data_mut_unchecked(&mut self) -> &mut T {
        unsafe { self.data.assume_init_mut() }
    }

    /// Takes the data out (assumes USED state, caller must verify).
    ///
    /// # Safety
    /// Caller must ensure the element is in USED state and will handle state transition.
    #[inline]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn take_data_unchecked(&mut self) -> T {
        unsafe { self.data.assume_init_read() }
    }

    /// Writes data into the element's storage.
    ///
    /// Does NOT change the element state - caller must handle that.
    #[inline]
    pub(crate) fn write_data(&mut self, data: T) {
        self.data = MaybeUninit::new(data);
    }
}

impl<T: fmt::Display> fmt::Display for Elem<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let prev_str = self.prev.to_string();
        let next_str = self.next.to_string();

        if self.is_used() {
            #[allow(unsafe_code)]
            let data = unsafe { self.data.assume_init_ref() };
            write!(f, "<{}[{}]{}>", prev_str, data, next_str)
        } else if self.is_sentinel() {
            write!(f, "<{}>|<{}>", prev_str, next_str)
        } else if self.is_zombie() {
            write!(f, "<{}[z]{}>", prev_str, next_str)
        } else {
            write!(f, "<{}<->{}>", prev_str, next_str)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_creation() {
        let elem: Elem<i32> = Elem::default();
        assert_eq!(elem.next, Slot::NONE);
        assert_eq!(elem.prev, Slot::NONE);
        assert!(elem.is_free());
        assert!(!elem.is_used());
        assert!(!elem.is_sentinel());
        assert!(!elem.is_zombie());
    }

    #[test]
    fn test_self_ref_creation() {
        let elem: Elem<i32> = Elem::new_self_ref(Slot::new(42), ElemState::Sentinel);
        assert_eq!(elem.next, Slot::new(42));
        assert_eq!(elem.prev, Slot::new(42));
        assert!(elem.is_sentinel());
        assert_eq!(elem.state(), ElemState::Sentinel);
    }

    #[test]
    fn test_state_transitions() {
        let mut elem: Elem<i32> = Elem::default();
        assert!(elem.is_free());
        assert_eq!(elem.state(), ElemState::Free);

        // Free -> Used
        let vers1 = elem.make_used();
        assert!(elem.is_used());
        assert_eq!(elem.vers_raw(), vers1);
        assert_eq!(elem.state(), ElemState::Used);

        // Used -> Zombie (no bump)
        elem.make_zombie();
        assert!(elem.is_zombie());
        assert_eq!(elem.state(), ElemState::Zombie);
        // Generation counter unchanged
        assert!(elem.vers.same_counter(Generation::from_raw(vers1)));

        // Zombie -> Free
        let vers2 = elem.make_free();
        assert!(elem.is_free());
        // Generation bumped
        assert!(vers2 > vers1);
    }

    #[test]
    fn test_link_operations() {
        let mut elem: Elem<i32> = Elem::default();

        // Set individual links
        let old_next = elem.set_next(Slot::new(10));
        assert_eq!(old_next, Slot::NONE);
        assert_eq!(elem.next, Slot::new(10));

        let old_prev = elem.set_prev(Slot::new(20));
        assert_eq!(old_prev, Slot::NONE);
        assert_eq!(elem.prev, Slot::new(20));

        // Set both links
        let (old_prev, old_next) = elem.set_links(Slot::new(30), Slot::new(40));
        assert_eq!(old_prev, Slot::new(20));
        assert_eq!(old_next, Slot::new(10));
        assert_eq!(elem.links(), (Slot::new(30), Slot::new(40)));
    }

    #[test]
    fn test_generation_bump() {
        let mut elem: Elem<i32> = Elem::default();
        let gen0 = elem.vers.counter();

        elem.bump_gen(ElemState::Used);
        let gen1 = elem.vers.counter();
        assert_eq!(gen1, gen0 + 1);

        elem.bump_gen(ElemState::Free);
        let gen2 = elem.vers.counter();
        assert_eq!(gen2, gen1 + 1);
    }

    #[test]
    fn test_data_operations() {
        let mut elem: Elem<String> = Elem::default();

        // Write data and mark as used
        elem.write_data("hello".to_string());
        elem.make_used();

        // Now we can access data
        #[allow(unsafe_code)]
        unsafe {
            assert_eq!(elem.data_ref_unchecked(), "hello");

            // Mutate data
            *elem.data_mut_unchecked() = "world".to_string();
            assert_eq!(elem.data_ref_unchecked(), "world");

            // Take data
            let taken = elem.take_data_unchecked();
            assert_eq!(taken, "world");
        }
    }
}
