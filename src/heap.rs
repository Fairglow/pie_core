//! A Fibonacci heap implementation, built on the `ElemPool` and `PieList`.

use crate::{ElemPool, Index, PieList};
use crate::IndexMap;
use core::{error, fmt, mem};
use alloc::{format, string::ToString, vec, vec::Vec};
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// An error returned from `FibHeap::decrease_key`.
#[derive(Debug, PartialEq, Eq)]
pub enum DecreaseKeyError {
    /// The provided handle was invalid (e.g., NONE, out of bounds, or pointed to a free node).
    InvalidHandle,
    /// The new key is greater than current
    NewKeyGreaterThanCurrent,
}

impl error::Error for DecreaseKeyError {}

impl fmt::Display for DecreaseKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandle => write!(f, "Invalid handle provided to decrease_key"),
            Self::NewKeyGreaterThanCurrent => write!(f, "Key greater than current provided"),
        }
    }
}

/// An opaque struct representing a node within the `FibHeap`.
///
/// Users of the heap cannot interact with this struct directly, but it is
/// made public to allow the `FibHandle` type alias to be public as well.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Node<K, V> {
    key: K,
    value: V,
    /// Handle to this node's parent. `NONE` if this is a root.
    parent: FibHandle<K, V>,
    /// A handle to this node's list of children.
    children: PieList<Node<K, V>>,
    /// The number of children in the `children` list.
    degree: usize,
    /// `true` if this node has lost a child since it became
    /// a child of its current parent.
    marked: bool,
}

/// A type-safe, `Copy`-able handle to a node in the `FibHeap`.
///
/// This handle is returned by [`FibHeap::push`] and is required for the
/// [`FibHeap::decrease_key`] operation. The handle remains valid as long
/// as the node it points to has not been popped from the heap.
pub type FibHandle<K, V> = Index<Node<K, V>>;

/// A Fibonacci heap, designed for efficient priority queue operations.
///
/// This heap is implemented on top of a `pie_core::ElemPool`, which avoids
/// per-node allocations and provides high performance. It is a min-heap,
/// meaning that `pop` will always return the element with the smallest key.
///
/// # Type Parameters
///
/// - `K`: The key type, which determines the priority of an element. Must implement `Ord`.
/// - `V`: The value type, which is the data stored in the heap.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FibHeap<K, V> {
    /// The pool allocator that stores all nodes for this heap.
    pool: ElemPool<Node<K, V>>,
    /// A handle to the doubly-linked list of root nodes.
    roots: PieList<Node<K, V>>,
    /// A handle to the node with the minimum key.
    min: FibHandle<K, V>,
    /// The total number of nodes in the heap.
    len: usize,
}

impl<K, V> FibHeap<K, V> {
    /// Creates a new, empty `FibHeap`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pie_core::FibHeap;
    /// let mut heap = FibHeap::<u32, &str>::new();
    /// assert!(heap.is_empty());
    /// ```
    pub fn new() -> Self {
        let mut pool = ElemPool::new();
        // The root list needs its own sentinel, allocated from the pool.
        let roots = PieList::new(&mut pool).without_leak_check();
        Self { pool, roots, min: FibHandle::NONE, len: 0, }
    }

    /// Returns the number of elements in the heap.
    ///
    /// # Complexity
    ///
    /// O(1)
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the heap contains no elements.
    ///
    /// # Complexity
    ///
    /// O(1)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the number of nodes the heap can hold without reallocating its internal storage.
    ///
    /// This is the capacity of the underlying `pie_core::ElemPool`.
    ///
    /// # Complexity
    ///
    /// O(1)
    pub fn pool_capacity(&self) -> usize {
        self.pool.capacity()
    }

    /// Removes all elements from the heap.
    ///
    /// This is an O(n) operation, as it must deallocate all nodes
    /// within its internal pool.
    pub fn clear(&mut self) {
        let mut new_pool = ElemPool::new();
        self.roots = PieList::new(&mut new_pool).without_leak_check();
        self.pool = new_pool;
        self.min = FibHandle::NONE;
        self.len = 0;
    }
}

