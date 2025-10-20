[![Rust](https://github.com/Fairglow/pie_core/actions/workflows/rust.yml/badge.svg)](https://github.com/Fairglow/pie_core/actions/workflows/rust.yml)

# **pie_core: High-Performance, Arena-Allocated Data Structures**

`pie_core` is a Rust crate that provides high-performance, arena-allocated data structure implementations, including a doubly-linked list and a Fibonacci heap (priority queue). It is built on an arena allocation (or "pool") model, which offers significant performance benefits over traditional node-based structures in specific scenarios.

This crate provides three main types:

* `ElemPool<T>`: An arena allocator that owns and manages the memory for all elements of a specific type.
* `PieList<T>`: A lightweight handle representing a single linked list whose elements are stored in a shared `ElemPool`.
* `FibHeap<K, V>`: A Fibonacci heap (priority queue) built on the same pool, offering an O(1) amortized `decrease_key` operation.

```rust
use pie_core::{ElemPool, PieList};

// 1. Create a shared pool to manage memory for all lists of this type.
let mut pool = ElemPool::<i32>::new();

// 2. Create list handles. They are lightweight and borrow from the pool.
let mut list_a = PieList::new(&mut pool);
list_a.push_back(10, &mut pool).unwrap();
list_a.push_back(20, &mut pool).unwrap();

let mut list_b = PieList::new(&mut pool);
list_b.push_back(100, &mut pool).unwrap();

assert_eq!(list_a.len(), 2);
assert_eq!(list_b.len(), 1);
assert_eq!(pool.len(), 3); // The pool tracks total items.
```

## **Core Design Philosophy**

The central design of `pie_core` is to move memory allocation away from individual nodes and into a centralized `ElemPool`.

1.  **Arena (Pool) Allocation**: Instead of making a heap allocation for every new element, all elements are stored contiguously in a `Vec` inside the `ElemPool`. This has two major benefits:
      * **Cache Locality**: Elements are closer together in memory, which can lead to fewer cache misses and significantly faster traversal and iteration.
      * **Reduced Allocator Pressure**: It avoids frequent calls to the global allocator, which can be a bottleneck in performance-critical applications.
2.  **Typed Indices**: Nodes are referenced by a type-safe `Index<T>` instead of raw pointers or `usize`. This leverages Rust's type system to prevent indices from one pool being accidentally used with another.
3.  **Multi-Structure, Single-Pool**: A single `ElemPool<T>` can manage the memory for thousands of `PieList<T>` (or `FibHeap`) instances. This is highly efficient for applications that create and destroy many short-lived data structures.
4.  **Safe, Powerful API**: The library provides a `CursorMut` API for complex, fine-grained list mutations like splitting or splicing. The design correctly models Rust's ownership rules, ensuring that a `CursorMut` locks its own list from modification but leaves the shared pool available for other lists to use.

## **When is `pie_core` a Good Choice?**

This crate is not a general-purpose replacement for `Vec` or `VecDeque`. It excels in specific contexts:

  * **Performance-Critical Loops**: In game development, simulations, or real-time systems where you frequently add, remove, or reorder items in the middle of a large collection.
  * **Efficient Priority Queues**: When you need a priority queue that supports an efficient O(1) amortized `decrease_key` operation, which is common in graph algorithms like Dijkstra's or Prim's.
  * **Managing Many Small Structures**: When you need to manage a large number of independent lists or heaps, sharing a single `ElemPool` is far more memory-efficient than having each structure handle its own allocations.
  * **Stable Indices Required**: The indices used by `pie_core` are stable; unlike a `Vec`, inserting or removing elements does not invalidate the indices of other elements.

**Important Note on `T` Size**: `pie_core` stores the data `T` (or `K` and `V` for the heap) directly inside the node structure. This design is optimized for smaller types (`Copy` types, numbers, small structs). If the data is very large, the increased size of each element can reduce the cache-locality benefits, as fewer elements will fit into a single cache line.

## **Strengths and Weaknesses**

### **Strengths**

  * **Performance**: Excellent cache-friendliness and minimal allocator overhead. Operations like `split_before` and `splice_before` are O(1). `FibHeap::decrease_key` is O(1) amortized.
  * **Safety**: The API is designed to prevent common iterator invalidation bugs. The cursor model ensures safe, concurrent manipulation of different lists within the same pool.
  * **Flexibility**: The multi-structure, single-pool architecture is powerful for managing complex data relationships.

### **Weaknesses**

  * **API Verbosity**: The design requires passing a mutable reference to the `ElemPool` for every operation. This is necessary for safety but is more verbose than standard library collections.
  * **Not a Drop-in Replacement**: Due to its unique API, `pie_core` cannot be used as a direct replacement for standard library collections without refactoring the calling code.
  * **Memory Growth**: The pool's capacity only ever grows. Memory is reused via an internal free list, but the underlying `Vec` does not shrink.

## **Alternatives**

`pie_core` is specialized. For many use cases, a standard library collection may be a better or simpler choice:

  * **`std::collections::Vec`**: The default and usually best choice for a sequence of data.
  * **`std::collections::VecDeque`**: Excellent for fast push/pop operations at both ends (queue-like data structures).
  * **`std::collections::LinkedList`**: A general-purpose doubly-linked list. It's simpler to use if you don't need the performance characteristics of an arena and are okay with per-node heap allocations.
  * **`std::collections::BinaryHeap`**: A simpler, cache-friendlier priority queue. Use it if you only need `push` and `pop` and do not require the efficient `decrease_key` operation provided by `FibHeap`.
  * **`indextree` / `slotmap`**: Other crates that explore arena allocation, generational indices, and graph-like data structures.

## **Disclosure of AI Collaboration**

This library was developed as a collaborative effort between a human developer and Google's Gemini AI model.

  * **The AI's Role**: The AI was instrumental in writing the foundational code, implementing core features (like `split_before`, `splice_before`, and the `FibHeap` consolidation logic), generating the comprehensive test suite for each module, and refactoring the code to improve its design and fix bugs. It also provided multiple in-depth code reviews, identifying issues related to performance, idiomatic Rust, and correctness.
  * **The Human's Role**: The human developer guided the overall architecture and design, set the goals, made final decisions on the public API, and performed critical reviews, and fixes, of all AI-generated code to ensure it met the project's standards for safety, performance, and maintainability.

This project serves as an example of human-AI partnership, where the AI acts as a highly capable pair programmer, accelerating development while the human provides the high-level direction and quality assurance.

## **Assessment**

The result of this collaboration is a professional-grade, feature-complete library for its intended niche. It is efficient, idiomatic, and highly maintainable, with a correctness guarantee backed by a thorough test suite.
