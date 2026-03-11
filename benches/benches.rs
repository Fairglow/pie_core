//! Comprehensive benchmarks for pie_core data structures.
//!
//! ## Naming Convention
//!
//! Benchmarks follow a strict `{category}/{implementation}/{operation}` pattern
//! to enable easy comparison in the bench-table tool:
//!
//! - **category**: The type of data structure or algorithm being tested
//! - **implementation**: Which crate/type is being benchmarked
//! - **operation**: The specific operation being measured
//!
//! ## Categories
//!
//! - `list`: Linked list and vector operations
//! - `heap`: Priority queue operations
//! - `algo`: Real-world algorithm benchmarks
//!
//! ## Benchmark Sizes
//!
//! We test at multiple scales to show asymptotic behavior:
//! - Small (N=100): Overhead-dominated, shows constant factors
//! - Medium (N=1,000): Balanced view
//! - Large (N=10,000): Algorithm-dominated, shows O() complexity

#![cfg_attr(feature = "bench-nightly", feature(linked_list_cursors))]
use std::hint::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use pie_core::{ElemPool, PieList, FibHeap as PieFibHeap};
use index_list::IndexList;
use std::collections::{BinaryHeap, VecDeque};

// Imports for heap benchmarks
use std::cmp::Reverse;
use fibonacci_heap::FibonacciHeap as ExtFibHeap;
use priority_queue::PriorityQueue;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::RngExt;

// ============================================================================
// Configuration
// ============================================================================

/// Benchmark sizes to test
const SIZES: &[usize] = &[100, 1_000, 10_000];

// ============================================================================
// List Benchmarks: Append (push_back)
// ============================================================================
// Shows: Vec is fastest for simple appending due to cache locality.
// PieList has arena overhead but avoids per-node allocation.