impl<K: Ord, V> FibHeap<K, V> {
    /// Pushes a new key-value pair onto the heap.
    ///
    /// Returns a `pie_core::FibHandle` which can be used with `decrease_key`.
    ///
    /// # Complexity
    ///
    /// O(1) amortized time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pie_core::FibHeap;
    /// let mut heap = FibHeap::new();
    /// let handle = heap.push(10, "ten");
    /// assert_eq!(heap.len(), 1);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the internal `ElemPool` fails to allocate a new node, which
    /// typically only happens in an out-of-memory situation.
    pub fn push(&mut self, key: K, value: V) -> FibHandle<K, V> {
        // Each node needs its *own* child list, which means
        // allocating a new sentinel for it.
        let children = PieList::new(&mut self.pool).without_leak_check();
        let node = Node {
            key,
            value,
            parent: FibHandle::NONE,
            children,
            degree: 0,
            marked: false,
        };
        // `push_front` creates the `ListElem` in the pool,
        // stores our `node` data inside it, and links it.
        // We panic on OOM, which is standard for collections.
        self.roots
            .push_front(node, &mut self.pool)
            .expect("Failed to allocate node");
        // `push_front` adds the node as the *first* element after the sentinel.
        let handle = self.pool.next(self.roots.sentinel);
        self.update_min(handle);
        self.len += 1;
        handle
    }

    /// Helper to update the `min` pointer.
    fn update_min(&mut self, handle: FibHandle<K, V>) {
        if self.min.is_none() {
            self.min = handle;
        } else {
            // These unwraps are safe because `min` and `handle`
            // are guaranteed to be valid, non-sentinel nodes.
            let min_key = &self.pool.data(self.min).unwrap().key;
            let new_key = &self.pool.data(handle).unwrap().key;
            if new_key < min_key {
                self.min = handle;
            }
        }
    }

    /// Returns a reference to the element with the smallest key, without removing it.
    ///
    /// Returns `None` if the heap is empty.
    ///
    /// # Complexity
    ///
    /// O(1) time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pie_core::FibHeap;
    /// let mut heap = FibHeap::new();
    /// heap.push(5, 'a');
    /// heap.push(3, 'b');
    ///
    /// assert_eq!(heap.peek(), Some((&3, &'b')));
    /// ```
    pub fn peek(&self) -> Option<(&K, &V)> {
        // `self.pool.data` safely handles `self.min` being `NONE`.
        self.pool.data(self.min).map(|node| (&node.key, &node.value))
    }

    /// Removes and returns the element with the smallest key (highest priority).
    ///
    /// Returns `None` if the heap is empty.
    ///
    /// # Complexity
    ///
    /// O(log n) amortized time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pie_core::FibHeap;
    /// let mut heap = FibHeap::new();
    /// heap.push(5, 'a');
    /// heap.push(3, 'b');
    ///
    /// assert_eq!(heap.pop(), Some((3, 'b')));
    /// assert_eq!(heap.pop(), Some((5, 'a')));
    /// assert_eq!(heap.pop(), None);
    /// ```
    pub fn pop(&mut self) -> Option<(K, V)> {
        if self.min.is_none() {
            return None;
        }
        let min_handle = self.min;
        // 1. Unlink the min node from the root list.
        self.pool.index_linkout(min_handle).unwrap();
        self.roots.len -= 1;
        // 2. Take the node data out of the pool.
        let min_node_data = self.pool.data_swap(min_handle, None).unwrap();
        self.len -= 1;
        // 3. Move the min node's children to the root list.
        let num_children = min_node_data.children.len;
        if num_children > 0 {
            let first_child = self.pool.next(min_node_data.children.sentinel);
            let last_child = self.pool.prev(min_node_data.children.sentinel);
            let root_last = self.pool.prev(self.roots.sentinel);
            // Splice children into the root list.
            self.pool.get_mut(root_last).unwrap().new_next(first_child);
            self.pool.get_mut(first_child).unwrap().new_prev(root_last);
            self.pool.get_mut(last_child).unwrap().new_next(self.roots.sentinel);
            self.pool.get_mut(self.roots.sentinel).unwrap().new_prev(last_child);
            self.roots.len += num_children;
            // Un-parent all moved children.
            let mut current = first_child;
            for _ in 0..num_children {
                self.pool.data_mut(current).unwrap().parent = FibHandle::NONE;
                current = self.pool.next(current);
            }
        }
        // 5. Return memory to the pool.
        self.pool
            .index_del(min_node_data.children.sentinel)
            .unwrap();
        self.pool.index_del(min_handle).unwrap();
        // 6. Consolidate the root list.
        if self.roots.is_empty() {
            self.min = FibHandle::NONE;
        } else {
            self.consolidate();
        }
        Some((min_node_data.key, min_node_data.value))
    }

