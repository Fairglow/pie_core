//! A Fibonacci heap implementation, built on the `pielist` pool.

use crate::index::Index;
use crate::list::PieList;
use crate::pool::ElemPool;
use std::{fmt, mem};

/// An opaque struct representing a node within the `FibHeap`.
///
/// Users of the heap cannot interact with this struct directly, but it is
/// made public to allow the `NodeHandle` type alias to be public as well.
pub struct Node<K, V> {
    key: K,
    value: V,
    /// Handle to this node's parent. `NONE` if this is a root.
    parent: NodeHandle<K, V>,
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
pub type NodeHandle<K, V> = Index<Node<K, V>>;

/// A Fibonacci heap, designed for efficient priority queue operations.
///
/// This heap is implemented on top of a `pielist::ElemPool`, which avoids
/// per-node allocations and provides high performance. It is a min-heap,
/// meaning that `pop` will always return the element with the smallest key.
///
/// # Type Parameters
///
/// - `K`: The key type, which determines the priority of an element. Must implement `Ord`.
/// - `V`: The value type, which is the data stored in the heap.
pub struct FibHeap<K, V> {
    /// The pool allocator that stores all nodes for this heap.
    pool: ElemPool<Node<K, V>>,
    /// A handle to the doubly-linked list of root nodes.
    roots: PieList<Node<K, V>>,
    /// A handle to the node with the minimum key.
    min: NodeHandle<K, V>,
    /// The total number of nodes in the heap.
    len: usize,
}

impl<K, V> FibHeap<K, V> {
    /// Creates a new, empty `FibHeap`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pielist::fibheap::FibHeap;
    /// let mut heap = FibHeap::<u32, &str>::new();
    /// assert!(heap.is_empty());
    /// ```
    pub fn new() -> Self {
        let mut pool = ElemPool::new();
        // The root list needs its own sentinel, allocated from the pool.
        let roots = PieList::new(&mut pool);
        Self {
            pool,
            roots,
            min: NodeHandle::NONE,
            len: 0,
        }
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
    /// This is the capacity of the underlying `pielist::ElemPool`.
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
        // We can't just clear the root list, as that would leak all descendant nodes.
        // We must re-create the heap's internal state entirely.
        let mut new_pool = ElemPool::new();
        self.roots = PieList::new(&mut new_pool);
        self.pool = new_pool;
        self.min = NodeHandle::NONE;
        self.len = 0;
    }
}

impl<K: Ord, V> FibHeap<K, V> {
    /// Pushes a new key-value pair onto the heap.
    ///
    /// Returns a `pielist::FibNodeHandle` which can be used with `decrease_key`.
    ///
    /// # Complexity
    ///
    /// O(1) amortized time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pielist::fibheap::FibHeap;
    /// let mut heap = FibHeap::new();
    /// let handle = heap.push(10, "ten");
    /// assert_eq!(heap.len(), 1);
    /// ```
    pub fn push(&mut self, key: K, value: V) -> NodeHandle<K, V> {
        // Each node needs its *own* child list, which means
        // allocating a new sentinel for it.
        let children = PieList::new(&mut self.pool);
        let node = Node {
            key,
            value,
            parent: NodeHandle::NONE,
            children,
            degree: 0,
            marked: false,
        };
        // `push_front` creates the `ListElem` in the pool,
        // stores our `node` data inside it, and links it.
        // We panic on OOM, which is standard for collections.
        self.roots.push_front(node, &mut self.pool)
            .expect("Failed to allocate node");

        // `push_front` adds the node as the *first* element after the sentinel.
        let handle = self.pool.next(self.roots.sentinel);

        self.update_min(handle);
        self.len += 1;
        handle
    }

    /// Helper to update the `min` pointer.
    fn update_min(&mut self, handle: NodeHandle<K, V>) {
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
    /// # use pielist::fibheap::FibHeap;
    /// let mut heap = FibHeap::new();
    /// heap.push(5, 'a');
    /// heap.push(3, 'b');
    ///
    /// assert_eq!(heap.peek(), Some((&3, &'b')));
    /// ```
    pub fn peek(&self) -> Option<(&K, &V)> {
        // `self.pool.data` safely handles `self.min` being `NONE`.
        self.pool.data(self.min)
            .map(|node| (&node.key, &node.value))
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
    /// # use pielist::fibheap::FibHeap;
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
                self.pool.data_mut(current).unwrap().parent = NodeHandle::NONE;
                current = self.pool.next(current);
            }
        }
        // 5. Return memory to the pool.
        self.pool.index_del(min_node_data.children.sentinel).unwrap();
        self.pool.index_del(min_handle).unwrap();

        // 6. Consolidate the root list.
        if self.roots.is_empty() {
            self.min = NodeHandle::NONE;
        } else {
            self.consolidate();
        }
        Some((min_node_data.key, min_node_data.value))
    }