fn bench_list_append(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/append");

    for &size in SIZES {
        group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::<u64>::new();
                    let list = PieList::new(&mut pool);
                    (pool, list)
                },
                |(mut pool, mut list)| {
                    for i in 0..n {
                        list.push_back(black_box(i as u64), &mut pool).unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &n| {
            b.iter_batched(
                Vec::<u64>::new,
                |mut vec| {
                    for i in 0..n {
                        vec.push(black_box(i as u64));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("vecdeque", size), &size, |b, &n| {
            b.iter_batched(
                VecDeque::<u64>::new,
                |mut deque| {
                    for i in 0..n {
                        deque.push_back(black_box(i as u64));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("indexlist", size), &size, |b, &n| {
            b.iter_batched(
                IndexList::<u64>::new,
                |mut list| {
                    for i in 0..n {
                        list.insert_last(black_box(i as u64));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Prepend (push_front)
// ============================================================================
// Shows: Vec is O(n) per insert (shifts all elements), linked lists are O(1).
// This is where PieList shines over Vec.

fn bench_list_prepend(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/prepend");

    for &size in SIZES {
        group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::<u64>::new();
                    let list = PieList::new(&mut pool);
                    (pool, list)
                },
                |(mut pool, mut list)| {
                    for i in 0..n {
                        list.push_front(black_box(i as u64), &mut pool).unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec push_front is O(n) - only benchmark small sizes to avoid timeout
        if size <= 1_000 {
            group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &n| {
                b.iter_batched(
                    Vec::<u64>::new,
                    |mut vec| {
                        for i in 0..n {
                            vec.insert(0, black_box(i as u64));
                        }
                    },
                    criterion::BatchSize::SmallInput,
                )
            });
        }

        group.bench_with_input(BenchmarkId::new("vecdeque", size), &size, |b, &n| {
            b.iter_batched(
                VecDeque::<u64>::new,
                |mut deque| {
                    for i in 0..n {
                        deque.push_front(black_box(i as u64));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("indexlist", size), &size, |b, &n| {
            b.iter_batched(
                IndexList::<u64>::new,
                |mut list| {
                    for i in 0..n {
                        list.insert_first(black_box(i as u64));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Iteration
// ============================================================================
// Shows: Vec has best cache locality. PieList is close due to arena allocation.
// std::LinkedList is worst due to scattered allocations.

fn bench_list_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/iterate");

    for &size in SIZES {
        // PieList
        group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &n| {
            // Setup once
            let mut pool = ElemPool::new();
            let mut list = PieList::new(&mut pool);
            for i in 0..n {
                list.push_back(i as u64, &mut pool).unwrap();
            }

            b.iter(|| {
                let sum: u64 = list.iter(&pool).sum();
                black_box(sum)
            })
        });

        // Vec
        group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &n| {
            let vec: Vec<u64> = (0..n as u64).collect();

            b.iter(|| {
                let sum: u64 = vec.iter().sum();
                black_box(sum)
            })
        });

        // VecDeque
        group.bench_with_input(BenchmarkId::new("vecdeque", size), &size, |b, &n| {
            let deque: VecDeque<u64> = (0..n as u64).collect();

            b.iter(|| {
                let sum: u64 = deque.iter().sum();
                black_box(sum)
            })
        });

        // IndexList
        group.bench_with_input(BenchmarkId::new("indexlist", size), &size, |b, &n| {
            let mut list = IndexList::new();
            for i in 0..n {
                list.insert_last(i as u64);
            }

            b.iter(|| {
                let sum: u64 = list.iter().map(|x| *x).sum();
                black_box(sum)
            })
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Middle Insert/Remove
// ============================================================================
// Shows: The core linked-list advantage. Vec is O(n), linked lists are O(1).
// Measures: cursor seek (O(n) for all) + insert/remove (O(1) linked, O(n) vec)

fn bench_list_mid_modify(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/mid_modify");

    for &size in SIZES {
        // PieList: O(n) seek + O(1) insert/remove
        group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::new();
                    let mut list = PieList::new(&mut pool);
                    for i in 0..n {
                        list.push_back(i as u64, &mut pool).unwrap();
                    }
                    (pool, list)
                },
                |(mut pool, mut list)| {
                    let mut cursor = list.cursor_mut_at(n / 2, &mut pool).unwrap();
                    cursor.insert_before(black_box(999), &mut pool).unwrap();
                    cursor.remove_current(&mut pool);
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec: O(n) shift for insert + O(n) shift for remove
        group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &n| {
            b.iter_batched(
                || (0..n as u64).collect::<Vec<_>>(),
                |mut vec| {
                    let mid = vec.len() / 2;
                    vec.insert(mid, black_box(999));
                    vec.remove(mid);
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // VecDeque: slightly better than Vec for middle operations
        group.bench_with_input(BenchmarkId::new("vecdeque", size), &size, |b, &n| {
            b.iter_batched(
                || (0..n as u64).collect::<VecDeque<_>>(),
                |mut deque| {
                    let mid = deque.len() / 2;
                    deque.insert(mid, black_box(999));
                    deque.remove(mid);
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // IndexList
        group.bench_with_input(BenchmarkId::new("indexlist", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut list = IndexList::<u64>::new();
                    for i in 0..n {
                        list.insert_last(i as u64);
                    }
                    list
                },
                |mut list| {
                    let middle_idx = list.iter().nth(n / 2).unwrap();
                    let new_idx = list.insert_before((*middle_idx).into(), black_box(999));
                    list.remove(new_idx);
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Repeated Middle Inserts (Linked List Advantage)
// ============================================================================
// Shows: When you need to insert MANY elements at various positions,
// linked lists maintain O(1) insert while Vec accumulates O(n) shifts.

fn bench_list_multi_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/multi_insert");

    // Insert 100 elements at random positions into a list of size N
    const INSERT_COUNT: usize = 100;

    let mut rng = StdRng::seed_from_u64(42);
    let insert_positions: Vec<usize> = (0..INSERT_COUNT)
        .map(|i| rng.random_range(0..(i + 1)))  // Valid position for each insert
        .collect();

    for &size in &[1_000usize, 10_000, 100_000] {
        // PieList: O(n) to find position, O(1) to insert, repeated 100 times
        // Total: O(100 * n) for finding + O(100) for inserting
        group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::new();
                    pool.reserve(n + INSERT_COUNT + 1);
                    let mut list = PieList::new(&mut pool);
                    for i in 0..n {
                        list.push_back(i as u64, &mut pool).unwrap();
                    }
                    (pool, list)
                },
                |(mut pool, mut list)| {
                    for (i, &pos) in insert_positions.iter().enumerate() {
                        let insert_at = pos % list.len().max(1);
                        let mut cursor = list.cursor_mut_at(insert_at, &mut pool).unwrap();
                        cursor.insert_before(black_box(i as u64), &mut pool).unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec: O(n) shift per insert, repeated 100 times
        // Total: O(100 * n) - but with actual memory moves
        group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &n| {
            b.iter_batched(
                || (0..n as u64).collect::<Vec<_>>(),
                |mut vec| {
                    for (i, &pos) in insert_positions.iter().enumerate() {
                        let insert_at = pos % vec.len().max(1);
                        vec.insert(insert_at, black_box(i as u64));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Splice (O(1) linked list advantage)
// ============================================================================
// Shows: Splicing two lists together. Vec must copy, linked lists just relink.
//
// IMPORTANT: This benchmark includes cursor positioning time for PieList (O(n)),
// which is unfair to linked lists. The true splice is O(1) once positioned.
// See bench_list_splice_only for isolated splice measurement.

fn bench_list_splice(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/splice");

    // Test with splice sizes proportional to main list
    // Format: (main_size, splice_size)
    let scenarios = [
        (1_000, 500),      // 50% splice
        (10_000, 5_000),   // 50% splice, larger scale
        (100_000, 50_000), // 50% splice, large scale
    ];

    for (main_size, splice_size) in scenarios {
        let label = format!("{}+{}", main_size, splice_size);

        // PieList: O(n) to find position + O(1) splice
        group.bench_with_input(BenchmarkId::new("pielist", &label), &(main_size, splice_size), |b, &(n, s)| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::new();
                    pool.reserve(n + s + 2);
                    let mut list1 = PieList::new(&mut pool);
                    for i in 0..n {
                        list1.push_back(i as u64, &mut pool).unwrap();
                    }

                    let mut list2 = PieList::new(&mut pool);
                    for i in 0..s {
                        list2.push_back(i as u64, &mut pool).unwrap();
                    }
                    (pool, list1, list2)
                },
                |(mut pool, mut list1, mut list2)| {
                    let mut cursor = list1.cursor_mut_at(n / 2, &mut pool).unwrap();
                    cursor.splice_before(black_box(&mut list2), &mut pool).unwrap();
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec: O(n) copy for splice
        group.bench_with_input(BenchmarkId::new("vec", &label), &(main_size, splice_size), |b, &(n, s)| {
            b.iter_batched(
                || {
                    let vec1: Vec<u64> = (0..n as u64).collect();
                    let vec2: Vec<u64> = (0..s as u64).collect();
                    (vec1, vec2)
                },
                |(mut vec1, vec2)| {
                    let mid = vec1.len() / 2;
                    vec1.splice(mid..mid, black_box(vec2));
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Splice at Front (No Traversal - True O(1) for PieList)
// ============================================================================
// Shows: The true O(1) splice advantage when already positioned at front.
// PieList splices at front without any traversal. Vec still shifts all elements.

fn bench_list_splice_front(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/splice_front");

    let scenarios = [
        (10_000, 5_000),
        (100_000, 50_000),
    ];

    for (main_size, splice_size) in scenarios {
        let label = format!("{}+{}", main_size, splice_size);

        // PieList: True O(1) - no traversal needed to get to front
        group.bench_with_input(BenchmarkId::new("pielist", &label), &(main_size, splice_size), |b, &(n, s)| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::new();
                    pool.reserve(n + s + 2);
                    let mut list1 = PieList::new(&mut pool);
                    for i in 0..n {
                        list1.push_back(i as u64, &mut pool).unwrap();
                    }

                    let mut list2 = PieList::new(&mut pool);
                    for i in 0..s {
                        list2.push_back(i as u64, &mut pool).unwrap();
                    }
                    (pool, list1, list2)
                },
                |(mut pool, mut list1, mut list2)| {
                    // cursor_mut starts at front - no traversal!
                    let mut cursor = list1.cursor_mut(&mut pool);
                    cursor.splice_before(black_box(&mut list2), &mut pool).unwrap();
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec: O(n) - must shift all elements right
        group.bench_with_input(BenchmarkId::new("vec", &label), &(main_size, splice_size), |b, &(n, s)| {
            b.iter_batched(
                || {
                    let vec1: Vec<u64> = (0..n as u64).collect();
                    let vec2: Vec<u64> = (0..s as u64).collect();
                    (vec1, vec2)
                },
                |(mut vec1, vec2)| {
                    // Insert at front - worst case for Vec
                    vec1.splice(0..0, black_box(vec2));
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// List Benchmarks: Sort
// ============================================================================
// Shows: Sorting comparison. Vec uses highly optimized pdqsort.
// PieList uses merge sort (stable, O(n log n), O(log n) auxiliary space).

fn bench_list_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/sort");

    // Create deterministic "random" data
    let mut rng = StdRng::seed_from_u64(42);

    for &size in SIZES {
        let mut random_data: Vec<u64> = (0..size as u64).collect();
        random_data.shuffle(&mut rng);
        let random_data = random_data; // Make immutable for cloning

        // PieList
        group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &_n| {
            b.iter_batched(
                || {
                    let mut pool = ElemPool::new();
                    let mut list = PieList::new(&mut pool);
                    for &val in &random_data {
                        list.push_back(val, &mut pool).unwrap();
                    }
                    (pool, list)
                },
                |(mut pool, mut list)| {
                    list.sort(&mut pool, |a, b| a.cmp(b));
                    black_box(&mut list);
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec
        group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &_n| {
            b.iter_batched(
                || random_data.clone(),
                |mut vec| {
                    vec.sort();
                    black_box(&mut vec);
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// Nightly-only: std::collections::LinkedList comparison
// ============================================================================
#[cfg(feature = "bench-nightly")]
mod linked_list_bench {
    use super::*;
    use std::collections::LinkedList;

    pub fn bench_linkedlist_mid_modify(c: &mut Criterion) {
        let mut group = c.benchmark_group("list/mid_modify");

        for &size in super::SIZES {
            group.bench_with_input(BenchmarkId::new("linkedlist", size), &size, |b, &n| {
                b.iter_batched(
                    || {
                        let mut list = LinkedList::new();
                        for i in 0..n {
                            list.push_back(i as u64);
                        }
                        list
                    },
                    |mut list| {
                        let mut cursor = list.cursor_front_mut();
                        for _ in 0..(n / 2) {
                            cursor.move_next();
                        }
                        cursor.insert_before(black_box(999));
                        cursor.remove_current();
                    },
                    criterion::BatchSize::SmallInput,
                )
            });
        }

        group.finish();
    }

    pub fn bench_linkedlist_iterate(c: &mut Criterion) {
        let mut group = c.benchmark_group("list/iterate");

        for &size in super::SIZES {
            group.bench_with_input(BenchmarkId::new("linkedlist", size), &size, |b, &n| {
                let list: LinkedList<u64> = (0..n as u64).collect();

                b.iter(|| {
                    let sum: u64 = list.iter().sum();
                    black_box(sum)
                })
            });
        }

        group.finish();
    }
}

// ============================================================================
// Heap Benchmarks: Push (Bulk Insertion)
// ============================================================================
// Shows: FibHeap has O(1) amortized push, BinaryHeap has O(log n).
// In practice, BinaryHeap's cache locality often wins for small N.

fn bench_heap_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap/push");

    for &size in SIZES {
        group.bench_with_input(BenchmarkId::new("piefibheap", size), &size, |b, &n| {
            b.iter_batched(
                PieFibHeap::<usize, usize>::new,
                |mut heap| {
                    for i in 0..n {
                        heap.push(black_box(i), black_box(i));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("binaryheap", size), &size, |b, &n| {
            b.iter_batched(
                BinaryHeap::<(Reverse<usize>, usize)>::new,
                |mut heap| {
                    for i in 0..n {
                        heap.push(black_box((Reverse(i), i)));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("priorityqueue", size), &size, |b, &n| {
            b.iter_batched(
                PriorityQueue::<usize, Reverse<usize>>::new,
                |mut heap| {
                    for i in 0..n {
                        heap.push(black_box(i), black_box(Reverse(i)));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("extfibheap", size), &size, |b, &n| {
            b.iter_batched(
                ExtFibHeap::new,
                |mut heap| {
                    for i in 0..n {
                        heap.insert(black_box(i as i32)).unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// Heap Benchmarks: Pop All (Drain)
// ============================================================================
// Shows: Popping all elements. Tests consolidation overhead for FibHeaps.
// BinaryHeap should win due to simpler structure and cache locality.

fn bench_heap_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap/pop");

    let mut rng = StdRng::seed_from_u64(42);

    for &size in SIZES {
        let mut random_keys: Vec<usize> = (0..size).collect();
        random_keys.shuffle(&mut rng);
        let random_keys = random_keys; // Make immutable

        group.bench_with_input(BenchmarkId::new("piefibheap", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = PieFibHeap::new();
                    for &key in &random_keys {
                        heap.push(key, key);
                    }
                    heap
                },
                |mut heap| {
                    for _ in 0..n {
                        black_box(heap.pop().unwrap());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("binaryheap", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = BinaryHeap::new();
                    for &key in &random_keys {
                        heap.push((Reverse(key), key));
                    }
                    heap
                },
                |mut heap| {
                    for _ in 0..n {
                        black_box(heap.pop().unwrap());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("priorityqueue", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = PriorityQueue::new();
                    for (i, &key) in random_keys.iter().enumerate() {
                        heap.push(i, Reverse(key));
                    }
                    heap
                },
                |mut heap| {
                    for _ in 0..n {
                        black_box(heap.pop().unwrap());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("extfibheap", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = ExtFibHeap::new();
                    for &key in &random_keys {
                        heap.insert(key as i32).unwrap();
                    }
                    heap
                },
                |mut heap| {
                    for _ in 0..n {
                        black_box(heap.extract_min().unwrap());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// Heap Benchmarks: Decrease Key (The FibHeap Advantage)
// ============================================================================
// Shows: THE key advantage of Fibonacci heaps - O(1) amortized decrease_key.
// BinaryHeap cannot do decrease_key; must use "lazy" push (shown as simulation).
// PriorityQueue supports change_priority but with O(log n) complexity.

fn bench_heap_decrease_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap/decrease_key");

    let mut rng = StdRng::seed_from_u64(42);

    for &size in SIZES {
        let mut random_indices: Vec<usize> = (0..size).collect();
        random_indices.shuffle(&mut rng);
        let random_indices = random_indices;

        // PieFibHeap: O(1) amortized decrease_key
        group.bench_with_input(BenchmarkId::new("piefibheap", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = PieFibHeap::new();
                    let mut handles = Vec::with_capacity(n);
                    for i in 0..n {
                        // Large initial key
                        handles.push(heap.push(n * 2, i));
                    }
                    (heap, handles)
                },
                |(mut heap, handles)| {
                    for i in 0..n {
                        let handle = handles[random_indices[i]];
                        heap.decrease_key(handle, black_box(i)).ok();
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // PriorityQueue: O(log n) change_priority
        group.bench_with_input(BenchmarkId::new("priorityqueue", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = PriorityQueue::new();
                    for i in 0..n {
                        heap.push(i, Reverse(n * 2));
                    }
                    heap
                },
                |mut heap| {
                    for i in 0..n {
                        let item_id = &random_indices[i];
                        heap.change_priority(item_id, black_box(Reverse(i)));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // BinaryHeap: Simulated decrease_key via lazy push
        // (This is the standard Dijkstra workaround)
        group.bench_with_input(BenchmarkId::new("binaryheap_lazy", size), &size, |b, &n| {
            b.iter_batched(
                || {
                    let mut heap = BinaryHeap::new();
                    for i in 0..n {
                        heap.push((Reverse(n * 2), i));
                    }
                    heap
                },
                |mut heap| {
                    for i in 0..n {
                        let item_id = random_indices[i];
                        // Just push a new entry with lower priority
                        heap.push(black_box((Reverse(i), item_id)));
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// Heap Benchmarks: Push then Pop All (Combined)
// ============================================================================
// Shows: Total cost of push followed by pop. BinaryHeap should dominate
// because FibHeap consolidation overhead during pop is expensive.

fn bench_heap_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap/push_pop");

    for &size in SIZES {
        group.bench_with_input(BenchmarkId::new("piefibheap", size), &size, |b, &n| {
            b.iter_batched(
                || (),
                |()| {
                    let mut heap = PieFibHeap::new();
                    for i in 0..n {
                        heap.push(black_box(i), i);
                    }
                    for _ in 0..n {
                        black_box(heap.pop());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("binaryheap", size), &size, |b, &n| {
            b.iter_batched(
                || (),
                |()| {
                    let mut heap = BinaryHeap::new();
                    for i in 0..n {
                        heap.push(black_box((Reverse(i), i)));
                    }
                    for _ in 0..n {
                        black_box(heap.pop());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.bench_with_input(BenchmarkId::new("priorityqueue", size), &size, |b, &n| {
            b.iter_batched(
                || (),
                |()| {
                    let mut heap = PriorityQueue::new();
                    for i in 0..n {
                        heap.push(black_box(i), black_box(Reverse(i)));
                    }
                    for _ in 0..n {
                        black_box(heap.pop());
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// Heap Benchmarks: Peek (Access minimum without removal)
// ============================================================================
// Shows: All heaps should be O(1) and extremely fast.

fn bench_heap_peek(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap/peek");
    let size = 10_000;

    let mut rng = StdRng::seed_from_u64(42);
    let mut random_keys: Vec<usize> = (0..size).collect();
    random_keys.shuffle(&mut rng);

    // Pre-fill heaps
    let mut pie_heap = PieFibHeap::new();
    for &key in &random_keys {
        pie_heap.push(key, key);
    }

    let mut binary_heap = BinaryHeap::new();
    for &key in &random_keys {
        binary_heap.push((Reverse(key), key));
    }

    let mut pq_heap = PriorityQueue::new();
    for (i, &key) in random_keys.iter().enumerate() {
        pq_heap.push(i, Reverse(key));
    }

    group.bench_function("piefibheap", |b| {
        b.iter(|| black_box(pie_heap.peek()))
    });

    group.bench_function("binaryheap", |b| {
        b.iter(|| black_box(binary_heap.peek()))
    });

    group.bench_function("priorityqueue", |b| {
        b.iter(|| black_box(pq_heap.peek()))
    });

    group.finish();
}

// ============================================================================
// List Benchmarks: Random Access
// ============================================================================
// Shows: Vec O(1) vs linked list O(n) per access.
// This is Vec's strength - linked lists must traverse.

fn bench_list_random_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("list/random_access");
    let access_count = 100; // Number of random accesses to perform

    for &size in SIZES {
        let mut rng = StdRng::seed_from_u64(42);
        let mut random_indices: Vec<usize> = (0..access_count).map(|_| {
            let mut idx: Vec<usize> = (0..size).collect();
            idx.shuffle(&mut rng);
            idx[0]
        }).collect();
        random_indices.shuffle(&mut rng);

        group.bench_with_input(BenchmarkId::new("vec", size), &size, |b, &n| {
            let vec: Vec<u64> = (0..n as u64).collect();

            b.iter(|| {
                let mut sum = 0u64;
                for &idx in &random_indices {
                    sum += vec[idx];
                }
                black_box(sum)
            })
        });

        group.bench_with_input(BenchmarkId::new("vecdeque", size), &size, |b, &n| {
            let deque: VecDeque<u64> = (0..n as u64).collect();

            b.iter(|| {
                let mut sum = 0u64;
                for &idx in &random_indices {
                    sum += deque[idx];
                }
                black_box(sum)
            })
        });

        // Only run linked list random access for small sizes (O(n) per access)
        if size <= 1_000 {
            group.bench_with_input(BenchmarkId::new("pielist", size), &size, |b, &n| {
                let mut pool = ElemPool::new();
                let mut list = PieList::new(&mut pool);
                for i in 0..n {
                    list.push_back(i as u64, &mut pool).unwrap();
                }

                b.iter(|| {
                    let mut sum = 0u64;
                    for &idx in &random_indices {
                        if let Some(val) = list.iter(&pool).nth(idx) {
                            sum += val;
                        }
                    }
                    black_box(sum)
                })
            });

            group.bench_with_input(BenchmarkId::new("indexlist", size), &size, |b, &n| {
                let mut ilist = IndexList::new();
                for i in 0..n {
                    ilist.insert_last(i as u64);
                }

                b.iter(|| {
                    let mut sum = 0u64;
                    for &idx in &random_indices {
                        if let Some(val) = ilist.iter().nth(idx) {
                            sum += *val;
                        }
                    }
                    black_box(sum)
                })
            });
        }
    }

    group.finish();
}

// ============================================================================
// Algorithm Benchmarks: Dijkstra's Shortest Path
// ============================================================================
#[cfg(feature = "petgraph")]
mod dijkstra_bench {
    use super::*;
    use petgraph::graph::Graph;
    use petgraph::algo::dijkstra;
    use pie_core::dijkstra_pie_core;

    /// Creates a dense graph with `node_count` nodes and `edge_count` random edges.
    fn create_dense_graph(node_count: usize, edge_count: usize) -> Graph<&'static str, u64> {
        let mut graph = Graph::new();
        let mut rng = StdRng::seed_from_u64(42);

        let nodes: Vec<_> = (0..node_count).map(|_| graph.add_node("")).collect();

        for _ in 0..edge_count {
            let a_idx = rng.random_range(0..node_count);
            let b_idx = rng.random_range(0..node_count);
            if a_idx == b_idx {
                continue;
            }
            let weight = rng.random_range(1..1000);
            graph.add_edge(nodes[a_idx], nodes[b_idx], weight);
        }
        graph
    }

    /// Creates a sparse grid graph of `width` x `height`.
    fn create_sparse_graph(width: usize, height: usize) -> Graph<&'static str, u64> {
        let mut graph = Graph::new();
        let mut rng = StdRng::seed_from_u64(42);

        let nodes: Vec<Vec<_>> = (0..height)
            .map(|_| (0..width).map(|_| graph.add_node("")).collect())
            .collect();

        for r in 0..height {
            for c in 0..width {
                let current_node = nodes[r][c];
                if c + 1 < width {
                    let right_node = nodes[r][c + 1];
                    let weight = rng.random_range(1..100);
                    graph.add_edge(current_node, right_node, weight);
                }
                if r + 1 < height {
                    let down_node = nodes[r + 1][c];
                    let weight = rng.random_range(1..100);
                    graph.add_edge(current_node, down_node, weight);
                }
            }
        }
        graph
    }

    pub fn bench_dijkstra_dense(c: &mut Criterion) {
        // n=100, m=5000 (close to n²/2 - very dense)
        let dense_graph = create_dense_graph(100, 5000);
        let start_node = dense_graph.node_indices().next().unwrap();

        let mut group = c.benchmark_group("algo/dijkstra_dense");

        group.bench_function("petgraph_binaryheap", |b| {
            b.iter(|| {
                dijkstra(
                    black_box(&dense_graph),
                    black_box(start_node),
                    None,
                    |e| *e.weight(),
                )
            })
        });

        group.bench_function("pie_core_fibheap", |b| {
            b.iter(|| dijkstra_pie_core(black_box(&dense_graph), black_box(start_node)))
        });

        group.finish();
    }

    pub fn bench_dijkstra_sparse(c: &mut Criterion) {
        // 100x100 grid: n=10,000, m≈20,000 (m ≈ 2n - very sparse)
        let sparse_graph = create_sparse_graph(100, 100);
        let start_node = sparse_graph.node_indices().next().unwrap();

        let mut group = c.benchmark_group("algo/dijkstra_sparse");

        group.bench_function("petgraph_binaryheap", |b| {
            b.iter(|| {
                dijkstra(
                    black_box(&sparse_graph),
                    black_box(start_node),
                    None,
                    |e| *e.weight(),
                )
            })
        });

        group.bench_function("pie_core_fibheap", |b| {
            b.iter(|| dijkstra_pie_core(black_box(&sparse_graph), black_box(start_node)))
        });

        group.finish();
    }
}

// ============================================================================
// Pool Sharing Benchmark: PieList's Unique Advantage
// ============================================================================
// Shows: Creating many lists that share a pool vs. independent Vecs.
// PieList avoids per-list allocator overhead when creating/destroying lists.

fn bench_pool_shared_lists(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool/shared_lists");

    // Create N lists, each with M elements, then destroy them all
    for &(list_count, elements_per_list) in &[(100, 100), (1000, 10), (10, 1000)] {
        let label = format!("{}x{}", list_count, elements_per_list);

        // PieList: All lists share one pre-allocated pool
        group.bench_with_input(BenchmarkId::new("pielist_shared", &label), &(list_count, elements_per_list), |b, &(n_lists, n_elems)| {
            b.iter_batched(
                || (),
                |()| {
                    let mut pool = ElemPool::new();
                    pool.reserve(n_lists * n_elems + n_lists + 1);
                    let mut lists: Vec<PieList<u64>> = Vec::with_capacity(n_lists);

                    // Create all lists
                    for i in 0..n_lists {
                        let mut list = PieList::new(&mut pool);
                        for j in 0..n_elems {
                            list.push_back((i * n_elems + j) as u64, &mut pool).unwrap();
                        }
                        lists.push(list);
                    }

                    // Sum and clear all lists
                    let mut total_len = 0;
                    for mut list in lists {
                        total_len += list.len();
                        list.clear(&mut pool);
                    }

                    black_box(total_len)
                },
                criterion::BatchSize::SmallInput,
            )
        });

        // Vec: Each Vec is independent
        group.bench_with_input(BenchmarkId::new("vec_separate", &label), &(list_count, elements_per_list), |b, &(n_lists, n_elems)| {
            b.iter_batched(
                || (),
                |()| {
                    let mut vecs: Vec<Vec<u64>> = Vec::with_capacity(n_lists);

                    // Create all vecs
                    for i in 0..n_lists {
                        let mut vec = Vec::with_capacity(n_elems);
                        for j in 0..n_elems {
                            vec.push((i * n_elems + j) as u64);
                        }
                        vecs.push(vec);
                    }

                    // Sum and clear all vecs
                    let mut total_len = 0;
                    for vec in &vecs {
                        total_len += vec.len();
                    }
                    // vecs dropped here, triggering n_lists deallocations

                    black_box(total_len)
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

// ============================================================================
// Benchmark Registration
// ============================================================================

fn register_benches(c: &mut Criterion) {
    // List benchmarks
    bench_list_append(c);
    bench_list_prepend(c);
    bench_list_iterate(c);
    bench_list_mid_modify(c);
    bench_list_multi_insert(c);
    bench_list_splice(c);
    bench_list_splice_front(c);
    bench_list_sort(c);
    bench_list_random_access(c);

    // Pool sharing benchmarks
    bench_pool_shared_lists(c);

    // Heap benchmarks
    bench_heap_push(c);
    bench_heap_pop(c);
    bench_heap_decrease_key(c);
    bench_heap_push_pop(c);
    bench_heap_peek(c);

    // Nightly-only benchmarks
    #[cfg(feature = "bench-nightly")]
    {
        linked_list_bench::bench_linkedlist_mid_modify(c);
        linked_list_bench::bench_linkedlist_iterate(c);
    }

    // Petgraph-based algorithm benchmarks
    #[cfg(feature = "petgraph")]
    {
        dijkstra_bench::bench_dijkstra_dense(c);
        dijkstra_bench::bench_dijkstra_sparse(c);
    }
}

criterion_group!(benches, register_benches);
criterion_main!(benches);