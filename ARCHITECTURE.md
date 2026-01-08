# Architecture of `pie_core`

This document describes the internal architecture and design decisions of the
`pie_core` library. It is intended for contributors and maintainers.

## Overview

`pie_core` is an arena-allocated data structure library providing:

- **`ElemPool<T>`** — A generational arena allocator with embedded linked-list support
- **`PieList<T>`** — A doubly-linked list handle
- **`FibHeap<K, V>`** — A Fibonacci heap (priority queue)
- **`Index<T>`** — Type-safe handles with ABA protection

The key insight is that all data structures share memory from a single `ElemPool`,
enabling efficient memory reuse and cache-friendly access patterns.

```
┌─────────────────────────────────────────────────────────────────┐
│                        ElemPool<T>                              │
│  ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┐  │
│  │Free │ S₁  │ A   │ B   │ S₂  │ X   │ Y   │ Z   │Free │Free │  │
│  │Sent.│     │     │     │     │     │     │     │     │     │  │
│  └──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┴──┬──┘  │
│     │     │     │     │     │     │     │     │     │     │     │
│  [0]│  [1]│  [2]│  [3]│  [4]│  [5]│  [6]│  [7]│  [8]│  [9]│     │
└─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┘

  Free List: [0] ↔ [8] ↔ [9] ↔ [0]  (circular)
  List 1:    [1] ↔ [2] ↔ [3] ↔ [1]  (S₁ is sentinel, A↔B are data)
  List 2:    [4] ↔ [5] ↔ [6] ↔ [7] ↔ [4]  (S₂ is sentinel, X↔Y↔Z are data)
```

## Core Types

### `Index<T>` — Generational Handle

```rust
pub struct Index<T> {
    slot: u32,        // Position in the Vec
    vers: u32,        // Generation + state bits
    _marker: PhantomData<T>,
}
```

**Design Decisions:**

1. **Type Safety via PhantomData**: `Index<Foo>` cannot be used with `ElemPool<Bar>`.
   Compile-time enforcement with zero runtime cost.

2. **Sentinel Value Pattern**: `Index::NONE` uses `slot = u32::MAX` instead of
   `Option<Index>`. This avoids double-wrapping and keeps the type simple.

3. **Generation Encoding**: The `vers` field contains both generation counter
   (30 bits) and element state (2 bits). See "State Machine" below.

4. **8-byte Size**: Compact enough to store in struct fields, pass by value,
   and use as HashMap keys efficiently.

### `Elem<T>` — The Node Structure

```rust
pub struct Elem<T> {
    next: Index<T>,           // 8 bytes
    prev: Index<T>,           // 8 bytes
    vers: u32,                // 4 bytes (generation + state)
    data: MaybeUninit<T>,     // sizeof(T) bytes
}
```

**Why Intrusive Links?**

Each element embeds its own `next`/`prev` pointers. This design:

- Eliminates external link storage overhead
- Enables the free list to reuse the same link fields
- Allows O(1) unlinking without knowing which list owns the element
- Makes serialization straightforward (all state is self-contained)

**Trade-off**: Elements are larger than in generic arenas (extra 16 bytes for links),
but this is offset by not needing separate link storage or wrapper types.

### State Machine

Each element has a 2-bit state encoded in the low bits of `vers`:

```
┌──────────────────────────────────────────────────────────────┐
│  vers: u32                                                   │
│  ┌────────────────────────────────────┬─────────────────────┐│
│  │     Generation (30 bits)           │  State (2 bits)     ││
│  │     0x00000000 - 0x3FFFFFFF        │  00=Free 01=Used    ││
│  │                                    │  10=Sentinel 11=Zomb││
│  └────────────────────────────────────┴─────────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

**States:**

| State    | Value | Description                                        |
|----------|-------|----------------------------------------------------|
| FREE     | 0b00  | On free list, no data, links point to free list    |
| USED     | 0b01  | Contains user data, linked in a PieList            |
| SENTINEL | 0b10  | List sentinel node, no data, self-referential links|
| ZOMBIE   | 0b11  | Data removed but not yet returned to free list     |

**State Transitions:**

```
           ┌─────────────────────────────────────┐
           │                                     │
           ▼                                     │
        ┌──────┐    index_new()     ┌────────┐   │
        │ FREE │ ─────────────────► │ ZOMBIE │   │
        └──────┘                    └────────┘   │
           ▲                            │        │
           │                            │        │
           │ index_del()                │ data_swap(Some(T))
           │                            ▼        │
           │                       ┌────────┐    │
           └────────────────────── │  USED  │ ───┘
                                   └────────┘    data_swap(None)
                                        │        makes it ZOMBIE
                                        │
                 index_make_sentinel()  │ (from ZOMBIE)
                                        ▼
                                   ┌──────────┐
                                   │ SENTINEL │
                                   └──────────┘
