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

- [x] Add more comparative tests against `index_list`. Added `tests/index_list_compare.rs` with 8 comparative tests covering push_back, push_front, pop_front, pop_back, mixed ops, length tracking, front/back access, and iteration order.
- [x] Add property-based testing with `proptest` or `quickcheck` for complex invariants. Added `proptest` dev-dependency and `tests/proptest_invariants.rs` with 9 property-based tests covering PieList (order, drain, retain, sort, reverse iteration), FibHeap (pop ordering, decrease_key, try_push), and pool slot reuse.
- [x] Add tests for edge cases: maximum pool size, generation counter overflow. Added generation overflow wrapping/cycle tests in `generation.rs` and pool reset capacity/empty tests in `pool.rs`.
- [x] Add tests for `Extend` trait on `PieViewMut` (implemented but untested). Added 4 tests in `view_mut.rs`: extend from vec, range, appending to existing list, and empty iterator.
- [x] Add tests for `cursor_at` / `cursor_mut_at` out-of-bounds error paths. Added 4 tests in `list.rs` covering both cursor types with one-past-end and far-out-of-bounds indices.
- [x] Add tests for iterator `size_hint` accuracy and `DoubleEndedIterator` in unit tests. Added `size_hint()` implementations to `Iter`, `IterMut`, and `Drain` (was missing — tests exposed the bug), plus 8 tests for size_hint, DoubleEndedIterator, and FusedIterator. Also added `FibHeap::Drain` size_hint and fused tests.
- [x] Add test for `FibHeap::Default` implementation. Added `test_default` and `test_default_then_use` in `heap.rs`. Also added `test_clear_then_reuse` verifying heap correctness after clear/repopulate cycle.

### Benchmarks

- [x] Add `drain` benchmark for both `PieList` and `FibHeap`. Added `list/drain` (pielist vs vec vs vecdeque) and `heap/drain` (piefibheap vs binaryheap) groups.
- [x] Add `shrink_to_fit` benchmark (key operation, not benchmarked). Added `pool/shrink_to_fit` group measuring compaction after 50% deletion via `retain`.
- [x] Add cursor traversal benchmark (cursors are a major API feature). Added `list/cursor_traverse` group comparing immutable cursor, mutable cursor, and iterator baseline.
- [x] Add `split_off` benchmark (only splice is benchmarked, not split). Added `list/split` group comparing `cursor.split_before()` (O(n/2) seek + O(1) split) vs `Vec::split_off`.

### Documentation

- [x] Add a `CHANGELOG.md` for version history tracking. Created with entries for all tagged versions (v0.2.0 through v0.2.14).
- [x] Add usage examples in README for the `petgraph` feature (Dijkstra example). Added a `Cargo.toml` snippet and a short code example showing `dijkstra_pie_core`, plus a link to the full `examples/dijkstra/` example.
- [x] Revise the "AI Assessment" in README to be accurate and evidence-based. Replaced subjective language with specific, verifiable claims backed by benchmarks and test counts.

### API Enhancements

- [x] Add `try_push` to `FibHeap` that returns `Result` instead of panicking on OOM (PieList already returns `Result`). `push` now delegates to `try_push`.
- [x] Include diagnostic information (slot/generation) in `DecreaseKeyError::InvalidHandle`.
- [x] Add `retain()` method to `PieList` for filtering elements in-place.
