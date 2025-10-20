use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use pie_core::{ElemPool, PieList, FibHeap as PieFibHeap};
use index_list::IndexList; // Import the crate for comparison

// --- Imports for heap benchmarks ---
use std::collections::BinaryHeap;
use std::cmp::Reverse; // To turn max-heaps into min-heaps
use fibonacci_heap::FibonacciHeap as ExtFibHeap;
use priority_queue::PriorityQueue;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
// ---------------------------------

const LIST_SIZE: usize = 1000;
const HEAP_SIZE: usize = 1000; // Use the same size for heap tests

// ##################################################################
// # PieList Benchmarks
// ##################################################################

/// Benchmark for appending elements to the end of the list.
/// This measures the combined performance of pool allocation and linking.
fn push_back_benchmark(c: &mut Criterion) {
    c.bench_function("pielist-push_back", |b| {
        b.iter_batched(
            // Setup: Create a new pool and list for each iteration to measure
            // from a clean state.
            || {
                let mut pool = ElemPool::<u64>::new();
                let list = PieList::new(&mut pool);
                (pool, list)
            },
            // The actual code being measured.
            |(mut pool, mut list)| {
                for i in 0..LIST_SIZE {
                    list.push_back(black_box(i as u64), &mut pool).unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

/// Benchmark for iterating through all elements of the list.
/// This highlights the cache-locality benefits of the arena allocator. A tight
/// loop over the contiguous memory in the pool should be very fast.
fn iter_benchmark(c: &mut Criterion) {
    // Setup: Create a single, pre-filled list.
    let mut pool = ElemPool::new();
    let mut list = PieList::new(&mut pool);
    for i in 0..LIST_SIZE {
        list.push_back(i as u64, &mut pool).unwrap();
    }

    c.bench_function("pielist-iter_sum", |b| {
        b.iter(|| {
            // Summing the elements ensures the compiler doesn't optimize away the loop.
            let sum: u64 = list.iter(&pool).sum();
            black_box(sum);
        })
    });
}

/// Benchmark for a classic linked-list use case: frequent modifications
/// in the middle of the list. This is where a `Vec` would perform poorly due
/// to needing to shift all subsequent elements.
fn pielist_insert_remove_middle_benchmark(c: &mut Criterion) {
    c.bench_function("pielist-insert_remove_middle", |b| {
        b.iter_batched(
            // Setup: Create a pre-filled list for each run.
            || {
                let mut pool = ElemPool::new();
                let mut list = PieList::new(&mut pool);
                for i in 0..LIST_SIZE {
                    list.push_back(i as u64, &mut pool).unwrap();
                }
                (pool, list)
            },
            // Measured code: get a cursor to the middle, insert, then remove.
            |(mut pool, mut list)| {
                let mut cursor = list.cursor_mut_at(LIST_SIZE / 2, &mut pool).unwrap();
                cursor.insert_before(black_box(999), &mut pool).unwrap();
                // We must re-borrow the pool for the next operation.
                cursor.remove_current(&mut pool);
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

/// Comparison benchmark for the same "insert/remove middle" operation using `index_list`.
fn index_list_insert_remove_middle_benchmark(c: &mut Criterion) {
    c.bench_function("index_list-insert_remove_middle", |b| {
        b.iter_batched(
            // Setup: Create a pre-filled IndexList.
            || {
                let mut list = IndexList::<u64>::new();
                for i in 0..LIST_SIZE {
                    list.insert_last(i as u64);
                }
                list
            },
            // Measured code: find the middle index, insert, then remove.
            |mut list| {
                // NOTE: Finding the middle index is O(n) in index_list's API,
                // which is part of the cost of using it.
                let middle_idx = list.iter().nth(LIST_SIZE / 2).unwrap();
                let new_idx = list.insert_before((*middle_idx).into(), black_box(999));
                list.remove(new_idx);
            },
            criterion::BatchSize::SmallInput,
        )
    });
}


/// Benchmark for splicing one list into another.
/// This operation is O(1) in a linked list but would be O(n) in a `Vec`.
/// This benchmark showcases one of the most powerful features of the cursor API.
fn splice_before_benchmark(c: &mut Criterion) {
    c.bench_function("pielist-splice_before_middle", |b| {
        b.iter_batched(
            // Setup: Create two lists, one to splice into and one to consume.
            || {
                let mut pool = ElemPool::new();
                let mut list1 = PieList::new(&mut pool);
                for i in 0..LIST_SIZE {
                    list1.push_back(i as u64, &mut pool).unwrap();
                }

                let mut list2 = PieList::new(&mut pool);
                for i in 0..50 { // A smaller list to splice in
                    list2.push_back(i as u64, &mut pool).unwrap();
                }
                (pool, list1, list2)
            },
            // Measured code: get a cursor and perform the splice.
            |(mut pool, mut list1, mut list2)| {
                let mut cursor = list1.cursor_mut_at(LIST_SIZE / 2, &mut pool).unwrap();
                cursor.splice_before(black_box(&mut list2), &mut pool).unwrap();
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

/// Benchmark for sorting the `PieList`.
fn pielist_sort_benchmark(c: &mut Criterion) {
    c.bench_function("pielist-sort", |b| {
        b.iter_batched(
            // Setup: Create a list with elements in reverse order to ensure
            // it's not already sorted.
            || {
                let mut pool = ElemPool::new();
                let mut list = PieList::new(&mut pool);
                for i in (0..LIST_SIZE).rev() {
                    list.push_back(black_box(i as u64), &mut pool).unwrap();
                }
                (pool, list)
            },
            // Measured code: sort the list.
            |(mut pool, mut list)| {
                list.sort(&mut pool, |a, b| a.cmp(b));
                // Black box the result to prevent the operation from being optimized away.
                black_box(&mut list);
            },
            criterion::BatchSize::SmallInput,
        )
    });
}


// ##################################################################
// # FibHeap Benchmarks
// ##################################################################

// --- Scenario 1: Bulk Push ---
// Measures the O(1) amortized push of FibHeaps vs O(log n) of others.

fn bench_heap_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_push_sequential");

    group.bench_function("pielist-push", |b| {
        b.iter_batched(
            || PieFibHeap::<usize, usize>::new(),
            |mut heap| {
                for i in 0..HEAP_SIZE {
                    heap.push(black_box(i), black_box(i));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("extfibheap-push", |b| {
        b.iter_batched(
            || ExtFibHeap::new(),
            |mut heap| {
                for i in 0..HEAP_SIZE {
                    heap.insert(black_box(i as i32)).unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("binaryheap-push", |b| {
        b.iter_batched(
            || BinaryHeap::<(Reverse<usize>, usize)>::new(),
            |mut heap| {
                for i in 0..HEAP_SIZE {
                    // We use Reverse(i) to make the max-heap a min-heap.
                    heap.push(black_box((Reverse(i), i)));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("priorityqueue-push", |b| {
        b.iter_batched(
            || PriorityQueue::<usize, Reverse<usize>>::new(),
            |mut heap| {
                for i in 0..HEAP_SIZE {
                    // We use Reverse(i) to make the max-priority queue a min-priority queue.
                    // The first `i` is the item's ID, the second is its priority.
                    heap.push(black_box(i), black_box(Reverse(i)));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}


// --- Scenario 2: Full Drain (Pop All) ---
// Pushes N random items, then measures popping all N items.
// This heavily tests the `pop` / `consolidate` logic.

fn bench_heap_pop_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_pop_all_random");

    // Setup: Create a single vec of random numbers to push.
    let mut rng = StdRng::seed_from_u64(42);
    let mut random_keys: Vec<usize> = (0..HEAP_SIZE).collect();
    random_keys.shuffle(&mut rng);

    group.bench_function("pielist-pop_all", |b| {
        b.iter_batched(
            || {
                let mut heap = PieFibHeap::new();
                for &key in &random_keys {
                    heap.push(key, key);
                }
                heap
            },
            |mut heap| {
                for _ in 0..HEAP_SIZE {
                    black_box(heap.pop().unwrap());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("extfibheap-pop_all", |b| {
        b.iter_batched(
            || {
                let mut heap = ExtFibHeap::new();
                for &key in &random_keys {
                    heap.insert(key as i32).unwrap();
                }
                heap
            },
            |mut heap| {
                for _ in 0..HEAP_SIZE {
                    black_box(heap.extract_min().unwrap());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("binaryheap-pop_all", |b| {
        b.iter_batched(
            || {
                let mut heap = BinaryHeap::new();
                for &key in &random_keys {
                    heap.push((Reverse(key), key));
                }
                heap
            },
            |mut heap| {
                for _ in 0..HEAP_SIZE {
                    black_box(heap.pop().unwrap());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("priorityqueue-pop_all", |b| {
        b.iter_batched(
            || {
                let mut heap = PriorityQueue::new();
                for (i, &key) in random_keys.iter().enumerate() {
                    heap.push(i, Reverse(key)); // Use 'i' as the unique ID
                }
                heap
            },
            |mut heap| {
                for _ in 0..HEAP_SIZE {
                    black_box(heap.pop().unwrap());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// --- Scenario 3: Decrease Key (Dijkstra-like) ---
// Pushes N items, then repeatedly calls decrease_key on random items.
// This is the main theoretical advantage of Fibonacci heaps.

fn bench_heap_decrease_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_decrease_key_random");

    // Setup: Create random indices to update.
    let mut rng = StdRng::seed_from_u64(42);
    let mut random_indices: Vec<usize> = (0..HEAP_SIZE).collect();
    // We shuffle the indices to ensure we update nodes in a random order,
    // which is a more realistic workload.
    random_indices.shuffle(&mut rng);

    // This vector is no longer needed, as we will generate keys sequentially.
    // let random_new_keys: Vec<usize> = (0..HEAP_SIZE).map(|_| rng.gen_range(0..HEAP_SIZE)).collect();

    group.bench_function("pielist-decrease_key", |b| {
        b.iter_batched(
            || {
                let mut heap = PieFibHeap::new();
                let mut handles = Vec::with_capacity(HEAP_SIZE);
                for i in 0..HEAP_SIZE {
                    // MODIFICATION: Initialize with a key that is guaranteed to be
                    // larger than any key we will decrease to.
                    handles.push(heap.push(HEAP_SIZE * 2, i));
                }
                (heap, handles)
            },
            |(mut heap, handles)| {
                // MODIFICATION: Iterate through the shuffled indices to ensure each
                // node is updated exactly once per batch run.
                for i in 0..HEAP_SIZE {
                    // The handle to the node we will update.
                    let handle = handles[random_indices[i]];
                    // The new key is `i`. Since the initial key was HEAP_SIZE * 2,
                    // this is always a valid decrease (e.g., 2000 -> 0, 2000 -> 1, etc.).
                    let new_key = i;
                    heap.decrease_key(handle, black_box(new_key));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("priorityqueue-change_priority", |b| {
        b.iter_batched(
            || {
                let mut heap = PriorityQueue::new();
                for i in 0..HEAP_SIZE {
                    // MODIFICATION: Use the same larger initial key.
                    heap.push(i, Reverse(HEAP_SIZE * 2));
                }
                heap
            },
            |mut heap| {
                // MODIFICATION: Use the same logic as the pielist bench.
                for i in 0..HEAP_SIZE {
                    let item_id = &random_indices[i];
                    let new_key = i;
                    heap.change_priority(item_id, black_box(Reverse(new_key)));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("binaryheap-push", |b| {
        b.iter_batched(
            || {
                let mut heap = BinaryHeap::new();
                for i in 0..HEAP_SIZE {
                    // MODIFICATION: Use the same larger initial key.
                    heap.push((Reverse(HEAP_SIZE * 2), i));
                }
                heap
            },
            |mut heap| {
                // MODIFICATION: Use the same logic as the pielist bench.
                for i in 0..HEAP_SIZE {
                    let item_id = random_indices[i];
                    let new_key = i;
                    heap.push(black_box((Reverse(new_key), item_id)));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}


// Group the benchmarks and define the main entry point.
criterion_group!(
    benches,
    // PieList benchmarks
    push_back_benchmark,
    iter_benchmark,
    pielist_insert_remove_middle_benchmark,
    index_list_insert_remove_middle_benchmark,
    splice_before_benchmark,
    pielist_sort_benchmark,

    // FibHeap benchmarks
    bench_heap_push,
    bench_heap_pop_all,
    bench_heap_decrease_key,
);
criterion_main!(benches);