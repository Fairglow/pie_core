//! The PieList type, a handle to a high-performance list with embedded data.

use crate::{
    index::Index,
    elem::ListElem,
    pool::{ElemPool, IndexError},
    // cursor::CursorMut,
};
use std::{fmt, marker::PhantomData};

/// An error type for fallible `PieList` operations.
#[derive(Debug, PartialEq, Eq)]
pub enum ListError {
    Index(IndexError),
    CannotOperateOnSentinel,
}

// ... (From, Error, Display impls for ListError) ...

/// A handle to a doubly-linked list with embedded data.
#[derive(Clone)]
pub struct PieList<T> {
    pub(crate) sentinel: Index<T>,
    pub(crate) len: usize,
    _marker: PhantomData<T>,
}

impl<T> PieList<T> {
    pub fn new(pool: &mut ElemPool<T>) -> Self {
        Self {
            sentinel: pool.index_new(),
            len: 0,
            _marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push_back(&mut self, value: T, pool: &mut ElemPool<T>) -> ElemIndex {
        let elem_ndx = pool.index_new();
        let tail_ndx = pool.get(self.sentinel).prev;
        pool.index_link_after(elem_ndx, tail_ndx);
        pool.get_mut(elem_ndx).data = Some(value);
        self.len += 1;
        elem_ndx
    }

    pub fn pop_back(&mut self, pool: &mut ElemPool<T>) -> Option<T> {
        if self.is_empty() { return None; }
        let tail_ndx = pool.get(self.sentinel).prev;
        let data = self.remove(tail_ndx, pool);
        Some(data)
    }

    pub fn remove(&mut self, target: ElemIndex, pool: &mut ElemPool<T>) -> T {
        assert_ne!(target, self.sentinel, "Cannot remove sentinel");
        let data = pool.get_mut(target).data.take().expect("Cannot remove an already-free element");
        pool.index_del(target);
        self.len -= 1;
        data
    }

    // ... (push_front, pop_front, insert_after, append, etc. all simplified) ...

    pub fn cursor_mut<'a>(&'a mut self, pool: &'a mut ElemPool<T>) -> CursorMut<'a, T> {
        let head = pool.get(self.sentinel).next;
        CursorMut::new(self, pool, head, 0)
    }
}
