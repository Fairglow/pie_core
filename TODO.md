# To Do List for `pie_core`

These are suggestions for improvements:

## Completed

- [x] pool.shrink_to_fit(): An advanced method to reclaim unused memory from the end of the pool's vector.
- [x] Serialization support via optional `serde` feature.
- [x] `no_std` support: Enable usage in embedded contexts (via `default-features = false`).
- [x] Add `PieView<'a, T>` to bundle a PieList with its ElemPool for common traits.
- [x] Add `PieViewMut<'a, T>` for mutable operations.
- [x] "View" Pattern: Implement `list.view(&pool)` that implements Debug, IntoIterator, PartialEq, and Ord.
- [x] Documentation: Explain why standard traits (like Drop) are missing to manage user expectations.
- [x] `ExactSizeIterator` implementation for `Iter`, `IterMut`, and `Drain`.
- [x] Optimize `sort()` to use bottom-up iterative merge sort with O(log n) sentinel reuse.
- [x] Add `#[inline]` annotations to hot-path accessor methods (`front`, `back`, `push_*`, `pop_*`, cursor `peek`/`move_*`).
- [x] Optimize `shrink_to_fit()` with O(1) slot-indexed remapping array instead of hash map lookups.
- [x] Add `FusedIterator` implementation for iterators (all four iterators: `Iter`, `IterMut`, `PieList::Drain`, `FibHeap::Drain`).
- [x] Document performance characteristics with benchmark results in README (see benchmark summary table and BENCHMARKS.md).

## Pending

### Code Quality

- [x] Remove dead code: `Elem::new_self_ref_raw`, `Elem::force_sentinel`, `ElemPool::get_elem`, and `ElemPool::elem` removed. `ElemPool::free_sentinel_index` gated behind `#[cfg(test)]`.
- [x] Replace bare `.unwrap()` calls in non-test code with `.expect("descriptive invariant")` for pool operations (data, index_del, index_linkout, index_link_after, etc.). `Slot::unwrap()` retained as it already uses `.expect()` internally.
- [x] Document the `shallow_copy()` pattern more prominently with explicit invariants for aliased handle usage in `FibHeap`.
- [x] `FibHeap::clear()` now clears the pool in-place via `ElemPool::reset()`, preserving allocated capacity.
- [x] `cascading_cut` in `heap.rs` rewritten as an iterative loop.
- [x] Fix the size comment in `index.rs`: corrected to "8 bytes — PhantomData<T> is zero-sized".

### Testing

- [ ] Add more comparative tests against `index_list`.
- [ ] Add property-based testing with `proptest` or `quickcheck` for complex invariants.
- [ ] Add tests for edge cases: maximum pool size, generation counter overflow.
- [ ] Add tests for `Extend` trait on `PieViewMut` (implemented but untested).
- [ ] Add tests for `cursor_at` / `cursor_mut_at` out-of-bounds error paths.
- [ ] Add tests for iterator `size_hint` accuracy and `DoubleEndedIterator` in unit tests (currently only tested indirectly through views).
- [ ] Add test for `FibHeap::Default` implementation.

### Benchmarks

- [ ] Add `drain` benchmark for both `PieList` and `FibHeap`.
- [ ] Add `shrink_to_fit` benchmark (key operation, not benchmarked).
- [ ] Add cursor traversal benchmark (cursors are a major API feature).
- [ ] Add `split_off` benchmark (only splice is benchmarked, not split).

### Documentation

- [ ] Add a `CHANGELOG.md` for version history tracking.
- [ ] Add usage examples in README for the `petgraph` feature (Dijkstra example).
- [ ] Soften the "AI Assessment" in README: "professional-grade, feature-complete" overstates the current state given pending TODO items, missing property-based tests, and no CHANGELOG. Consider "well-crafted" and "feature-rich" instead.

### API Enhancements

- [x] Add `try_push` to `FibHeap` that returns `Result` instead of panicking on OOM (PieList already returns `Result`). `push` now delegates to `try_push`.
- [x] Include diagnostic information (slot/generation) in `DecreaseKeyError::InvalidHandle`.
- [x] Add `retain()` method to `PieList` for filtering elements in-place.
