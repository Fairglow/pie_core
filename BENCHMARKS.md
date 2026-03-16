# Benchmark Guide for pie_core

This document explains the benchmark suite, how to run it, and how to interpret the results to make informed decisions about using `pie_core`.

## Quick Start

```bash
# Run all benchmarks
cargo bench

# Run benchmarks with petgraph Dijkstra comparison
cargo bench --features petgraph

# Run nightly-only benchmarks (std::collections::LinkedList)
cargo +nightly bench --features bench-nightly

# Generate comparison table
cargo run -p bench-table

# Generate markdown table for documentation
cargo run -p bench-table -- --markdown
```

## Benchmark Categories

### List Operations (`list/`)

These benchmarks compare `PieList` against standard library collections and other arena-based lists.

| Benchmark | What it measures | Who should win | Real-world scenario |
|-----------|------------------|----------------|---------------------|
| `append` | Push N elements to back | Vec (no link overhead) | Building a collection from a stream of events or records |
| `prepend` | Push N elements to front | PieList/VecDeque (O(1) vs Vec's O(n)) | Undo stack, MRU cache, prepending log entries |
| `iterate` | Sum all elements | Vec (best cache locality) | Aggregation, rendering a list, serialization |
| `mid_modify` | Single insert+remove at middle | IndexList/PieList (O(1) vs Vec's O(n)) | Text editor: insert/delete a character at the cursor |
| `multi_insert` | Insert 100 elements at random positions | **PieList** (O(n) total vs Vec's O(n²)) | Collaborative editing: applying a batch of remote insertions |
| `splice` | Merge two lists at middle (includes seek) | PieList (O(1) splice, but O(n) seek) | Paste clipboard content into the middle of a document |
| `splice_front` | Merge two lists at front (no seek) | **PieList** (O(1) vs Vec's O(n) shift) | Prepending a header/preamble to an existing list |
| `sort` | In-place sorting | Vec (pdqsort is highly optimized) | Sorting a dataset for display or binary search |
| `random_access` | 100 random index lookups | Vec (O(1) vs linked list O(n)) | Jump-to-line in a text editor, index-based lookups |
| `drain` | Consume all elements via draining iterator | Vec (contiguous memory) | Teardown: processing all items then discarding the list |
| `cursor_traverse` | Forward scan via cursor with peek | PieList iter (no cursor overhead) | Cursor-based scan: find-next, search within a list |
| `split` | Split a list at the midpoint | PieList (O(1) split after O(n/2) seek) | Splitting a document into two halves for parallel processing |
| `pop_front` | Pop all from front (FIFO drain) | PieList/VecDeque (O(1) vs Vec O(n)) | Queue pattern: processing items in arrival order |
| `pop_back` | Pop all from back (LIFO drain) | All similar (all O(1) per pop) | Stack pattern: undo history, backtracking |
| `retain` | Filter ~50% in place by predicate | Vec/VecDeque (cache locality) | Removing dead entities, filtering stale cache entries |

### Pool Operations (`pool/`)

| Benchmark | What it measures | Notes | Real-world scenario |
|-----------|------------------|-------|---------------------|
| `shared_lists` | Create N lists, fill with M elements, clear | Shows pool allocation patterns | ECS/game engine: many entity-component lists sharing one allocator |
| `shrink_to_fit` | Compact pool after 50% deletion | Measures defragmentation cost | Long-running server: periodic memory reclamation after churn |

### Heap Operations (`heap/`)

These benchmarks compare `FibHeap` against other priority queue implementations.

| Benchmark | What it measures | Who should win | Real-world scenario |
|-----------|------------------|----------------|---------------------|
| `push` | Bulk insertion | All similar (O(1) amortized for Fib, O(log n) for Binary) | Job scheduler: enqueuing a burst of tasks |
| `pop` | Pop all elements in priority order | BinaryHeap (simpler structure, better cache locality) | Event loop: draining all pending events |
| `decrease_key` | Update priority of existing elements | **PieFibHeap** (O(1) amortized) | Dijkstra/Prim/A*: relaxing edge weights |
| `push_pop` | Push N then pop N (combined lifecycle) | BinaryHeap (consolidation overhead for Fib) | Task queue: batch enqueue then drain to completion |
| `peek` | Access minimum without removal | All O(1) | Event loop: check next deadline without dequeuing |
| `drain` | Pop all elements via drain iterator | BinaryHeap (simpler pop) | Shutdown: process remaining items in priority order |
| `mixed_workload` | Interleaved push/decrease_key/pop | *PieFibHeap* (O(1) decrease_key) | Dijkstra-like access pattern without graph overhead |

### Algorithm Benchmarks (`algo/`)

Real-world algorithm comparisons showing practical impact. These are the most important benchmarks because they measure end-to-end performance in realistic scenarios where the data structure choice actually matters.

| Benchmark | What it measures | Notes | Real-world scenario |
|-----------|------------------|-------|---------------------|
| `dijkstra_dense` | Dense graph (n=100 and n=1000) | Tests decrease_key impact with many relaxations | Routing in a highly-connected network (mesh WiFi, datacenter) |
| `dijkstra_sparse` | Sparse graph (n=10k, m=20k) | Grid-like structure, fewer relaxations | Pathfinding on a map grid (game AI, robot navigation) |

## Benchmark Sizes

Each benchmark is run at multiple sizes to show asymptotic behavior:

- **100 elements**: Overhead-dominated. Shows constant factors.
- **1,000 elements**: Balanced. Typical use case.
- **10,000 elements**: Algorithm-dominated. Shows O(n) complexity effects.
- **100,000 elements**: Large scale tests for splice/multi-insert benchmarks.

## Key Benchmark Results

**Disclaimer**: These benchmarks are synthetic and run on a specific system. They serve as an indication of probability, not a guarantee. Real-world performance varies significantly by hardware and workload. Always verify with your own profiling.

These results show relative performance (lower is better, 1.00x is fastest):

### Where PieList Wins

| Benchmark | PieList | Vec | Notes |
|-----------|---------|-----|-------|
| `multi_insert` (100k) | 1.00x | **426x** | 100 inserts at random positions |
| `splice_front` (100k) | 1.00x | **1528x** | Merge at front, no traversal |
| `prepend` (1k) | 1.00x | **7.6x** | Push to front repeatedly |

**Note on `prepend` and Vec**: The `prepend` benchmark excludes Vec for N > 1,000 because Vec's O(n) per insertion makes repeated front-insertion O(n²) total. At N=10,000 this would take ~100x longer than at N=1,000, dominating the benchmark run time without providing useful signal. VecDeque and PieList both handle front-insertion in O(1).

### Where Vec Wins

| Benchmark | Vec | PieList | Notes |
|-----------|-----|---------|-------|
| `iterate` (10k) | 1.00x | **22x** | Sum all elements |
| `random_access` (1k) | 1.00x | **2450x** | 100 random lookups |
| `sort` (10k) | 1.00x | **10.2x** | In-place sorting |
| `append` (10k) | 1.00x | **2.6x** | Push to back |

### Heap Results

| Benchmark | BinaryHeap | FibHeap | PriorityQueue | Notes |
|-----------|------------|---------|---------------|-------|
| `push` (1k) | 1.00x | 6.4x | 12.0x | Bulk insertion |
| `pop` (1k) | 1.00x | **10.8x** | 2.4x | Extract all elements |
| `decrease_key` (1k) | N/A (lazy push) | **1.00x** | 2.0x | FibHeap's key advantage |
| `push_pop` (1k) | 1.00x | 6.5x | 3.2x | Full lifecycle: enqueue then drain |
| `peek` | ~same | ~same | ~same | All O(1) |
| `drain` (1k) | 1.00x | **14.1x** | — | Pop all via drain iterator |
| `mixed_workload` (1k) | 1.00x (lazy) | 5.5x | 2.5x | Interleaved push/decrease_key/pop |
| `dijkstra_dense` (100) | 1.00x | 1.20x | — | Dense graph (n=100, m=5k) |
| `dijkstra_dense` (1k) | 1.00x | 1.11x | — | Dense graph (n=1k, m=50k) |
| `dijkstra_sparse` | 1.00x | **3.8x** | — | Sparse graph (n=10k, m=20k) |

**Note on `decrease_key` fairness**: The BinaryHeap "lazy push" simulation only measures the cost of pushing a duplicate entry. It does *not* measure the extra pop-side cost from discarding stale duplicates. In a real Dijkstra implementation, the lazy approach adds overhead at pop time that this isolated benchmark does not capture. The algorithm-level Dijkstra benchmarks provide a more honest comparison.

## Implementations Compared

### List Comparisons

| Implementation | Type | Notes |
|----------------|------|-------|
| `pielist` | `pie_core::PieList` | Arena-allocated linked list |
| `vec` | `std::vec::Vec` | Contiguous array |
| `vecdeque` | `std::collections::VecDeque` | Ring buffer |
| `indexlist` | `index_list::IndexList` | Another arena-based list |
| `linkedlist` | `std::collections::LinkedList` | Nightly only (requires CursorMut) |

### Heap Comparisons

| Implementation | Type | Notes |
|----------------|------|-------|
| `piefibheap` | `pie_core::FibHeap` | Fibonacci heap with decrease_key |
| `binaryheap` | `std::collections::BinaryHeap` | Binary heap, no decrease_key |
| `priorityqueue` | `priority_queue::PriorityQueue` | Supports change_priority |
| `extfibheap` | `fibonacci_heap::FibonacciHeap` | Another Fibonacci heap impl (no drain, uses Rc/RefCell) |

## Interpreting Results

### Using bench-table

The `bench-table` tool displays Criterion benchmark results in easy-to-read tables:

```bash
# Default: compact view (just relative times)
cargo run -p bench-table

# Show absolute times alongside relative
cargo run -p bench-table -- --full

# One narrow table per operation (best for terminals)
cargo run -p bench-table -- --split

# Filter to specific implementations
cargo run -p bench-table -- --impl pielist,vec,binaryheap

# Filter to specific category
cargo run -p bench-table -- --category heap

# Implementations as rows (transpose)
cargo run -p bench-table -- --vertical

# Generate markdown for documentation
cargo run -p bench-table -- --markdown

# Combine options
cargo run -p bench-table -- --split --full --category list

# Show inline ASCII bar charts
cargo run -p bench-table -- --bars

# Quick one-line-per-category summary
cargo run -p bench-table -- --summary

# Regression comparison (requires both base and new benchmarks)
cargo bench -- --save-baseline base
# ... make changes ...
cargo bench -- --save-baseline new
cargo run -p bench-table -- --base --new
```

### Understanding the Tables

Example compact table:

```
| Size | pielist | vec   | vecdeque | indexlist |
|------|---------|-------|----------|-----------|
| 100  | 1.42x   | 1.00x | 1.15x    | 2.13x     |
| 1k   | 1.38x   | 1.00x | 1.11x    | 2.32x     |
| 10k  | 1.41x   | 1.00x | 1.11x    | 2.66x     |
```

Example split table (one per operation):

```
  append
+-----------+-------+
| Impl      | Rel   |
+=====================+
| vec       | 1.00x |
| pielist   | 1.42x |
| vecdeque  | 1.15x |
+-----------+-------+
```

- **Green** (in terminal): fastest or within 5%
- **Red** (in terminal): 2x or more slower
- `1.00x` is the winner

### What the Numbers Tell You

1. **Consistent ratios across sizes** → Overhead-based difference (constant factor)
2. **Growing ratios with size** → Algorithmic complexity difference (O(n) vs O(1))
3. **Shrinking ratios with size** → Amortization or cache effects

## When to Use pie_core

Based on actual benchmark results (N=10,000 elements unless noted):

### Use PieList When...

✅ **Repeated middle insertions**: Inserting 100 elements at random positions—PieList is **426x faster** than Vec (grows with list size)
```rust
// PieList: O(n) to find + O(1) to insert, repeated = O(n * k)
// Vec: O(n) shift per insert, repeated = O(n * k) but with actual memory moves
```

✅ **Splicing at known positions**: When cursor is already positioned, splice is O(1)
```rust
cursor.splice_before(&mut other_list, &mut pool); // O(1)
```

✅ **Front operations**: Push/splice at front—PieList is **8-1530x faster** than Vec

✅ **Need stable indices**: Indices survive insertions/removals

✅ **Managing many small lists**: One pool serves many lists efficiently

### Use Vec Instead When...

❌ **Random access by index**: Vec is **2450x faster** (O(1) vs O(n))

❌ **Sequential iteration**: Vec is **22x faster** (perfect cache locality)

❌ **Sorting**: Vec is **10.2x faster** (pdqsort is heavily optimized)

❌ **Simple push/pop at ends only**: Vec has **2.6x less overhead**

### Use FibHeap When...

✅ **Decrease-key is critical**: Dijkstra, Prim, A* algorithms

✅ **Dense graphs**: FibHeap is within 11% of BinaryHeap on dense graphs (n=1k), and the gap narrows with size

### Use BinaryHeap Instead When...

❌ **No decrease-key needed**: Simple task scheduling, event queues

❌ **Maximum pop throughput**: BinaryHeap is **8.5x faster** for extraction

❌ **Sparse graphs**: BinaryHeap is **3.8x faster** on sparse graph Dijkstra

## Fairness Notes

### What Makes a Fair Comparison

1. **Same workload**: All implementations do the same logical work
2. **Same data types**: All use `u64` for consistency
3. **Batched setup**: Setup is not measured, only the operation
4. **Randomized data**: Prevents favorable orderings
5. **Multiple sizes**: Shows both constant factors and asymptotic behavior

### Known Limitations

1. **PieList seek time**: Finding the middle is O(n) for all linked lists. The benchmark includes seek time to be fair, as real code must also seek.

2. **BinaryHeap decrease_key simulation**: BinaryHeap doesn't support decrease_key, so we show the "lazy push" workaround used in practice.

3. **Memory usage not measured**: Benchmarks measure time only. PieList may use more memory due to link overhead.

4. **Cache effects vary**: Results depend on CPU cache sizes and memory subsystem.

## Running Subset of Benchmarks

```bash
# Run only list benchmarks
cargo bench -- list/

# Run only heap benchmarks
cargo bench -- heap/

# Run only specific operation
cargo bench -- list/append

# Run only specific implementation
cargo bench -- pielist
```

## Memory Usage Estimates

These are computed from struct layouts (not runtime-measured). Actual memory depends on alignment and allocator overhead.

### PieList per-element overhead

Each element in a PieList is stored as an `Elem<T>` in the `ElemPool`:

| Field | Size | Purpose |
|-------|------|---------|
| `next: Slot` | 4 bytes | Next-link (u32 index) |
| `prev: Slot` | 4 bytes | Prev-link (u32 index) |
| `vers: Generation` | 4 bytes | Generation counter + state (ABA protection) |
| `data: MaybeUninit<T>` | size_of::\<T\>() | User data |
| **Total per element** | **12 + size_of::\<T\>()** | (plus alignment padding) |

For `T = u64` (8 bytes): **20 bytes per element** vs Vec's 8 bytes per element (**2.5x overhead**).

Additionally, each PieList has a sentinel node (same size as a data element) and a list header:

| Field | Size | Purpose |
|-------|------|---------|
| `sentinel: Index<T>` | 8 bytes | Handle to sentinel node |
| `len: usize` | 8 bytes | Element count |

### FibHeap per-entry overhead

Each FibHeap entry is a `Node<K, V>` stored in the pool:

| Field | Size | Purpose |
|-------|------|---------|
| `key: K` | size_of::\<K\>() | Priority key |
| `value: V` | size_of::\<V\>() | User value |
| `parent: FibHandle` | 8 bytes | Parent handle |
| `children: PieList<Node>` | 16 bytes | Child list header |
| `degree: usize` | 8 bytes | Number of children |
| `marked: bool` | 1 byte (+7 padding) | Cut marker |
| **Node subtotal** | **40 + K + V** | (with typical alignment) |
| Elem wrapper | +12 bytes | next/prev/generation |
| **Total per entry** | **~52 + K + V** | |

For `K = usize, V = usize` (16 bytes): **~68 bytes per entry** vs BinaryHeap's ~16 bytes (**~4.3x overhead**). This overhead is the price for O(1) decrease_key.

### Comparison summary

| Implementation | Per-element (T=u64) | Notes |
|----------------|---------------------|-------|
| `Vec<T>` | 8 bytes | Contiguous, no overhead |
| `VecDeque<T>` | 8 bytes | Ring buffer, same density |
| `PieList<T>` | ~20 bytes | +12 bytes for links + generation |
| `BinaryHeap<T>` | 8 bytes | Contiguous array |
| `FibHeap<K,V>` | ~68 bytes (K=V=usize) | Rich node structure for O(1) decrease_key |

## Profiling Tips

For deeper analysis:

```bash
# Generate flamegraph (requires cargo-flamegraph)
cargo flamegraph --bench benches -- --bench

# CPU profiling with perf
perf record cargo bench -- list/mid_modify/pielist
perf report

# Memory profiling with Valgrind
valgrind --tool=massif cargo bench -- list/append/pielist
```

## Contributing Benchmarks

When adding new benchmarks:

1. **Use consistent naming**: `category/operation` group, implementation as `BenchmarkId`
2. **Test multiple sizes**: Use `SIZES` constant
3. **Include competitors**: At least Vec/BinaryHeap as baseline
4. **Document expected winner**: In comments, explain who should win and why
5. **Use `black_box`**: Prevent compiler from optimizing away results
6. **Document the real-world scenario**: Each benchmark must justify its existence with a concrete use case

## Benchmark Coverage Analysis

### What the Current Suite Proves

1. **Structural mutations are PieList's strength**: The `multi_insert`, `splice_front`, and `prepend` benchmarks demonstrate orders-of-magnitude advantages for list mutations. The growing ratios with size confirm this is an algorithmic (O(1) vs O(n)) advantage, not just a constant-factor win.

2. **Sequential access is Vec's strength**: The `iterate`, `random_access`, and `sort` benchmarks show that cache locality dominates for read-heavy workloads. These ratios are consistent across sizes, confirming it's a constant-factor (cache) difference.

3. **FibHeap's decrease_key is genuine O(1)**: The `decrease_key` benchmark shows a clear advantage over PriorityQueue's O(log n) `change_priority`. BinaryHeap cannot even perform this operation natively.

4. **End-to-end algorithm impact varies**: The Dijkstra benchmarks show that FibHeap's theoretical advantage does not always translate to a practical win. On sparse graphs, BinaryHeap's simplicity and cache locality compensate for its worse asymptotic `decrease_key`. On dense graphs (many relaxations), the gap narrows.

5. **Pool sharing has modest benefits**: The `shared_lists` benchmark shows the pool model eliminates per-list allocation overhead, but the advantage is not dramatic for typical sizes.

### What the Current Suite Cannot Prove

These are gaps where the benchmark suite provides no data:

1. **Memory efficiency**: No benchmark measures memory consumption. PieList stores two `Index<T>` pointers per node plus a generation counter, which may be significant for large element counts or constrained environments.

2. **Mixed read-write workloads**: Every benchmark tests a single operation in isolation. Real workloads interleave reads, writes, and structural mutations. The iteration penalty may or may not matter when iteration is 5% of the workload.

3. **FIFO/LIFO queue patterns**: `pop_front`/`pop_back` are not benchmarked independently. These are critical for queue and stack use cases where VecDeque is the natural competitor.

4. **Retain/filter patterns**: `retain()` is not benchmarked. In-place filtering of a list is a common operation (e.g., removing dead entities from a game world).

5. **ExtFibHeap consistency**: The external Fibonacci heap crate (`fibonacci_heap`) appears in `push` and `pop` benchmarks but is missing from `push_pop`, `decrease_key`, `peek`, and `drain`. This makes cross-implementation comparison incomplete.

6. **Larger Dijkstra graphs**: The dense graph is only n=100, which is tiny. The sparse graph is 10k nodes but only has rightward and downward edges (no backtracking). More realistic graph topologies and sizes would strengthen the algorithm comparison.

### How to Interpret Ratio Trends

| Pattern | Meaning | Example |
|---------|---------|---------|
| Ratio is constant across sizes | Constant-factor difference (cache, overhead) | `iterate`: Vec ~25x faster at all sizes |
| Ratio grows with size | Algorithmic complexity difference | `prepend`: Vec's O(n²) vs PieList's O(n) |
| Ratio shrinks with size | Amortization kicking in | `push`: FibHeap overhead diluted at large N |
| Ratio near 1.0x | Implementations are equivalent for this workload | `peek`: all heaps O(1) |

## Terminal Visualization

The `bench-table` tool provides colored terminal tables for quick comparison. Current features:

- **Green** cells: Fastest or within 5% of fastest
- **Red** cells: 2x or more slower than fastest
- Multiple layout modes: `--split` (one table per operation), `--vertical`, `--combined`
- Filtering: `--impl` and `--category` for focused comparisons

### Recommended Workflows

```bash
# Quick overview: "Where does PieList win?"
cargo run -p bench-table -- --split --impl pielist,vec

# Heap comparison: "Is FibHeap worth it?"
cargo run -p bench-table -- --category heap --full

# Focus on the operations PieList is designed for
cargo run -p bench-table -- --split --impl pielist,vec,vecdeque --category list

# Generate tables for documentation
cargo run -p bench-table -- --markdown --full
```

### Future Visualization Improvements

The current tool shows relative performance ratios in tables. For a more intuitive "at a glance" view, planned improvements include:

- **Inline bar charts**: Horizontal ASCII bars proportional to relative time within each table cell, so the magnitude of differences is visual rather than numeric
- **Summary dashboard**: A single-screen overview showing the headline win/loss for each category
- **Regression markers**: Indicators when a new benchmark run shows regression vs. a saved baseline
