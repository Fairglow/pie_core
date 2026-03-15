//! Comparative tests between `pie_core::PieList` and `index_list::IndexList`.
//!
//! Verifies that both data structures produce equivalent results for
//! the same sequence of operations.

use index_list::IndexList;
use pie_core::{ElemPool, PieList};

/// Helper: collect PieList into Vec.
fn pie_to_vec<T: Clone>(list: &PieList<T>, pool: &ElemPool<T>) -> Vec<T> {
    list.iter(pool).cloned().collect()
}

/// Helper: collect IndexList into Vec.
fn idx_to_vec<T: Clone>(list: &IndexList<T>) -> Vec<T> {
    list.iter().cloned().collect()
}

#[test]
fn push_back_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    for v in 0..20 {
        pie.push_back(v, &mut pool).unwrap();
        idx.insert_last(v);
    }
    assert_eq!(pie_to_vec(&pie, &pool), idx_to_vec(&idx));
    pie.clear(&mut pool);
}

#[test]
fn push_front_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    for v in 0..20 {
        pie.push_front(v, &mut pool).unwrap();
        idx.insert_first(v);
    }
    assert_eq!(pie_to_vec(&pie, &pool), idx_to_vec(&idx));
    pie.clear(&mut pool);
}

#[test]
fn pop_front_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    for v in 0..10 {
        pie.push_back(v, &mut pool).unwrap();
        idx.insert_last(v);
    }
    for _ in 0..5 {
        let pie_val = pie.pop_front(&mut pool);
        let idx_val = idx.remove_first();
        assert_eq!(pie_val, idx_val);
    }
    assert_eq!(pie_to_vec(&pie, &pool), idx_to_vec(&idx));
    pie.clear(&mut pool);
}

#[test]
fn pop_back_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    for v in 0..10 {
        pie.push_back(v, &mut pool).unwrap();
        idx.insert_last(v);
    }
    for _ in 0..5 {
        let pie_val = pie.pop_back(&mut pool);
        let idx_val = idx.remove_last();
        assert_eq!(pie_val, idx_val);
    }
    assert_eq!(pie_to_vec(&pie, &pool), idx_to_vec(&idx));
    pie.clear(&mut pool);
}

#[test]
fn mixed_push_pop_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    // Interleave pushes and pops.
    for v in 0..30 {
        if v % 3 == 0 {
            pie.push_front(v, &mut pool).unwrap();
            idx.insert_first(v);
        } else {
            pie.push_back(v, &mut pool).unwrap();
            idx.insert_last(v);
        }
        if v % 5 == 0 && pie.len() > 0 {
            pie.pop_front(&mut pool);
            idx.remove_first();
        }
    }
    assert_eq!(pie_to_vec(&pie, &pool), idx_to_vec(&idx));
    assert_eq!(pie.len(), idx.len());
    pie.clear(&mut pool);
}

#[test]
fn length_tracking_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    assert_eq!(pie.len(), idx.len());
    assert_eq!(pie.is_empty(), idx.is_empty());
    for v in 0..15 {
        pie.push_back(v, &mut pool).unwrap();
        idx.insert_last(v);
        assert_eq!(pie.len(), idx.len());
    }
    for _ in 0..10 {
        pie.pop_front(&mut pool);
        idx.remove_first();
        assert_eq!(pie.len(), idx.len());
    }
    pie.clear(&mut pool);
}

#[test]
fn front_back_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    // Both empty.
    assert_eq!(pie.front(&pool), None);
    assert_eq!(idx.get_first(), None);
    assert_eq!(pie.back(&pool), None);
    assert_eq!(idx.get_last(), None);
    // Add elements.
    for v in [10, 20, 30] {
        pie.push_back(v, &mut pool).unwrap();
        idx.insert_last(v);
    }
    assert_eq!(pie.front(&pool), idx.get_first());
    assert_eq!(pie.back(&pool), idx.get_last());
    pie.clear(&mut pool);
}

#[test]
fn iteration_order_equivalence() {
    let mut pool = ElemPool::new();
    let mut pie = PieList::new(&mut pool);
    let mut idx = IndexList::new();
    let values = [42, 17, 99, 3, 55, 81, 7];
    for &v in &values {
        pie.push_back(v, &mut pool).unwrap();
        idx.insert_last(v);
    }
    // Forward iteration.
    let pie_fwd: Vec<_> = pie.iter(&pool).copied().collect();
    let idx_fwd: Vec<_> = idx.iter().copied().collect();
    assert_eq!(pie_fwd, idx_fwd);
    assert_eq!(pie_fwd, values.to_vec());
    // Reverse iteration.
    let pie_rev: Vec<_> = pie.iter(&pool).rev().copied().collect();
    let idx_rev: Vec<_> = idx.iter().rev().copied().collect();
    assert_eq!(pie_rev, idx_rev);
    pie.clear(&mut pool);
}
