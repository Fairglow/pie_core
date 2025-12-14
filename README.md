[![Rust](https://github.com/Fairglow/pie_core/actions/workflows/rust.yml/badge.svg)](https://github.com/Fairglow/pie_core/actions/workflows/rust.yml)

# **pie_core: High-Performance, Arena-Allocated Data Structures**

`pie_core` is a Rust crate that provides high-performance, arena-allocated data structure implementations, including a doubly-linked list and a Fibonacci heap (priority queue). It is built on an arena allocation (or "pool") model, which offers significant performance benefits over traditional node-based structures in specific scenarios.

This crate provides three main types:

- `ElemPool<T>`: An arena allocator that owns and manages the memory for all elements of a specific type.
- `PieList<T>`: A lightweight handle representing a single linked list whose elements are stored in a shared `ElemPool`.
  - `PieView<'a, T>`: A temporary read-only view that bundles a `PieList` and `ElemPool` together, providing a standard, less verbose API for iteration.
  - `PieViewMut<'a, T>`: A temporary mutable view that provides a convenient API for modifying a list.
  - `Cursor<'a, T>`: A read-only cursor that points to an element in a list, allowing for robust, fine-grained navigation.
  - `CursorMut<'a, T>`: A mutable cursor that allows for complex, fine-grained list mutations like splitting a list or splicing two lists together.
- `FibHeap<K, V>`: A Fibonacci heap (priority queue) built on the same pool, offering an O(1) amortized `decrease_key` operation.

```rust
use pie_core::{ElemPool, PieList};

// 1. Create a shared pool to manage memory for all lists of this type.
let mut pool = ElemPool::<i32>::new();

// 2. Create list handles. They are lightweight and borrow from the pool.
let mut list_a = PieList::new(&mut pool);

// OPTION A: Verbose API (Pass pool to every method)
// Useful when managing complex borrows or distinct lifetimes.
list_a.push_back(10, &mut pool).unwrap();

// OPTION B: Simplified API (Use a PieView)
// Useful for standard operations. The view bundles the list and pool.
let mut view = list_a.view_mut(&mut pool);
view.push_back(20); // No pool argument needed!
view.push_back(30);

assert_eq!(list_a.len(), 3);
assert_eq!(pool.len(), 3); // The pool tracks total items.
```

## **Core Design Philosophy**

The central design of `pie_core` is to move memory allocation away from individual nodes and into a centralized `ElemPool`.

1.  **Arena (Pool) Allocation**: Instead of making a heap allocation for every new element, all elements are stored contiguously in a `Vec` inside the `ElemPool`. This has two major benefits:
  - **Cache Locality**: Elements are closer together in memory, which can lead to fewer cache misses and significantly faster traversal and iteration.
  - **Reduced Allocator Pressure**: It avoids frequent calls to the global allocator, which can be a bottleneck in performance-critical applications.
2.  **Typed Indices**: Nodes are referenced by a type-safe `Index<T>` instead of raw pointers or `usize`. This leverages Rust's type system to prevent indices from one pool being accidentally used with another.
3.  **Multi-Structure, Single-Pool**: A single `ElemPool<T>` can manage the memory for thousands of `PieList<T>` (or `FibHeap`) instances. This is highly efficient for applications that create and destroy many short-lived data structures.
4.  **Views and Cursors for a Safe, Powerful API**: The library provides several ways to interact with lists, each designed for a specific purpose:
  - **Views (`PieView` and `PieViewMut`)**: For convenience, `pie_core` offers view structs. `PieView` provides a simplified, read-only API for iteration, while `PieViewMut` allows for common list modifications. They bundle a list handle and a pool reference, reducing API verbosity for standard operations.
  - **Cursors (`Cursor` and `CursorMut`)**: For advanced operations, the library uses a cursor-based API. A `Cursor` provides read-only navigation, while a `CursorMut` allows for powerful, O(1) structural mutations like splitting a list at a specific point or splicing another list into it. The design correctly models Rust's ownership rules, ensuring that a `CursorMut` exclusively locks its own list from other modifications but leaves the shared pool available for other lists to use.

## **When is `pie_core` a Good Choice?**

This crate is not a general-purpose replacement for `Vec` or `VecDeque`. It excels in specific contexts:

- **Performance-Critical Loops**: In game development, simulations, or real-time systems where you frequently add, remove, or reorder items in the middle of a large collection.
- **Efficient Priority Queues**: When you need a priority queue that supports an efficient O(1) amortized `decrease_key` operation, which is common in graph algorithms like Dijkstra's or Prim's.
- **Managing Many Small Structures**: When you need to manage a large number of independent lists or heaps, sharing a single `ElemPool` is far more memory-efficient than having each structure handle its own allocations.
- **Stable Indices**: The indices used by `pie_core` are stable; unlike a `Vec`, inserting or removing elements does not invalidate the indices of other elements. Only shrinking the pool invalidates some of them.
- Embedded and `no_std` Environments: The crate supports `no_std` (requiring `extern crate alloc`), making it ideal for bare-metal or embedded systems where the standard library is unavailable but dynamic allocation is permitted.

**Important Note on `T` Size**: `pie_core` stores the data `T` (or `K` and `V` for the heap) directly inside the node structure. This design is optimized for smaller types (`Copy` types, numbers, small structs). If the data is very large, the increased size of each element can reduce the cache-locality benefits, as fewer elements will fit into a single cache line.

## **Strengths and Weaknesses**

### **Strengths**

- **Performance**: Excellent cache-friendliness and minimal allocator overhead. Operations like `split_before` and `splice_before` are O(1). `FibHeap::decrease_key` is O(1) amortized.
- **Safety**: The API is designed to prevent common iterator invalidation bugs. The cursor model ensures safe, concurrent manipulation of different lists within the same pool.
- **Flexibility**: The multi-structure, single-pool architecture is powerful for managing complex data relationships.
- **Embeddable**: Can be compiled without the standard library (i.e. `no_std` support via `default-features = false`), suitable for embedded contexts.

### **Weaknesses**

- **API Verbosity**: The design requires passing a mutable reference to the `ElemPool` for every operation. This is necessary for safety but is more verbose than standard library collections. Mitigation: The `PieView` struct is provided to bundle the list and pool together, offering a standard, cleaner API for common use cases.
- **Not a Drop-in Replacement**: Due to its unique API, `pie_core` cannot be used as a direct replacement for standard library collections without refactoring the calling code.
- **Memory Growth**: The pool's capacity only ever grows. Memory is reused via an internal free list, but the underlying `Vec` does not shrink automatically. The user is responsible for shrinking the pool at opportune moments, if necessary.
- **Handle Invalidation on Shrink**: Calling `shrink_to_fit()` on a pool invalidates all existing `Index<T>` and `FibHandle` values. The operation returns a remapping table, and users **must** call `remap()` on all active lists and update any stored handles. Failure to do so leads to stale index errors.
- **Non-RAII Cleanup**: Unlike standard collections, dropping a `PieList` does not deallocate its contents. Users must manually call `clear()` or `drain()` to return memory to the pool. **In debug builds, dropping a non-empty list will panic** to help catch memory leaks during development. This check is disabled in release builds.

## Features

- **`serde`**: Enables serialization and deserialization for `ElemPool`, `PieList`, and `FibHeap` via the Serde framework. To use it, enable the `serde` feature in your `Cargo.toml`.
- **`petgraph`**: Provides helper functions to use `FibHeap` with the `petgraph` crate for graph algorithms.
- **`no_std` support**: `pie_core` is `no_std` compatible by disabling default features, making it suitable for embedded environments.

## **Alternatives**

`pie_core` is specialized. For many use cases, a standard library collection may be a better or simpler choice:

- **`std::collections::Vec`**: The default and usually best choice for a sequence of data.
- **`std::collections::VecDeque`**: Excellent for fast push/pop operations at both ends (queue-like data structures).
- **`std::collections::LinkedList`**: A general-purpose doubly-linked list. It's simpler to use if you don't need the performance characteristics of an arena and are okay with per-node heap allocations.
- **`std::collections::BinaryHeap`**: A simpler, cache-friendlier priority queue. Use it if you only need `push` and `pop` and do not require the efficient `decrease_key` operation provided by `FibHeap`.
- **`indextree` / `slotmap`**: Other crates that explore arena allocation, generational indices, and graph-like data structures.

## Project Structure

This repository contains more than just the library code. Here is a breakdown of the key components:

- **`src/`**: Core library code for `ElemPool`, `PieList`, `FibHeap`, and related components.
- **`benches/`**: Criterion benchmarks for performance testing against other popular crates.
- **`examples/`**:
  - `fibonacci_heap.rs`: A simple example of using `FibHeap`.
  - `text_editor.rs`: A more complex example demonstrating a text editor backend using `PieList`.
  - `dijkstra/`: A complete example of Dijkstra's pathfinding algorithm using `FibHeap` with `petgraph`.
- **`tests/`**: Integration tests, including tests for `serde` and `pathfinding`.
- **`tools/bench-table`**: A small utility to format benchmark results into a markdown table.
- **`.github/workflows/rust.yml`**: Continuous integration setup to run tests and checks on every push.
- **`justfile`**: A convenient way to run common commands like `just test` or `just bench`.

## **Disclosure of AI Collaboration**

This library was developed as a collaborative effort between a human developer and Google's Gemini AI model. This was not a "vibe coding" exercise; the AI functioned more as a junior developer, receiving clear instructions and integrating hand-coded components. A clear vision of the design and goals guided the effort and ensured those objectives were met.

The AI wrote most of the code and performed well when given clear instructions, saving a significant amount of time. However, I can confidently say that it would not have achieved the end result on its own.

This project serves as an example of human-AI partnership, where the AI acts as a highly capable pair programmer, accelerating development while the human provides the high-level direction and quality assurance.

## **AI Assessment**

The result of this collaboration is a professional-grade, feature-complete library for its intended niche. It is efficient, idiomatic, and highly maintainable, with a correctness guarantee backed by a thorough test suite.
