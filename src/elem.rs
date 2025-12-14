//! Definition of the generic `ListElem<T>` type.

use crate::index::Index;
use core::{fmt, mem::{self, MaybeUninit}};

// --- Bitwise State Management ---
// Layout: [ ... Generation (30 bits) ... | State (2 bits) ]
pub(crate) const STATE_MASK: u32 = 0b11;
pub(crate) const STATE_FREE: u32 = 0b00;     // 0
pub(crate) const STATE_USED: u32 = 0b01;     // 1
pub(crate) const STATE_SENTINEL: u32 = 0b10; // 2
pub(crate) const STATE_ZOMBIE: u32 = 0b11;   // 3 (Used but data taken)
const GEN_INCREMENT: u32 = 0b100; // Adds 1 to the generation part

/// The fundamental node structure for a doubly-linked list.
pub struct Elem<T> {
    pub(crate) next: Index<T>,
    pub(crate) prev: Index<T>,
    /// Stores both the generation count and the state (Free/Used/Sentinel).
    pub(crate) vers: u32,
    pub(crate) data: MaybeUninit<T>,
}

impl<T> Clone for Elem<T>
where T: Clone {
    fn clone(&self) -> Self {
        Self {
            next: self.next,
            prev: self.prev,
            vers: self.vers,
            data: if self.is_used() {
                // SAFETY: is_used() checks the state bits, confirming data initialization.
                #[allow(unsafe_code)]
                MaybeUninit::new(unsafe { self.data.assume_init_ref().clone() })
            } else {
                MaybeUninit::uninit()
            },
        }
    }
}

// Manual implementation of PartialEq
impl<T: PartialEq> PartialEq for Elem<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.vers != other.vers || self.next != other.next || self.prev != other.prev {
            return false;
        }
        if self.is_used() {
            // SAFETY: is_used() confirms data is initialized
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

// Manual implementation of Debug
impl<T: fmt::Debug> fmt::Debug for Elem<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("ListElem");
        dbg.field("next", &self.next);
        dbg.field("prev", &self.prev);
        dbg.field("vers", &self.vers);
        if self.is_used() {
            // SAFETY: Checked is_used()
            #[allow(unsafe_code)]
            dbg.field("data", unsafe { self.data.assume_init_ref() });
        } else if self.is_sentinel() {
            dbg.field("data", &"<sentinel>");
        } else if self.is_zombie() {
            dbg.field("data", &"<zombie>");
        } else {
            dbg.field("data", &"<free>");
        }
        dbg.finish()
    }
}

impl<T> Default for Elem<T> {
    fn default() -> Self {
        Self {
            next: Index::NONE,
            prev: Index::NONE,
            vers: STATE_FREE, // Default is free
            data: MaybeUninit::uninit(),
        }
    }
}

impl<T> Elem<T> {
    /// A builder-style method to set the data for this element.
    #[inline]
    pub fn with_data(mut self, data: T) -> Self {
        self.data = MaybeUninit::new(data);
        // Retain generation, set state to USED
        self.vers = (self.vers & !STATE_MASK) | STATE_USED;
        self
    }

    /// A builder-style method to set both `next` and `prev` links.
    #[inline]
    pub fn with_both(mut self, index: Index<T>) -> Self {
        self.next = index;
        self.prev = index;
        self
    }

    // --- State Checkers ---

    #[inline(always)]
    pub fn is_sentinel(&self) -> bool {
        (self.vers & STATE_MASK) == STATE_SENTINEL
    }

    /// Checks if the element is in use (i.e., contains user data).
    #[inline]
    pub fn is_used(&self) -> bool {
        (self.vers & STATE_MASK) == STATE_USED
    }

    #[inline]
    pub fn is_free(&self) -> bool {
        (self.vers & STATE_MASK) == STATE_FREE
    }

    #[inline]
    pub fn is_zombie(&self) -> bool {
        (self.vers & STATE_MASK) == STATE_ZOMBIE
    }

    // --- State Transitions ---

