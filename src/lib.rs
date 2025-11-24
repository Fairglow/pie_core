//! A high-performance, index-based data structure toolkit.
//!
//! `pie_core` provides data structures built on a central, contiguous
//! `ElemPool`. This design offers several key advantages over traditional
//! pointer-based collections:
//!
//! - **Cache-Friendly:** By storing nodes in a `Vec`, traversals are significantly
//!   faster due to improved data locality.
//! - **No Alloc/Dealloc Churn:** The pool manages memory, reusing freed nodes.
//!   This eliminates the overhead of frequent calls to the system allocator, making
//!   insertions and removals very fast.
//! - **Multi-Structure Support:** A single `ElemPool` can manage the memory for many
//!   separate `PieList` and `FibHeap` instances, making it ideal for managing
//!   numerous small data structures efficiently.
//! - **Safe Cursors:** The library provides a `CursorMut` API for complex, efficient
//!   in-place list mutations like splitting and splicing.
//! - **Efficient Priority Queue:** Includes a `FibHeap` implementation with O(1)
//!   amortized `decrease_key`, ideal for graph algorithms.
//!
//! This crate is a "Pie" library because it stores the data (`T`) *directly inside* the
//! element structure, which is efficient for smaller `T`. This avoids an extra
//! layer of indirection.
//!
//! # Core Concepts
//!
//! - [`ElemPool`]: The memory arena that owns all elements. All operations on a
//!   structure require a reference to the pool.
//! - [`PieList<T>`]: A lightweight handle representing a single doubly-linked list.
//! - [`FibHeap<K, V>`]: A Fibonacci heap (priority queue) optimized for
//!   fast `decrease_key` operations.
//! - [`Index<T>`]: A type-safe, copyable handle to a specific element within the pool.
//!   It acts as a "safe pointer".
//! - [`CursorMut<'a, T>`]: A mutable cursor for efficient `PieList` navigation
//!   and modification.
//! - [`FibHandle<K, V>`]: A type-safe handle to a node in a `FibHeap`,
//!   required for `decrease_key`.
//!
//! # Example
//!
//! ```
//! use pie_core::{ElemPool, PieList};
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
//! # list_b.clear(&mut pool);
//! ```

#![deny(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

// We need 'alloc' to be available unconditionally because we use 
// 'alloc::vec::Vec' in pool.rs to support both std and no_std targets.
extern crate alloc;
extern crate core;

// Central type alias for the map used in shrink_to_fit.
// This ensures pool, list, and heap all use the same type.
#[cfg(feature = "std")]
pub(crate) type IndexMap<K, V> = std::collections::HashMap<K, V>;

#[cfg(not(feature = "std"))]
pub(crate) type IndexMap<K, V> = alloc::collections::BTreeMap<K, V>;

// --- Module Declarations ---
mod cursor;
mod cursor_mut;
mod elem;
mod index;
mod list;
mod pool;
mod heap;
#[cfg(feature = "petgraph")]
mod petgraph;
mod view;
mod view_mut;

// --- Public API Re-exports ---
pub use cursor::Cursor;
pub use cursor_mut::CursorMut;
pub use elem::ListElem;
pub use index::Index;
pub use list::PieList;
pub use view::PieView;
pub use view_mut::PieViewMut;
pub use pool::{ElemPool, IndexError};
pub use heap::{FibHeap, FibHandle, Node, DecreaseKeyError};
#[cfg(feature = "petgraph")]
pub use petgraph::dijkstra_pie_core;