    /// Consolidates the root list by linking trees of the same degree.
    fn consolidate(&mut self) {
        // Max degree is ~log_phi(n). 64 is safe for n up to 2^64.
        let mut a: Vec<Option<FibHandle<K, V>>> = vec![None; 64];
        self.min = FibHandle::NONE;
        let mut handles = Vec::with_capacity(self.roots.len());
        let mut current = self.pool.next(self.roots.sentinel);
        while current != self.roots.sentinel {
            handles.push(current);
            current = self.pool.next(current);
        }
        self.pool.get_mut(self.roots.sentinel).unwrap()
            .new_links(self.roots.sentinel, self.roots.sentinel);
        self.roots.len = 0;
        for &handle in &handles {
            let mut x = handle;
            let mut d = self.pool.data(x).unwrap().degree;
            while let Some(mut y) = a[d] {
                if self.pool.data(x).unwrap().key > self.pool.data(y).unwrap().key {
                    mem::swap(&mut x, &mut y);
                }
                self.heap_link(y, x);
                a[d] = None;
                d += 1;
            }
            a[d] = Some(x);
        }
        for handle in a.iter().flatten() {
            self.pool.index_link_after(*handle, self.roots.sentinel).unwrap();
            self.roots.len += 1;
            self.update_min(*handle);
        }
    }

    /// Links node `y` as a child of node `x`.
    fn heap_link(&mut self, y: FibHandle<K, V>, x: FibHandle<K, V>) {
        let mut x_children = self.pool.data_mut(x).unwrap().children.clone();
        self.pool.index_link_after(y, x_children.sentinel).unwrap();
        x_children.len += 1;
        self.pool.data_mut(x).unwrap().children = x_children;
        self.pool.data_mut(x).unwrap().degree += 1;
        self.pool.data_mut(y).unwrap().parent = x;
        self.pool.data_mut(y).unwrap().marked = false;
    }

    /// Decreases the key of a node in the heap.
    ///
    /// This is a key feature of Fibonacci heaps, allowing for efficient updates
    /// to priorities. The handle must be one that was returned by a previous
    /// call to `push`.
    ///
    /// # Decrease only
    ///
    /// The design is optimized for decreasing the key at the expense of the
    /// efficiency of increasing them. Therefore, use-case where keys increase
    /// are not suitable for this implementation.
    ///
    /// # Complexity
    ///
    /// O(1) amortized time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pie_core::{FibHeap, FibHandle};
    /// let mut heap = FibHeap::new();
    /// heap.push(10, "high priority");
    /// let handle = heap.push(100, "low priority");
    ///
    /// assert_eq!(heap.peek().unwrap().0, &10);
    ///
    /// // Decrease the key of the "low priority" item.
    /// heap.decrease_key(handle, 5);
    ///
    /// // It is now the minimum element.
    /// assert_eq!(heap.peek().unwrap().0, &5);
    /// ```
    pub fn decrease_key(&mut self, handle: FibHandle<K, V>, new_key: K
    ) -> Result<(), DecreaseKeyError> {
        let parent = {
            let node = self.pool.data(handle).ok_or(DecreaseKeyError::InvalidHandle)?;
            if new_key > node.key {
                return Err(DecreaseKeyError::NewKeyGreaterThanCurrent);
            }
            node.parent
        };
        {
            // This unwrap is now 100% safe because of the check above.
            let node_mut = self.pool.data_mut(handle).unwrap();
            node_mut.key = new_key;
        }
        if parent.is_some() && self.pool.data(handle).unwrap().key < self.pool.data(parent).unwrap().key {
            self.cut(handle, parent);
            self.cascading_cut(parent);
        }
        self.update_min(handle);
        Ok(())
    }

    /// Cuts node `x` from its parent `y`.
    fn cut(&mut self, x: FibHandle<K, V>, y: FibHandle<K, V>) {
        // 1. Unlink `x` from `y`'s child list.
        self.pool.index_linkout(x).unwrap();
        let mut y_children = self.pool.data_mut(y).unwrap().children.clone();
        y_children.len -= 1;
        self.pool.data_mut(y).unwrap().children = y_children;
        self.pool.data_mut(y).unwrap().degree -= 1;
        // 2. Add `x` to the root list.
        self.pool.index_link_after(x, self.roots.sentinel).unwrap();
        self.roots.len += 1;
        // 3. Update `x`'s parent and mark.
        self.pool.data_mut(x).unwrap().parent = FibHandle::NONE;
        self.pool.data_mut(x).unwrap().marked = false;
    }