    /// Bumps the generation count and sets the state to `new_state`.
    /// Returns the new version integer.
    pub(crate) fn bump_gen(&mut self, new_state: u32) -> u32 {
        // 1. Clear the old state bits
        let clean_vers = self.vers & !STATE_MASK;
        // 2. Increment the generation part (handles wrapping automatically via overflow)
        // 3. OR in the new state
        self.vers = clean_vers.wrapping_add(GEN_INCREMENT) | new_state;
        self.vers
    }

    /// Transitions a free node to a used node, initializing it with data.
    /// Returns the new version number for the Index.
    pub fn make_used(&mut self, data: T) -> u32 {
        debug_assert!(self.is_free(), "Element must be free to become used");
        self.data = MaybeUninit::new(data);
        self.bump_gen(STATE_USED)
    }

    /// Transitions a used/sentinel node to a free node, dropping data if present.
    /// Returns the new version number.
    pub fn make_free(&mut self) -> u32 {
        debug_assert!(!self.is_free(), "Element must not already be free");
        if self.is_used() {
            // SAFETY: We are dropping the data.
            #[allow(unsafe_code)]
            unsafe { self.data.assume_init_drop(); }
        }
        // If it was Zombie, data is already gone, so we just transition.

        // Safety: Prevent using old data bits
        self.data = MaybeUninit::uninit();
        self.bump_gen(STATE_FREE)
    }

    /// Transitions a free node to a sentinel.
    /// Returns the new version number.
    pub fn make_sentinel(&mut self) -> u32 {
        debug_assert!(self.is_free(), "Element must be free to become a sentinel");
        self.bump_gen(STATE_SENTINEL)
    }

    /// Force sets the state to sentinel (used during init).
    pub(crate) fn force_sentinel(&mut self) {
        self.vers = (self.vers & !STATE_MASK) | STATE_SENTINEL;
    }

    #[inline]
    pub fn new_next(&mut self, next: Index<T>) -> Index<T> {
        mem::replace(&mut self.next, next)
    }

    #[inline]
    pub fn new_prev(&mut self, prev: Index<T>) -> Index<T> {
        mem::replace(&mut self.prev, prev)
    }

    #[inline]
    pub fn new_links(&mut self, prev: Index<T>, next: Index<T>) -> (Index<T>, Index<T>) {
        let old_prev = mem::replace(&mut self.prev, prev);
        let old_next = mem::replace(&mut self.next, next);
        (old_prev, old_next)
    }

    /// Replaces the data in this element with `new_data`.
    /// Returns `Some(old_data)` if the element was previously in use, or `None` if it was free/sentinel.
    pub fn replace_data(&mut self, new_data: T) -> Option<T> {
        let old_data = if self.is_used() {
            // SAFETY: We are about to overwrite this data.
            #[allow(unsafe_code)]
            Some(unsafe { self.data.assume_init_read() })
        } else {
            None
        };

        // If we are Used OR Zombie, we become Used with data.
        if self.is_used() || self.is_zombie() {
            self.data = MaybeUninit::new(new_data);
            // Ensure state is USED (fix Zombie state)
            self.vers = (self.vers & !STATE_MASK) | STATE_USED;
        }
        old_data
    }

    /// Takes the data out of the element, transitioning it to a Zombie state.
    /// This preserves the generation count but changes the state bits,
    /// signaling that the data is gone but the node is not yet Free.
    pub(crate) fn take_data(&mut self) -> Option<T> {
        if self.is_used() {
            #[allow(unsafe_code)]
            let val = unsafe { self.data.assume_init_read() };
            // Transition to Zombie: Keep generation, set state to 11
            self.vers = (self.vers & !STATE_MASK) | STATE_ZOMBIE;
            Some(val)
        } else {
            None
        }
    }

    #[inline]
    pub fn links(&self) -> (Index<T>, Index<T>) {
        (self.prev, self.next)
    }
}

