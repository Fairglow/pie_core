//! A high-performance, index-based linked list optimized for embedded data.
//!
//! `pielist` provides a doubly-linked list implementation that stores all its
//! elements in a central, contiguous `ElemPool`. This design offers several

//! key advantages over traditional pointer-based lists like `std::collections::LinkedList`:
//!
//! - **Cache-Friendly:** By storing nodes in a `Vec`, traversals are significantly
//!   faster due to improved data locality.
//! - **No Alloc/Dealloc Churn:** The pool manages memory, reusing freed nodes.
//!   This eliminates the overhead of frequent calls to the system allocator, making
//!   insertions and removals very fast.
//! - **Multi-List Support:** A single `ElemPool` can manage the memory for many
//!   separate `PieList` instances, making it ideal for scenarios where you need
//!   to manage numerous small lists, such as in game development or graph algorithms.
//! - **Safe Cursors:** The library provides a `CursorMut` API for complex, efficient
//!   in-place mutations like splitting and splicing lists, all while upholding
//!   Rust's borrow-checking rules.
//!
//! This crate is a "PieList" because it stores the data (`T`) *directly inside* the
//! list element structure, which is efficient for smaller `T`. This avoids an extra
//! layer of indirection compared to designs that store pointers to data.
//!
//! # Core Concepts
//!
//! - [`ElemPool`]: The memory arena that owns all list elements. All operations on a
//!   list require a mutable reference to the pool.
//! - [`PieList<T>`]: A lightweight handle representing a single doubly-linked list.
//!   It doesn't own the elements itself, but references them within the pool.
//! - [`Index<T>`]: A type-safe, copyable handle to a specific element within the pool.
//!   It acts as a "safe pointer".
//! - [`CursorMut<'a, T>`]: A mutable cursor that provides an efficient way to navigate
//!   and modify a list at a specific position.
//!
//! # Example
//!
//! ```
//! use pielist::{ElemPool, PieList};
//!
//! // 1. Create a pool to manage memory for all our lists.
//! // The pool can only hold one type of data. Here, we choose &'static str.
//! let mut pool: ElemPool<&'static str> = ElemPool::new();
//!
//! // 2. Create two separate list handles.
//! let mut list_a = PieList::new(&mut pool);
//! let mut list_b = PieList::new(&mut pool);
//!
//! // 3. Push data into the lists.
//! list_a.push_back("Apple", &mut pool).unwrap();
//! list_a.push_back("Banana", &mut pool).unwrap();
//!
//! // list_b must also use strings, as that is the pool's type.
//! list_b.push_front("Cat", &mut pool).unwrap();
//! list_b.push_front("Dog", &mut pool).unwrap();
//!
//! // 4. The pool now contains all 4 elements.
//! assert_eq!(pool.len(), 4);
//! assert_eq!(list_a.len(), 2);
//! assert_eq!(list_b.len(), 2);
//!
//! // 5. Iterate and access data.
//! let fruits: Vec<_> = list_a.iter(&pool).copied().collect();
//! assert_eq!(fruits, vec!["Apple", "Banana"]);
//!
//! // 6. When a list is no longer needed, clear it to return its
//! //    elements to the pool's free list for reuse.
//! list_a.clear(&mut pool);
//! assert_eq!(pool.len(), 2); // Only list_b's elements remain.
//! ```

#![deny(unsafe_code)]

// --- Module Declarations ---
mod cursor;
mod elem;
mod index;
mod list;
mod pool;

// --- Public API Re-exports ---
pub use cursor::CursorMut;
pub use elem::ListElem;
pub use index::Index;
pub use list::PieList;
pub use pool::{ElemPool, IndexError};