    /// Performs a cascading cut on node `y`.
    fn cascading_cut(&mut self, y: FibHandle<K, V>) {
        let y_parent = self.pool.data(y).unwrap().parent;
        if y_parent.is_some() {
            if !self.pool.data(y).unwrap().marked {
                // This is the first child `y` has lost. Mark it.
                self.pool.data_mut(y).unwrap().marked = true;
            } else {
                // `y` has already lost a child, so cut it too.
                self.cut(y, y_parent);
                self.cascading_cut(y_parent);
            }
        }
    }

    /// Compacts the internal pool, reducing memory usage.
    ///
    /// This method shrinks the underlying `Vec` to match the number of live
    /// nodes. Because this operation moves nodes in memory, it changes their
    /// indices.
    ///
    /// # Returns
    /// A `IndexMap` mapping old handles to new handles.
    ///
    /// **CRITICAL:** If you are holding any `FibHandle`s returned by `push`,
    /// you **must** update them using this map.
    ///
    /// ```rust
    /// # use pie_core::FibHeap;
    /// let mut heap = FibHeap::new();
    /// let handle = heap.push(10, "data");
    ///
    /// let map = heap.shrink_to_fit();
    ///
    /// // Safe way to update your handle:
    /// let new_handle = map.get(&handle).copied().unwrap_or(handle);
    /// ```
    pub fn shrink_to_fit(&mut self) -> IndexMap<FibHandle<K, V>, FibHandle<K, V>> {
        // 1. Shrink the pool. This moves nodes and fixes the Pool-level links.
        let map = self.pool.shrink_to_fit();
        // 2. Fix Heap-level pointers (roots and min)
        self.roots.remap(&map);
        if let Some(new_min) = map.get(&self.min) {
            self.min = *new_min;
        }
        // 3. Fix Node-level pointers (parent and children)
        // The pool's shrink method fixes `prev/next` links, but it knows nothing
        // about the `parent` or `children` fields inside our `Node` struct.
        // We must traverse the pool and update them manually.
        for elem in self.pool.iter_mut() {
            if elem.is_used() {
                // SAFETY: We just checked is_used()
                #[allow(unsafe_code)]
                let node = unsafe { elem.data.assume_init_mut() };
                // Fix parent pointer
                if let Some(new_parent) = map.get(&node.parent) {
                    node.parent = *new_parent;
                }
                // Fix children list sentinel pointer
                node.children.remap(&map);
            }
        }
        map
    }

    /// Creates a draining iterator that removes all elements from the heap in
    /// ascending order of their keys and yields their (key, value) pairs.
    ///
    /// The heap will be empty after the iterator has been fully consumed.
    ///
    /// # Note
    ///
    /// If the iterator is dropped before it is fully consumed, it will not
    /// remove the remaining elements. The heap will be left partially drained.
    /// This is because, unlike `PieList::drain`, each call to `next()` is an
    /// O(log n) operation, and automatically clearing the remainder on drop
    /// could be an unexpectedly expensive operation.
    ///
    /// # Complexity
    /// Each call to `next()` on the iterator has the same complexity as
    /// [`pop()`]: O(log n) amortized time. Consuming the entire iterator
    /// is equivalent to sorting the elements, taking O(n log n) time.
    ///
    /// # Example
    /// ```
    /// # use pie_core::FibHeap;
    /// let mut heap = FibHeap::new();
    /// heap.push(10, "ten");
    /// heap.push(5, "five");
    /// heap.push(20, "twenty");
    ///
    /// let drained_items: Vec<_> = heap.drain().collect();
    /// assert_eq!(drained_items, vec![(5, "five"), (10, "ten"), (20, "twenty")]);
    /// assert!(heap.is_empty());
    /// ```
    /// [`pop()`]: FibHeap::pop
    pub fn drain(&mut self) -> Drain<'_, K, V> {
        Drain { heap: self }
    }
}

// Default implementation
impl<K: Ord, V> Default for FibHeap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

