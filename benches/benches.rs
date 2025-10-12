use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use pielist::{ElemPool, PieList};

const LIST_SIZE: usize = 1000;

/// Benchmark for appending elements to the end of the list.
/// This measures the combined performance of pool allocation and linking.
fn push_back_benchmark(c: &mut Criterion) {
    c.bench_function("push_back", |b| {
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

    c.bench_function("iter_sum", |b| {
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
fn cursor_insert_remove_middle_benchmark(c: &mut Criterion) {
    c.bench_function("cursor_insert_remove_middle", |b| {
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
                cursor.remove_current(&mut pool);
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

/// Benchmark for splicing one list into another.
/// This operation is O(1) in a linked list but would be O(n) in a `Vec`.
/// This benchmark showcases one of the most powerful features of the cursor API.
fn splice_before_benchmark(c: &mut Criterion) {
    c.bench_function("splice_before_middle", |b| {
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

// Group the benchmarks and define the main entry point.
criterion_group!(
    benches,
    push_back_benchmark,
    iter_benchmark,
    cursor_insert_remove_middle_benchmark,
    splice_before_benchmark
);
criterion_main!(benches);