```

**Why ZOMBIE?**

The Zombie state enables two-phase deletion:
1. `data_swap(None)` — Take data out (element becomes Zombie)
2. `index_del()` — Return element to free list (Zombie → Free)

This separation is crucial for:
- FibHeap's `pop()` which moves nodes between lists before freeing
- Safe iteration during `drain()` operations
- Maintaining link integrity during complex operations

### `ElemPool<T>` — The Arena

```rust
pub struct ElemPool<T> {
    elems: Vec<Elem<T>>,  // Contiguous storage
    freed: usize,          // Count of free elements
    used: usize,           // Count of data-holding elements
}
```

**Slot 0 is Reserved:**

The element at index 0 is always the **free list sentinel**. It:
- Never holds user data
- Its `next`/`prev` link the circular free list
- Has a fixed, special version that doesn't follow normal generation rules

**Allocation Strategy:**

```rust
fn index_new() -> Index<T> {
    if free_list_not_empty {
        // Reuse: O(1) - unlink from free list, bump generation
        pop_from_free_list()
    } else {
        // Grow: O(1) amortized - push new element to Vec
        elems.push(new_element)
    }
}
```

**Generation Counter (ABA Protection):**

When an element is freed and reused, its generation is incremented. Old handles
become "stale" — they have the right slot but wrong generation, so `get()` fails:

```rust
let handle = list.push_back(42, &mut pool);  // handle = 5@3 (slot 5, gen 3)
pool.data_swap(handle, None);
pool.index_del(handle);                       // element 5 becomes gen 4

// Later, slot 5 is reused for new data
let handle2 = list.push_back(99, &mut pool); // handle2 = 5@5 (slot 5, gen 5)

pool.data(handle);   // Returns None - stale handle (gen 3 ≠ gen 5)
pool.data(handle2);  // Returns Some(&99) - valid handle
```

### `PieList<T>` — List Handle

```rust
pub struct PieList<T> {
    sentinel: Index<T>,  // Handle to this list's sentinel element
    len: usize,          // Cached length (avoids traversal)
}
```

**Sentinel Design:**

Each list has a dedicated sentinel node. The sentinel:
- Has state = SENTINEL (no data)
- Points to itself when the list is empty
- Its `next` points to the head, `prev` points to the tail
- Simplifies edge cases (empty list, single element)

```
Empty List:        List with [A, B, C]:

  ┌───┐               ┌───┐
  │ S │ ◄──────────►  │ S │
  └───┘               └─┬─┘
                        │
          ┌─────────────┼─────────────┐
          ▼             │             ▼
        ┌───┐         ┌───┐         ┌───┐
        │ A │ ◄─────► │ B │ ◄─────► │ C │
        └───┘         └───┘         └───┘
          ▲                           │
          └───────────────────────────┘
```

**Memory Leak Warning:**

When a `PieList` is dropped, its elements are NOT automatically freed. This is
intentional — the pool doesn't know which list owns which elements. Users must
call `clear()` or `drain()` before dropping.

In debug builds, a panic occurs if a non-empty list is dropped (leak detection).

### `FibHeap<K, V>` — Fibonacci Heap

```rust
pub struct FibHeap<K, V> {
    pool: ElemPool<Node<K, V>>,  // Owns its own pool
    roots: PieList<Node<K, V>>,  // List of root trees
    min: FibHandle<K, V>,         // Handle to minimum node
    len: usize,
}