// Ensure the heap cleans up its *own* pool on drop.
impl<K, V> Drop for FibHeap<K, V> {
    fn drop(&mut self) {
        // A full `clear` is the safest way to ensure all memory in the
        // pool is properly handled.
        self.clear();
    }
}

impl<K: Ord + fmt::Display, V: fmt::Display> fmt::Display for FibHeap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return writeln!(f, "FibHeap (empty)");
        }
        writeln!(f, "FibHeap (len: {}, min: {})",
            self.len,
            self.pool.data(self.min)
                .map_or_else(|| "N/A".to_string(), |n| n.key.to_string()))?;
        let mut current = self.pool.next(self.roots.sentinel);
        if current == self.roots.sentinel {
            return writeln!(f, "  <no roots>");
        }
        loop {
            let next = self.pool.next(current);
            let is_last = next == self.roots.sentinel;
            self.fmt_node(current, f, "  ", is_last)?;
            if is_last {
                break;
            }
            current = next;
        }
        Ok(())
    }
}

impl<K: Ord + fmt::Display, V: fmt::Display> FibHeap<K, V> {
    /// Recursive helper to format a single node and its descendants.
    fn fmt_node(
        &self,
        handle: FibHandle<K, V>,
        f: &mut fmt::Formatter<'_>,
        prefix: &str,
        is_last: bool,
    ) -> fmt::Result {
        // This unwrap is safe within this context, as we only ever call this
        // with valid handles from traversing the heap structure.
        let node = self.pool.data(handle).unwrap();
        // Print the current node's line
        let connector = if is_last { "└─" } else { "├─" };
        let marked_str = if node.marked { " (M)" } else { "" };
        writeln!(f, "{}{} Node(k: {}, v: {}){}",
            prefix, connector, node.key, node.value, marked_str)?;
        // Prepare the prefix for children
        let child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
        // Recursively print children
        let children_list = node.children.clone();
        if !children_list.is_empty() {
            let mut current_child = self.pool.next(children_list.sentinel);
            while current_child != children_list.sentinel {
                let next_child = self.pool.next(current_child);
                let is_last_child = next_child == children_list.sentinel;
                self.fmt_node(current_child, f, &child_prefix, is_last_child)?;
                current_child = next_child;
            }
        }
        Ok(())
    }
}

/// A draining iterator for a `FibHeap`.
///
/// This struct is created by the [`drain()`] method on [`FibHeap`].
/// See its documentation for more.
///
/// [`drain()`]: FibHeap::drain
pub struct Drain<'a, K: Ord, V> {
    heap: &'a mut FibHeap<K, V>,
}

