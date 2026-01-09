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

| Benchmark | What it measures | Who should win |
|-----------|------------------|----------------|
| `append` | Push N elements to back | Vec (no link overhead) |
| `prepend` | Push N elements to front | PieList/VecDeque (O(1) vs Vec's O(n)) |
| `iterate` | Sum all elements | Vec (best cache locality) |
| `mid_modify` | Single insert+remove at middle | IndexList/PieList (O(1) vs Vec's O(n)) |
| `multi_insert` | Insert 100 elements at random positions | **PieList** (O(n) total vs Vec's O(n²)) |
| `splice` | Merge two lists at middle (includes seek) | PieList (O(1) splice, but O(n) seek) |
| `splice_front` | Merge two lists at front (no seek) | **PieList** (O(1) vs Vec's O(n) shift) |
| `sort` | In-place sorting | Vec (pdqsort is highly optimized) |
| `random_access` | 100 random index lookups | Vec (O(1) vs linked list O(n)) |

### Pool Operations (`pool/`)

| Benchmark | What it measures | Notes |
|-----------|------------------|-------|
| `shared_lists` | Create N lists, fill with M elements, clear | Shows pool allocation patterns |

### Heap Operations (`heap/`)

These benchmarks compare `FibHeap` against other priority queue implementations.

| Benchmark | What it measures | Who should win |
|-----------|------------------|----------------|
| `push` | Bulk insertion | All similar (O(1) amortized for Fib, O(log n) for Binary) |
| `pop` | Drain all elements | BinaryHeap (simpler structure, better cache locality) |
| `decrease_key` | Update priority | **PieFibHeap** (O(1) amortized - this is its raison d'être) |

### Algorithm Benchmarks (`algo/`)

Real-world algorithm comparisons showing practical impact.

| Benchmark | What it measures | Notes |
|-----------|------------------|-------|
| `dijkstra_dense` | Dense graph (n=100, m=5000) | Tests decrease_key impact |
| `dijkstra_sparse` | Sparse graph (n=10k, m=20k) | Grid-like structure |

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
| `multi_insert` (100k) | 1.00x | **415x** | 100 inserts at random positions |
| `splice_front` (100k) | 1.00x | **1300x** | Merge at front, no traversal |
| `prepend` (1k) | 1.00x | **7.3x** | Push to front repeatedly |

### Where Vec Wins

| Benchmark | Vec | PieList | Notes |
|-----------|-----|---------|-------|
| `iterate` (10k) | 1.00x | **25x** | Sum all elements |
| `random_access` (1k) | 1.00x | **2400x** | 100 random lookups |
| `sort` (10k) | 1.00x | **10.6x** | In-place sorting |
| `append` (10k) | 1.00x | **2.6x** | Push to back |

### Heap Results

| Benchmark | BinaryHeap | FibHeap | Notes |
|-----------|------------|---------|-------|
| `push` | 1.00x | 3.9x | Bulk insertion |
| `pop` | 1.00x | **6.5x** | Extract all elements |
| `decrease_key` | N/A | **1.00x** | FibHeap's key advantage |
| `dijkstra_dense` | 1.00x | 1.35x | Dense graph (n=100, m=5000) |
| `dijkstra_sparse` | 1.00x | **4.1x** | Sparse graph (n=10k, m=20k) |

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
| `extfibheap` | `fibonacci_heap::FibonacciHeap` | Another Fibonacci heap impl |

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

✅ **Repeated middle insertions**: Inserting 100 elements at random positions—PieList is **415x faster** than Vec (grows with list size)
```rust
// PieList: O(n) to find + O(1) to insert, repeated = O(n * k)
// Vec: O(n) shift per insert, repeated = O(n * k) but with actual memory moves
```

✅ **Splicing at known positions**: When cursor is already positioned, splice is O(1)
```rust
cursor.splice_before(&mut other_list, &mut pool); // O(1)
```

✅ **Front operations**: Push/splice at front—PieList is **7-1300x faster** than Vec

✅ **Need stable indices**: Indices survive insertions/removals

✅ **Managing many small lists**: One pool serves many lists efficiently

### Use Vec Instead When...

❌ **Random access by index**: Vec is **2400x faster** (O(1) vs O(n))

❌ **Sequential iteration**: Vec is **25x faster** (perfect cache locality)

❌ **Sorting**: Vec is **10.6x faster** (pdqsort is heavily optimized)

❌ **Simple push/pop at ends only**: Vec has **2.6x less overhead**

### Use FibHeap When...

✅ **Decrease-key is critical**: Dijkstra, Prim, A* algorithms

✅ **Dense graphs**: FibHeap slightly outperforms BinaryHeap on dense graphs

### Use BinaryHeap Instead When...

❌ **No decrease-key needed**: Simple task scheduling, event queues

❌ **Maximum pop throughput**: BinaryHeap is **6x faster** for extraction

❌ **Sparse graphs**: BinaryHeap is **4x faster** on sparse graph Dijkstra

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

1. **Use consistent naming**: `category/operation` group, implementation as function
2. **Test multiple sizes**: Use `SIZES` constant
3. **Include competitors**: At least Vec/BinaryHeap as baseline
4. **Document expected winner**: In comments, explain who should win and why
5. **Use `black_box`**: Prevent compiler from optimizing away results
