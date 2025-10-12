//! A high-performance, index-based linked list optimized for embedded data.
//!
//! PieList is a variant of MindList designed for simplicity and maximum performance
//! when the data (`T`) is small and can be stored directly with the list's
//! structural elements. This avoids one layer of indirection compared to MindList,
//! potentially improving cache locality.
//!

#![deny(unsafe_code)]

// --- Module Declarations ---
// mod cursor;
mod elem;
mod index;
mod pool;
mod list;

// --- Public API Re-exports ---
// pub use cursor::CursorMut;
pub use elem::ListElem;
pub use index::Index;
pub use list::PieList;
pub use pool::{ElemPool, IndexError};