impl<'a, K: Ord, V> Iterator for Drain<'a, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.heap.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_new_empty_len() {
        let heap = FibHeap::<i32, ()>::new();
        assert!(heap.is_empty());
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.peek(), None);
    }

    #[test]
    fn test_push_and_peek() {
        let mut heap = FibHeap::new();
        heap.push(10, 'a');
        assert!(!heap.is_empty());
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.peek(), Some((&10, &'a')));

        heap.push(5, 'b');
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.peek(), Some((&5, &'b')));

        heap.push(20, 'c');
        assert_eq!(heap.len(), 3);
        assert_eq!(heap.peek(), Some((&5, &'b')));
    }

    #[test]
    fn test_pop() {
        let mut heap = FibHeap::new();
        heap.push(10, "ten");
        heap.push(20, "twenty");
        heap.push(5, "five");
        heap.push(15, "fifteen");

        assert_eq!(heap.len(), 4);
        assert_eq!(heap.pop(), Some((5, "five")));
        assert_eq!(heap.len(), 3);
        assert_eq!(heap.pop(), Some((10, "ten")));
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.pop(), Some((15, "fifteen")));
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.pop(), Some((20, "twenty")));
        assert_eq!(heap.len(), 0);
        assert!(heap.is_empty());
        assert_eq!(heap.pop(), None);
    }

    #[test]
    fn test_pop_empty() {
        let mut heap = FibHeap::<i32, i32>::new();
        assert_eq!(heap.pop(), None);
    }

    #[test]
    fn test_clear() {
        let mut heap = FibHeap::new();
        heap.push(10, ());
        heap.push(5, ());
        assert!(!heap.is_empty());
        assert_eq!(heap.len(), 2);

        heap.clear();
        assert!(heap.is_empty());
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.peek(), None);
        assert_eq!(heap.pop(), None);
    }

    #[test]
    fn test_drop() {
        let mut heap = FibHeap::new();
        heap.push(10, 'a');
        heap.push(5, 'b');
        // Drop is called implicitly when heap goes out of scope.
        // This test just ensures it doesn't panic.
    }

    #[test]
    fn test_consolidation() {
        let mut heap = FibHeap::new();
        // Push 8 items, creating 8 root nodes.
        for i in (0..8).rev() {
            heap.push(i, ());
        }
        assert_eq!(heap.roots.len, 8);
        assert_eq!(heap.peek().unwrap().0, &0);

        // Pop the minimum. This will trigger a consolidation.
        assert_eq!(heap.pop(), Some((0, ())));
        assert_eq!(heap.len(), 7);
        // The exact number of roots depends on the consolidation link order,
        // but it must be less than the original 7.
        assert!(heap.roots.len < 7);
        assert_eq!(heap.peek().unwrap().0, &1);
        heap.clear();
    }

    #[test]
    fn test_decrease_key_simple() {
        let mut heap = FibHeap::new();
        heap.push(10, 'a');
        let handle = heap.push(20, 'b');

        assert_eq!(heap.peek(), Some((&10, &'a')));

        // Decrease key of 'b'. It's a root, so no cut is needed.
        let _ = heap.decrease_key(handle, 5);
        assert_eq!(heap.peek(), Some((&5, &'b')));
        assert_eq!(heap.len(), 2);
    }

    #[test]
    fn test_decrease_key_with_cut() {
        let mut heap = FibHeap::new();
        heap.push(10, 'a');
        heap.push(20, 'b');
        heap.push(5, 'c');

        // Pop 5, which consolidates 10 and 20.
        assert_eq!(heap.pop(), Some((5, 'c')));
        assert_eq!(heap.roots.len, 1); // Only one root tree.
        assert_eq!(heap.peek(), Some((&10, &'a')));

        // Get handle for node 20, which is now a child of 10.
        // This relies on knowing the consolidation structure, but is safe here.
        let handle_20 = heap
            .pool
            .next(heap.pool.data(heap.min).unwrap().children.sentinel);
        assert_eq!(heap.pool.data(handle_20).unwrap().key, 20);

        // Decrease key of 20 to 8. This violates heap property.
        // Node 20 (now 8) must be cut and moved to root list.
        let _ = heap.decrease_key(handle_20, 8);

        assert_eq!(heap.peek(), Some((&8, &'b')));
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.roots.len, 2); // Roots should be 10 and 8
        heap.clear();
    }

    #[test]
    fn test_decrease_key_cascading_cut() {
        let mut heap = FibHeap::new();

        // --- Setup a 3-level tree: h10(GP) -> h30(P) -> h40(C) ---
        // This complex setup is proven by the `println!` logs to create
        // the structure we need.

        // Create C1 and C2 (h30, h40)
        let h30 = heap.push(30, "P"); // This will be the Parent
        let h40 = heap.push(40, "C"); // This will be the Child
        heap.push(29, "min");
        heap.pop(); // Pops 29. Consolidates h30 and h40. Root is h30(d=1)->h40.
        println!("#1: {heap}");

        // Create another node (h20, will be ignored)
        let h20 = heap.push(20, "P-Sibling");
        heap.push(19, "min");
        heap.pop(); // Pops 19. Consolidates h20(d=0) and h30(d=1). Roots are h20 and h30.
        println!("#2: {heap}");

        // Create GP and link the other trees
        let h10 = heap.push(10, "GP"); // This is the Grandparent
        heap.push(9, "min");
        heap.pop(); // Pops 9. Consolidates h10(d=0), h20(d=0), h30(d=1).
                    // This creates the final tree shown in log #6:
                    // h10(d=2) -> (h20, (h30 -> h40))
        println!("#3: {heap}");

        // --- Test the cascading cut logic ---

        // 1. Assert the structure is GP(h10) -> P(h30) -> C(h40)
        assert_eq!(
            heap.pool.data(h30).unwrap().parent,
            h10,
            "P(h30) should be child of GP(h10)"
        );
        assert_eq!(
            heap.pool.data(h40).unwrap().parent,
            h30,
            "C(h40) should be child of P(h30)"
        );

        // 2. Cut C (h40) from P (h30). This must MARK P (h30).
        let _ = heap.decrease_key(h40, 5); // New key is 5.
        println!("#4: {heap}"); // h40 is now a root

        // Verify C (h40) is now a root.
        assert!(
            heap.pool.data(h40).unwrap().parent.is_none(),
            "h40 should be a root after cut"
        );
        // Verify P (h30) is marked (it's not a root and lost a child).
        assert!(
            heap.pool.data(h30).unwrap().marked,
            "h30 should be marked after losing one child"
        );
        // Verify GP (h10) is NOT marked (it's the root).
        assert!(
            !heap.pool.data(h10).unwrap().marked,
            "h10 should not be marked"
        );

        // 3. Now, cut P (h30) from GP (h10). This must trigger a CASCADING CUT.
        let _ = heap.decrease_key(h30, 6); // New key is 6.
        println!("#5: {heap}"); // h30 is now a root

        // Verify P (h30) is now a root.
        assert!(
            heap.pool.data(h30).unwrap().parent.is_none(),
            "h30 should be a root after cascading cut"
        );
        // Verify P's mark was reset to false because it became a root.
        assert!(
            !heap.pool.data(h30).unwrap().marked,
            "h30's mark should be reset"
        );

        // Verify GP (h10) was *not* cut (it's root) but *was* checked by cascading_cut.
        // Since h10 is root, its parent is NONE, so cascading_cut(h10) does nothing.
        assert!(
            heap.pool.data(h10).unwrap().parent.is_none(),
            "h10 should still be a root"
        );
        // We can also check h20, which should be untouched.
        assert_eq!(
            heap.pool.data(h20).unwrap().parent,
            h10,
            "h20 should still be a child of h10"
        );
        heap.clear();
    }

    #[test]
    fn test_decrease_key_panic() {
        let mut heap = FibHeap::new();
        let handle = heap.push(10, ());
        let err = heap.decrease_key(handle, 20);
        assert_eq!(err, Err(DecreaseKeyError::NewKeyGreaterThanCurrent));
    }

    #[test]
    fn test_drain() {
        let mut heap = FibHeap::new();
        heap.push(20, 'd');
        heap.push(5, 'a');
        heap.push(15, 'c');
        heap.push(10, 'b');

        assert_eq!(heap.len(), 4);

        let mut drain = heap.drain();
        assert_eq!(drain.next(), Some((5, 'a')));
        assert_eq!(drain.next(), Some((10, 'b')));
        // After two pops, the heap is still valid but smaller.
        let rest: Vec<_> = drain.collect();
        assert_eq!(rest, vec![(15, 'c'), (20, 'd')]);

        // The original heap should now be empty.
        assert!(heap.is_empty());
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.pop(), None);
        heap.clear();
    }

    /// Validates the internal consistency of the Heap's graph structure.
    ///
    /// This checks invariants that must hold true before and after shrinking:
    /// 1. All nodes in the pool are reachable (no orphans).
    /// 2. Parent/Child pointers are bidirectional and correct.
    /// 3. Root nodes have no parents.
    fn validate_heap_integrity<K: Ord + fmt::Debug, V>(heap: &FibHeap<K, V>) {
        let mut visited = HashSet::new();
        let mut nodes_to_visit = Vec::new();

        // 1. Collect roots
        if !heap.roots.is_empty() {
            let mut current = heap.pool.next(heap.roots.sentinel);
            while current != heap.roots.sentinel {
                nodes_to_visit.push((current, FibHandle::NONE)); // Root has NONE parent
                current = heap.pool.next(current);
            }
        }

        // 2. Traverse the graph
        while let Some((handle, expected_parent)) = nodes_to_visit.pop() {
            assert!(visited.insert(handle), "Cycle detected or node visited twice!");

            let node = heap.pool.data(handle).expect("Handle points to free/invalid memory");

            // Check Parent integrity
            assert_eq!(node.parent, expected_parent, "Node {:?} has wrong parent pointer", handle);

            // Check Children
            if !node.children.is_empty() {
                let mut child = heap.pool.next(node.children.sentinel);
                let mut count = 0;
                while child != node.children.sentinel {
                    nodes_to_visit.push((child, handle)); // Expect this node to be the parent
                    child = heap.pool.next(child);
                    count += 1;
                }
                assert_eq!(node.children.len(), count, "Child list len mismatch");
            }
        }

        // 3. Ensure we visited every used node in the pool
        // (Subtracting 1 for the pool sentinel is handled by pool.len())
        assert_eq!(visited.len(), heap.len(), "Pool contains orphaned nodes not reachable from roots!");
    }

    #[test]
    fn test_shrink_handle_usability() {
        let mut heap = FibHeap::new();

        // 1. Setup: Create a structure with depth
        // Push enough items to force consolidation later
        let _h1 = heap.push(10, "A");
        let _h2 = heap.push(20, "B");
        let h3 = heap.push(30, "C");
        let h4 = heap.push(40, "D");

        // 2. Create Holes.
        // We need to pop items that were allocated early (low indices) to force moves.
        // FibHeap pops min. 10 is min.
        assert_eq!(heap.pop(), Some((10, "A"))); // Pops h1. Hole at index associated with h1.

        // Structure might now be consolidated (depending on degree).
        // Let's ensure we have a tree.
        // Pop again to force more consolidation.
        assert_eq!(heap.pop(), Some((20, "B"))); // Pops h2.

        // Remaining: 30 and 40.
        // Depending on consolidation logic, one might be child of other.

        println!("Before shrink: {}", heap);
        validate_heap_integrity(&heap);

        // 3. Shrink
        let map = heap.shrink_to_fit();

        println!("After shrink: {}", heap);
        validate_heap_integrity(&heap);

        // 4. Update Handles
        // h3 and h4 are the remaining ones. They likely moved.
        let _new_h3 = map.get(&h3).copied().unwrap_or(h3);
        let new_h4 = map.get(&h4).copied().unwrap_or(h4);

        // 5. Test Usability: Decrease Key
        // This is critical: decrease_key accesses `node.parent`.
        // If internal parent pointers weren't fixed, this panics or reads garbage.
        heap.decrease_key(new_h4, 5).expect("Decrease key failed on remapped handle");

        assert_eq!(heap.peek().unwrap().0, &5);
        assert_eq!(heap.pop(), Some((5, "D")));
        assert_eq!(heap.pop(), Some((30, "C")));
    }

    #[test]
    fn test_shrink_structural_stress() {
        let mut heap = FibHeap::new();
        let mut handles = Vec::new();

        // 1. Build a large, complex heap
        for i in 0..100 {
            handles.push(heap.push(i, i));
        }

        // 2. Perform random Pops to create fragmentation and trees
        // Removing 40% of items randomly-ish (by popping min repeatedly)
        for _ in 0..40 {
            heap.pop();
        }

        // Remove handles that were popped (approximated by removing first 40)
        // This is just for our tracking; the heap knows truth.
        handles.drain(0..40);

        let initial_len = heap.len();
        let initial_cap = heap.pool_capacity();

        validate_heap_integrity(&heap);

        // 3. Shrink
        let map = heap.shrink_to_fit();

        // 4. Verify
        assert_eq!(heap.len(), initial_len);
        assert!(heap.pool_capacity() <= initial_cap);
        validate_heap_integrity(&heap);

        // 5. Remap our external handles and verify data access
        for old_handle in handles {
            let new_handle = map.get(&old_handle).copied().unwrap_or(old_handle);

            // We should be able to read every remaining node
            let node = heap.pool.data(new_handle).expect("Remapped handle points to nothing!");
            // Basic sanity check that keys are in expected range
            assert!(node.key >= 40 && node.key < 100);
        }

        // 6. Clean up
        while heap.pop().is_some() {}
    }

    #[test]
    fn test_shrink_empty_and_single() {
        // Case 1: Empty
        let mut heap = FibHeap::<i32, i32>::new();
        let map = heap.shrink_to_fit();
        assert!(map.is_empty());
        validate_heap_integrity(&heap);

        // Case 2: Single Item
        let _h = heap.push(1, 1);
        // Pop it to make a hole, then push new one?
        // Or just push, then clear... no wait.
        // To test movement of a single item, we need a hole below it.
        // push(1), push(2), pop(1). Heap has [2] at index 2. Hole at 1.
        let h2 = heap.push(2, 2);
        heap.pop(); // removes 1.

        let map = heap.shrink_to_fit();
        // h2 should have moved.
        assert!(map.contains_key(&h2));
        let new_h2 = map[&h2];

        assert_eq!(heap.peek(), Some((&2, &2)));

        // Ensure internal min pointer updated
        assert_eq!(heap.min, new_h2);
    }
}
