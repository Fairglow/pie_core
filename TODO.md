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

- [ ] Remove dead code: `Elem::new_self_ref_raw`, `Elem::force_sentinel`, `ElemPool::get_elem`, and `ElemPool::elem` are unused. Either remove them or move behind `#[cfg(test)]` if only needed for tests. `ElemPool::free_sentinel_index` is only used in one test and should be `#[cfg(test)]`.
- [ ] Replace bare `.unwrap()` calls in non-test code with `.expect("descriptive invariant")` per repo guidelines (e.g., in `sort()`, `splice()`, `pop()`, `consolidate()`).
- [ ] Document the `shallow_copy()` pattern more prominently: it creates aliased list handles to work around borrow checker limitations in `FibHeap`. The copies must not outlive the operation and must not perform conflicting mutations. Consider whether field-level extraction could replace some uses.
- [ ] `FibHeap::clear()` discards the old pool's capacity entirely by creating a new `ElemPool`. For heaps that will be reused at similar sizes, this forces re-growing. Consider clearing in-place if capacity reuse matters.
- [ ] `cascading_cut` in `heap.rs` is recursive. Fibonacci heap theory bounds depth to O(log n), but an iterative rewrite would eliminate any stack overflow risk for adversarial inputs.
- [ ] Fix the size comment in `index.rs` (line 5): claims "16 bytes on 64-bit, 12 bytes on 32-bit" but `Index<T>` is 8 bytes for most `T` types (two `u32` fields + zero-sized `PhantomData`). The ARCHITECTURE.md correctly states "8-byte Size".

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