    /// Consolidates the root list by linking trees of the same degree.
    fn consolidate(&mut self) {
        // Max degree is ~log_phi(n). 64 is safe for n up to 2^64.
        let mut a: Vec<Option<NodeHandle<K, V>>> = vec![None; 64];
        self.min = NodeHandle::NONE;

        let mut handles = Vec::with_capacity(self.roots.len());
        let mut current = self.pool.next(self.roots.sentinel);
        while current != self.roots.sentinel {
            handles.push(current);
            current = self.pool.next(current);
        }
        self.pool.get_mut(self.roots.sentinel).unwrap().new_links(self.roots.sentinel, self.roots.sentinel);
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
    fn heap_link(&mut self, y: NodeHandle<K, V>, x: NodeHandle<K, V>) {
        let mut x_children = self.pool.data_mut(x).unwrap().children;

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
    /// # Panics
    ///
    /// Panics if `new_key` is greater than the node's current key, or if the
    /// handle is invalid.
    ///
    /// # Complexity
    ///
    /// O(1) amortized time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use pielist::fibheap::{FibHeap, FibNodeHandle};
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
    pub fn decrease_key(&mut self, handle: NodeHandle<K, V>, new_key: K) {
        let parent = {
            let node = self.pool.data(handle).expect("Invalid handle in decrease_key");
            if new_key > node.key {
                panic!("new_key is greater than current key");
            }
            node.parent
        };
        {
            let node_mut = self.pool.data_mut(handle).unwrap();
            node_mut.key = new_key;
        }
        if parent.is_some() && self.pool.data(handle).unwrap().key < self.pool.data(parent).unwrap().key {
            self.cut(handle, parent);
            self.cascading_cut(parent);
        }
        self.update_min(handle);
    }

    /// Cuts node `x` from its parent `y`.
    fn cut(&mut self, x: NodeHandle<K, V>, y: NodeHandle<K, V>) {
        // 1. Unlink `x` from `y`'s child list.
        self.pool.index_linkout(x).unwrap();
        let mut y_children = self.pool.data_mut(y).unwrap().children;
        y_children.len -= 1;
        self.pool.data_mut(y).unwrap().children = y_children;
        self.pool.data_mut(y).unwrap().degree -= 1;

        // 2. Add `x` to the root list.
        self.pool.index_link_after(x, self.roots.sentinel).unwrap();
        self.roots.len += 1;

        // 3. Update `x`'s parent and mark.
        self.pool.data_mut(x).unwrap().parent = NodeHandle::NONE;
        self.pool.data_mut(x).unwrap().marked = false;
    }

    /// Performs a cascading cut on node `y`.
    fn cascading_cut(&mut self, y: NodeHandle<K, V>) {
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
        writeln!(
            f,
            "FibHeap (len: {}, min: {})",
            self.len,
            self.pool.data(self.min).map_or_else(|| "N/A".to_string(), |n| n.key.to_string())
        )?;
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
        handle: NodeHandle<K, V>,
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
        writeln!(
            f,
            "{}{} Node(k: {}, v: {}){}",
            prefix, connector, node.key, node.value, marked_str
        )?;
        // Prepare the prefix for children
        let child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });

        // Recursively print children
        let children_list = node.children;
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


#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_decrease_key_simple() {
        let mut heap = FibHeap::new();
        heap.push(10, 'a');
        let handle = heap.push(20, 'b');

        assert_eq!(heap.peek(), Some((&10, &'a')));

        // Decrease key of 'b'. It's a root, so no cut is needed.
        heap.decrease_key(handle, 5);
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
        let handle_20 = heap.pool.next(heap.pool.data(heap.min).unwrap().children.sentinel);
        assert_eq!(heap.pool.data(handle_20).unwrap().key, 20);

        // Decrease key of 20 to 8. This violates heap property.
        // Node 20 (now 8) must be cut and moved to root list.
        heap.decrease_key(handle_20, 8);

        assert_eq!(heap.peek(), Some((&8, &'b')));
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.roots.len, 2); // Roots should be 10 and 8
    }

    #[test]
    fn test_decrease_key_cascading_cut() {
        let mut heap = FibHeap::new();

        // 1. Setup a predictable structure that forces Grandparent -> Parent -> Children
        let h10 = heap.push(10, "GP");
        let h20 = heap.push(20, "P");
        let h30 = heap.push(30, "C1");
        let h40 = heap.push(40, "C2");

        // Push and pop values smaller than the main nodes to force consolidation
        // without affecting the main nodes.
        heap.push(0, "min");
        heap.pop(); // Consolidates 10, 20, 30, 40. A likely structure is 10 -> (20, 30, 40)

        heap.push(1, "min");
        heap.pop(); // Further consolidation. A likely structure is 10 -> (20 -> (30, 40))

        // Assert the structure we need for the test. This makes the test robust.
        assert_eq!(heap.pool.data(h20).unwrap().parent, h10, "h20 should be a child of h10");
        assert_eq!(heap.pool.data(h30).unwrap().parent, h20, "h30 should be a child of h20");
        assert_eq!(heap.pool.data(h40).unwrap().parent, h20, "h40 should be a child of h20");

        // 2. Cut C1 (h30) from P (h20). This must MARK P.
        heap.decrease_key(h30, 5); // New key is 5.

        // Verify C1 is now a root.
        assert!(heap.pool.data(h30).unwrap().parent.is_none(), "h30 should be a root after cut");
        // Verify P is marked.
        assert!(heap.pool.data(h20).unwrap().marked, "h20 should be marked after losing one child");

        // 3. Cut C2 (h40) from P (h20). This must trigger a CASCADING CUT on P.
        heap.decrease_key(h40, 6); // New key is 6.

        // Verify C2 is now a root.
        assert!(heap.pool.data(h40).unwrap().parent.is_none(), "h40 should be a root after cut");
        // Verify P (h20) was also cut and is now a root.
        assert!(heap.pool.data(h20).unwrap().parent.is_none(), "h20 should be a root after cascading cut");
        // Verify P's mark was reset to false because it became a root.
        assert!(!heap.pool.data(h20).unwrap().marked, "h20's mark should be reset to false");

        // Verify GP (h10) is now marked, because it's a non-root that lost a child (h20).
        assert!(heap.pool.data(h10).unwrap().marked, "h10 should be marked after losing h20");
    }

    #[test]
    #[should_panic]
    fn test_decrease_key_panic() {
        let mut heap = FibHeap::new();
        let handle = heap.push(10, ());
        heap.decrease_key(handle, 20); // Panics because 20 > 10
    }
}