impl<T> fmt::Display for Elem<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.is_used() {
            write!(f, "<{}[#]{}>", self.prev, self.next)
        } else if self.is_sentinel() {
            write!(f, "<{}>|<{}>", self.prev, self.next)
        } else if self.is_zombie() {
            write!(f, "<{}[z]{}>", self.prev, self.next)
        } else {
            write!(f, "<{}<->{}>", self.prev, self.next)
        }
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use super::{Elem, Index};
    use core::mem::MaybeUninit;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    // Proxy for Serialization: Uses a reference (&T) to avoid cloning
    #[derive(Serialize)]
    struct SerializeProxy<'a, T> {
        next: Index<T>,
        prev: Index<T>,
        vers: u32,
        data: Option<&'a T>,
    }

    // Proxy for Deserialization: Must own the data (T)
    #[derive(Deserialize)]
    #[serde(bound(deserialize = "T: Deserialize<'de>"))]
    struct DeserializeProxy<T> {
        next: Index<T>,
        prev: Index<T>,
        vers: u32,
        data: Option<T>,
     }

    impl<T: Serialize> Serialize for Elem<T> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let data = if self.is_used() {
                #[allow(unsafe_code)]
                Some(unsafe { self.data.assume_init_ref() })
            } else {
                None
            };
            let proxy = SerializeProxy {
                next: self.next,
                prev: self.prev,
                vers: self.vers,
                data,
            };
            proxy.serialize(serializer)
        }
    }

    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Elem<T> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let proxy = DeserializeProxy::<T>::deserialize(deserializer)?;
            let data = if let Some(data) = proxy.data {
                MaybeUninit::new(data)
            } else {
                MaybeUninit::uninit()
            };
            Ok(Elem {
                next: proxy.next,
                prev: proxy.prev,
                vers: proxy.vers,
                data,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct MyElemData {
        value: i32,
    }

    impl fmt::Display for MyElemData {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", self.value)
        }
    }

    #[test]
    fn test_default_creation() {
        let elem = Elem::<MyElemData>::default();
        assert_eq!(elem.next, Index::NONE);
        assert_eq!(elem.prev, Index::NONE);
        assert!(!elem.is_used());
    }

    #[test]
    fn test_builder_methods_and_equality() {
        let data = MyElemData { value: 100 };
        let index = Index::<MyElemData>::from(42_u32);

        let elem1 = Elem::default().with_data(data);
        let elem2 = Elem::default().with_data(data);

        assert_eq!(elem1, elem2);
        assert!(elem1.is_used());

        let elem3 = Elem::default().with_data(MyElemData { value: 200 });
        assert_ne!(elem1, elem3);

        let elem_links = Elem::default().with_both(index);
        assert_eq!(elem_links.next, index);
        assert_eq!(elem_links.prev, index);
        assert!(!elem_links.is_used());
    }

    #[test]
    fn test_replace_data() {
        let mut elem = Elem::default().with_data(MyElemData { value: 10 });

        // Replace existing data
        let old = elem.replace_data(MyElemData { value: 20 });
        assert_eq!(old, Some(MyElemData { value: 10 }));
        #[allow(unsafe_code)]
        let data = unsafe { elem.data.assume_init_ref() };
        assert_eq!(*data, MyElemData { value: 20 });

        // Replace on a free node
        let mut free_elem = Elem::<MyElemData>::default();
        assert!(free_elem.is_free());
        let old_free = free_elem.replace_data(MyElemData { value: 99 });
        assert_eq!(old_free, None);
        assert!(!free_elem.is_used()); // replacing data on free node should not make it used
    }

    #[test]
    fn test_debug_impl() {
        let elem = Elem::default().with_data(MyElemData { value: 55 });
        let debug_str = format!("{:?}", elem);
        // Ensure debug implementation is stable
        assert!(debug_str.contains("ListElem"));
        assert!(debug_str.contains("data: MyElemData"));
        assert!(debug_str.contains("55"));

        let empty_elem = Elem::<MyElemData>::default();
        let debug_str_empty = format!("{:?}", empty_elem);
        assert!(debug_str_empty.contains("<free>"));
    }
}