struct Node<K, V> {
    key: K,
    value: V,
    parent: FibHandle<K, V>,      // NONE if root
    children: PieList<Node<K, V>>,// Child nodes form a list
    degree: usize,
    marked: bool,
}
```

**Complexity:**

| Operation      | Amortized | Worst-case |
|----------------|-----------|------------|
| push           | O(1)      | O(1)       |
| peek           | O(1)      | O(1)       |
| pop            | O(log n)  | O(n)       |
| decrease_key   | O(1)      | O(log n)   |

**Integration with PieList:**

The heap uses `PieList` for:
- The root list (forest of heap-ordered trees)
- Each node's children list

This shared infrastructure means all heap nodes live in the same `ElemPool`,
enabling efficient memory reuse across heap operations.

## Key Design Patterns

### 1. Pool-Centric API

All operations require passing the pool explicitly:

```rust
// NOT: list.push_back(42)
list.push_back(42, &mut pool)
```

**Rationale:**
- Multiple lists can share one pool
- Clear ownership semantics
- No hidden global state or interior mutability

### 2. Handle-Based References

Instead of references (`&T`), the library uses handles (`Index<T>`):

```rust
let handle = list.push_back(42, &mut pool);  // Returns handle, not reference
let value = pool.data(handle);                // Get reference when needed
```

**Rationale:**
- Handles are `Copy` — can be stored, passed, compared cheaply
- No lifetime parameters propagating through user code
- Safe concurrent access patterns possible (with appropriate synchronization)

### 3. Two-Phase Deletion

Deletion is split into data removal and element recycling:

```rust
// Phase 1: Take data (element becomes Zombie)
let data = pool.data_swap(handle, None);

// Phase 2: Return to free list (Zombie → Free)
pool.index_del(handle);
```

This allows operations like `pop()` to:
1. Unlink the element from its list
2. Take its data
3. Perform additional bookkeeping
4. Finally return the element to the free list

### 4. Shrink with Remapping

`shrink_to_fit()` compacts the pool but invalidates handles. It returns a
remapping table so data structures can update their handles:

```rust
let remap = pool.shrink_to_fit();
list.remap(&remap);  // Update sentinel handle
// User must also remap any handles they've stored
```

## Module Organization

```
src/
├── lib.rs          # Public API exports, feature flags
├── index.rs        # Index<T> type and operations
├── elem.rs         # Elem<T> node structure, state machine
├── pool.rs         # ElemPool<T> arena allocator
├── list.rs         # PieList<T> and iterators
├── heap.rs         # FibHeap<K, V> implementation
├── cursor.rs       # Immutable cursor for PieList
├── cursor_mut.rs   # Mutable cursor with split/splice
├── view.rs         # PieView<T> for trait impls (Debug, PartialEq)
├── view_mut.rs     # PieViewMut<T> for ergonomic mutations
└── petgraph.rs     # Optional petgraph integration (Dijkstra)
```

## Unsafe Code

The library uses `#![deny(unsafe_code)]` at the crate level, but allows it
locally with `#[allow(unsafe_code)]` for specific operations:

| Location | Reason |
|----------|--------|
| `Elem::data` access | `MaybeUninit<T>` requires unsafe to read initialized data |
| Iterator `next()` | Pointer-based traversal for performance |
| `ElemPool::Drop` | Must drop initialized data in USED elements |

All unsafe blocks are:
- Preceded by state checks (e.g., `is_used()`)
- Documented with `// SAFETY:` comments
- Minimal in scope

## Performance Characteristics

### Memory Layout

```
ElemPool<u64>:
┌────────────────────────────────────────────────────┐
│ Vec<Elem<u64>> (contiguous, cache-friendly)        │
├────────────────────────────────────────────────────┤
│ Elem<u64>: next(8) + prev(8) + vers(4) + data(8)   │
│            = 28 bytes, padded to 32 bytes          │
└────────────────────────────────────────────────────┘
```

### Operation Costs

| Operation                | Time       | Allocations                |
|--------------------------|------------|----------------------------|
| `push_back`/`push_front` | O(1)       | 0 (if free list non-empty) |
| `pop_back`/`pop_front`   | O(1)       | 0                          |
| `iter()` traversal       | O(n)       | 0                          |
| `sort()`                 | O(n log n) | O(log n) sentinels         |
| `shrink_to_fit()`        | O(freed)   | O(freed) for remap         |

### Cache Behavior

- Sequential iteration: Good locality (elements in same Vec)
- Random access by handle: Single indirection (Vec index)
- Insertion order ≠ memory order: Reuse can fragment logical order

## Testing Strategy

1. **Unit Tests**: Each module has `#[cfg(test)]` tests
2. **Integration Tests**: `tests/` directory for cross-module scenarios
3. **Doc Tests**: Examples in documentation are tested
4. **Stress Tests**: Randomized operations in pool and heap tests
5. **Debug Assertions**: Leak detection, state invariants

## Feature Flags

| Feature         | Effect                                          |
|-----------------|-------------------------------------------------|
| `std` (default) | Enables `HashMap` for IndexMap, std Error trait |
| `serde`         | Serialization/deserialization support           |
| `petgraph`      | Dijkstra's algorithm using FibHeap              |
