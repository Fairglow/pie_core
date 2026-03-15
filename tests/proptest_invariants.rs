//! Property-based tests for `pie_core` data structure invariants.

use pie_core::{ElemPool, PieList, FibHeap};
use proptest::prelude::*;

// ============================================================================
// PieList properties
// ============================================================================

proptest! {
    /// Push N items, verify length and iteration order match.
    #[test]
    fn list_push_back_preserves_order(values in prop::collection::vec(any::<i32>(), 0..200)) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for &v in &values {
            list.push_back(v, &mut pool).unwrap();
        }
        prop_assert_eq!(list.len(), values.len());
        let collected: Vec<_> = list.iter(&pool).copied().collect();
        prop_assert_eq!(collected, values);
        list.clear(&mut pool);
    }

    /// Push items then drain; pool should be empty afterward.
    #[test]
    fn list_drain_empties_pool(values in prop::collection::vec(any::<i32>(), 1..100)) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for &v in &values {
            list.push_back(v, &mut pool).unwrap();
        }
        let drained: Vec<_> = list.drain(&mut pool).collect();
        prop_assert_eq!(drained, values);
        prop_assert!(list.is_empty());
        prop_assert_eq!(pool.len(), 0);
        list.clear(&mut pool);
    }

    /// Retain keeps exactly the expected elements.
    #[test]
    fn list_retain_correct(values in prop::collection::vec(0..100i32, 0..100)) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for &v in &values {
            list.push_back(v, &mut pool).unwrap();
        }
        let expected: Vec<_> = values.iter().copied().filter(|v| v % 2 == 0).collect();
        list.retain(&mut pool, |v| v % 2 == 0);
        let result: Vec<_> = list.iter(&pool).copied().collect();
        prop_assert_eq!(result, expected);
        list.clear(&mut pool);
    }

    /// Sort produces a non-decreasing sequence.
    #[test]
    fn list_sort_produces_sorted_output(values in prop::collection::vec(any::<i32>(), 0..200)) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for &v in &values {
            list.push_back(v, &mut pool).unwrap();
        }
        list.sort(&mut pool, i32::cmp);
        let sorted: Vec<_> = list.iter(&pool).copied().collect();
        prop_assert_eq!(sorted.len(), values.len());
        for w in sorted.windows(2) {
            prop_assert!(w[0] <= w[1], "not sorted: {} > {}", w[0], w[1]);
        }
        list.clear(&mut pool);
    }

    /// DoubleEndedIterator yields same elements as forward iterator, reversed.
    #[test]
    fn list_iter_rev_matches_forward(values in prop::collection::vec(any::<i32>(), 0..100)) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for &v in &values {
            list.push_back(v, &mut pool).unwrap();
        }
        let forward: Vec<_> = list.iter(&pool).copied().collect();
        let mut reverse: Vec<_> = list.iter(&pool).rev().copied().collect();
        reverse.reverse();
        prop_assert_eq!(forward, reverse);
        list.clear(&mut pool);
    }
}

// ============================================================================
// FibHeap properties
// ============================================================================

proptest! {
    /// Pop always returns the minimum element.
    #[test]
    fn heap_pop_returns_sorted(values in prop::collection::vec(any::<i32>(), 1..200)) {
        let mut heap = FibHeap::new();
        for &v in &values {
            heap.push(v, ());
        }
        let mut sorted = values.clone();
        sorted.sort();
        for expected in sorted {
            let (got, _) = heap.pop().unwrap();
            prop_assert_eq!(got, expected);
        }
        prop_assert!(heap.is_empty());
    }

    /// decrease_key followed by pop respects new ordering.
    #[test]
    fn heap_decrease_key_ordering(
        n in 2..50usize,
        decrease_idx in 0..50usize,
        decrease_amount in 1..1000i32,
    ) {
        let n = n;
        let decrease_idx = decrease_idx % n;
        let mut heap = FibHeap::new();
        let mut handles = Vec::new();
        // Push values 100..100+n so decrease is always valid.
        for i in 0..n {
            let h = heap.push(100 + i as i32, i);
            handles.push((h, 100 + i as i32));
        }
        let (handle, old_key) = handles[decrease_idx];
        let new_key = old_key - decrease_amount;
        heap.decrease_key(handle, new_key).unwrap();
        // Pop all and verify sorted order.
        let mut prev = i32::MIN;
        while let Some((k, _)) = heap.pop() {
            prop_assert!(k >= prev, "not sorted: {} < {}", k, prev);
            prev = k;
        }
    }

    /// try_push returns Ok for reasonable sizes.
    #[test]
    fn heap_try_push_succeeds(n in 0..200usize) {
        let mut heap = FibHeap::new();
        for i in 0..n {
            prop_assert!(heap.try_push(i as i32, ()).is_ok());
        }
        prop_assert_eq!(heap.len(), n);
        heap.clear();
    }
}

// ============================================================================
// Pool reuse properties
// ============================================================================

proptest! {
    /// After clearing a list, the pool's freed slots are reused by new allocations.
    #[test]
    fn pool_reuses_freed_slots(n in 1..100usize) {
        let mut pool = ElemPool::new();
        let mut list = PieList::new(&mut pool);
        for i in 0..n {
            list.push_back(i as i32, &mut pool).unwrap();
        }
        list.clear(&mut pool);
        prop_assert_eq!(pool.len(), 0);
        prop_assert_eq!(pool.free_len(), n);
        // Reallocate the same number of elements — should reuse free slots.
        let mut list2 = PieList::new(&mut pool);
        for i in 0..n {
            list2.push_back(i as i32, &mut pool).unwrap();
        }
        // No new capacity was needed (sentinels use 1 extra slot each).
        prop_assert_eq!(pool.len(), n);
        list2.clear(&mut pool);
    }
}
