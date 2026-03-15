# Changelog

All notable changes to `pie_core` are documented in this file.

## [0.2.14] — Unreleased

### Added
- `FibHeap::try_push()`: fallible push that returns `Result` instead of panicking on allocation failure. `push()` now delegates to `try_push()`.
- `PieList::retain()`: filter elements in-place, removing those for which the predicate returns `false`.
- Diagnostic information (slot/generation) in `DecreaseKeyError::InvalidHandle`.
- `size_hint()` on `PieList::Iter`, `IterMut`, and `Drain` (was missing despite `ExactSizeIterator`).
- `ElemPool::reset()`: clear the pool in-place, preserving allocated capacity.
- Property-based tests via `proptest` (9 tests covering list, heap, and pool invariants).
- Comparative tests against `index_list` (8 tests in `tests/index_list_compare.rs`).
- Benchmarks for `drain`, `shrink_to_fit`, cursor traversal, and `split_before`.
- `CHANGELOG.md` for version history tracking.

### Changed
- `FibHeap::clear()` now uses `ElemPool::reset()` to preserve allocated capacity.
- `cascading_cut` in `heap.rs` rewritten from recursive to iterative.
- Replaced bare `.unwrap()` in non-test code with `.expect("descriptive invariant")`.

### Removed
- Dead code: `Elem::new_self_ref_raw`, `Elem::force_sentinel`, `ElemPool::get_elem`, `ElemPool::elem`.
- `ElemPool::free_sentinel_index` moved behind `#[cfg(test)]`.

## [0.2.13]

### Changed
- Bug fixes and improved test coverage.
- Hardening, correctness, and performance improvements.
- Upgraded dependencies to latest versions.

## [0.2.12]

### Changed
- Improved benchmarking and cleanups.

## [0.2.11]

### Added
- Generational indices for the pool (`Index<T>` with ABA-problem protection).
- Internal documentation.

### Changed
- Performance improvements across pool operations.

## [0.2.10]

### Added
- `PieViewMut<'a, T>` for mutable list operations via a bundled view.
- `Cursor<'a, T>` and `CursorMut<'a, T>` for fine-grained list navigation and mutation.

## [0.2.9]

### Fixed
- Documentation example code fixes (lists cleared properly in examples).

## [0.2.8]

### Added
- Memory leak detection: dropping a non-empty `PieList` panics in debug builds.
- `text_editor.rs` example demonstrating `PieList` as a text editor backend.

## [0.2.7]

### Added
- `PieView<'a, T>` for a simplified, read-only API that bundles list and pool.
- `no_std` support (via `default-features = false`).

## [0.2.6]

### Added
- `serde` feature for serialization/deserialization of `ElemPool`, `PieList`, and `FibHeap`.
- `ElemPool::shrink_to_fit()` and `FibHeap::shrink_to_fit()` for memory compaction.
- Dijkstra's algorithm example with `petgraph` integration.
- Dijkstra dense/sparse benchmarks.

## [0.2.3]

### Added
- Real-world integration tests (pathfinding, text editor).
- Nightly-only benchmarks for `LinkedList` comparison.

### Fixed
- CI workflow permissions.

## [0.2.2]

### Added
- `PieList::append()` and `PieList::prepend()` for list concatenation.
- `ElemPool::reserve()` for pre-allocating capacity.
- `DecreaseKeyError` enum (replacing panics in `decrease_key`).

## [0.2.1]

### Added
- `PieList::drain()` and `FibHeap::drain()` iterators.
- `ElemPool::free_len()` to query freed element count.
- `justfile` for common build/test/bench commands.

## [0.2.0]

Initial public release with `ElemPool`, `PieList`, and `FibHeap`.
