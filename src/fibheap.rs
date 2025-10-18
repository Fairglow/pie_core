//! A Fibonacci heap implementation, built on the `pielist` pool.

use crate::index::Index;
use crate::list::PieList;
use crate::pool::ElemPool;
use std::mem;

// This is the data that will be stored inside the `ElemPool`.
// It's separate from the `ListElem`'s `next`/`prev` links.
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
/// This handle is stable and remains valid until the node is
/// removed from the heap. It is required for the `decrease_key` operation.
pub type NodeHandle<K, V> = Index<Node<K, V>>;

/// A Fibonacci heap, designed for efficient priority queue operations.
///
/// This heap is implemented on top of a `pielist::ElemPool`, which avoids
/// per-node allocations and provides high performance.
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
    /// # use pielist::FibHeap;
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
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the heap contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Removes all elements from the heap.
    ///
    /// This is an O(n) operation, as it must visit every node
    /// to return its memory to the pool.
    pub fn clear(&mut self) {
                // We can't just drop the pool, as it's not recursive.
                // We also can't just clear the root list, as that
                // would leak all descendant nodes.        // We must re-create the heap.
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
    /// Returns a `NodeHandle` which can be used with `decrease_key`.
    ///
    /// # Complexity
    /// O(1) amortized time.
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
        self.roots
            .push_front(node, &mut self.pool)
            .expect("Failed to allocate node");

        // `push_front` adds the node as the *first* element after the sentinel.
        let handle = self.pool.next(self.roots.sentinel);

        // Update the minimum pointer if necessary.
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
    /// # Complexity
    /// O(1) time.
    pub fn peek(&self) -> Option<(&K, &V)> {
        // `self.pool.data` safely handles `self.min` being `NONE`.
        self.pool.data(self.min).map(|node| (&node.key, &node.value))
    }

    /// Removes and returns the element with the smallest key (highest priority).
    ///
    /// # Complexity
    /// O(log n) amortized time.
    pub fn pop(&mut self) -> Option<(K, V)> {
        if self.min.is_none() {
            return None;
        }

        let min_handle = self.min;

        // 1. Unlink the min node from the root list.
        // We use the `pub(crate)` function from ElemPool.
        self.pool.index_linkout(min_handle).unwrap();
        self.roots.len -= 1;

        // 2. Take the node data out of the pool.
        // This leaves the `ListElem` empty, ready for `index_del`.
        let min_node_data = self.pool.data_swap(min_handle, None).unwrap();
        self.len -= 1;

        // 3. Move the min node's children to the root list.
        let num_children = min_node_data.children.len;
        if num_children > 0 {
            // Get the first and last child.
            let first_child = self.pool.next(min_node_data.children.sentinel);
            let last_child = self.pool.prev(min_node_data.children.sentinel);

            // Get the current end of the root list.
            let root_last = self.pool.prev(self.roots.sentinel);

            // Splice the children in at the end of the root list.
            // (root_last) <-> (first_child)
            self.pool.get_mut(root_last).unwrap().new_next(first_child);
            self.pool.get_mut(first_child).unwrap().new_prev(root_last);

            // (last_child) <-> (root_sentinel)
            self.pool.get_mut(last_child).unwrap().new_next(self.roots.sentinel);
            self.pool.get_mut(self.roots.sentinel).unwrap().new_prev(last_child);

            // Update root list length
            self.roots.len += num_children;

            // 4. Set the parent of all moved children to `NONE`.
            let mut current = first_child;
            for _ in 0..num_children {
                self.pool.data_mut(current).unwrap().parent = NodeHandle::NONE;
                current = self.pool.next(current);
            }
        }

        // 5. Delete the now-empty sentinels and node elements.
        // This returns their memory to the pool's free list.
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

        // We need to iterate and *modify* the list, so we get the handles first.
        let mut handles = Vec::with_capacity(self.roots.len());
        let mut current = self.pool.next(self.roots.sentinel);
        while current != self.roots.sentinel {
            handles.push(current);
            current = self.pool.next(current);
        }

        // Reset the root list. The nodes are unlinked one by one.
        self.pool.get_mut(self.roots.sentinel).unwrap().new_links(self.roots.sentinel, self.roots.sentinel);
        self.roots.len = 0;

        for &handle in &handles {
            let mut x = handle;
            let mut d = self.pool.data(x).unwrap().degree;

            while let Some(mut y) = a[d] {
                // Ensure x is the smaller key
                if self.pool.data(x).unwrap().key > self.pool.data(y).unwrap().key {
                    mem::swap(&mut x, &mut y);
                }
                // Link y under x.
                self.heap_link(y, x);
                a[d] = None;
                d += 1;
            }
            a[d] = Some(x);
        }

        // Rebuild the root list from the consolidated trees in A.
        for handle in a.iter().flatten() {
            // Add node to the root list.
            self.pool.index_link_after(*handle, self.roots.sentinel).unwrap();
            self.roots.len += 1;
            self.update_min(*handle);
        }
    }

    /// Links node `y` as a child of node `x`.
    /// `y` is assumed to be unlinked from any list.
    fn heap_link(&mut self, y: NodeHandle<K, V>, x: NodeHandle<K, V>) {
        let mut x_children = self.pool.data_mut(x).unwrap().children;

        // Add `y` to `x`'s child list (at the front).
        self.pool.index_link_after(y, x_children.sentinel).unwrap();
        x_children.len += 1;
        self.pool.data_mut(x).unwrap().children = x_children;

        // Update `x`'s degree.
        self.pool.data_mut(x).unwrap().degree += 1;

        // Set `y`'s parent and mark.
        self.pool.data_mut(y).unwrap().parent = x;
        self.pool.data_mut(y).unwrap().marked = false;
    }

    /// Decreases the key of a node in the heap.
    ///
    /// # Panics
    /// Panics if the `new_key` is greater than the current key.
    ///
    /// # Complexity
    /// O(1) amortized time.
    pub fn decrease_key(&mut self, handle: NodeHandle<K, V>, new_key: K) {
        let parent;

        // --- Scope 1: Read-only access ---
        {
            let node = self.pool.data(handle).expect("Invalid handle in decrease_key");
            if new_key > node.key {
                panic!("new_key is greater than current key");
            }
            // `parent` is a NodeHandle, which is `Copy`
            parent = node.parent;
        }
        // `node` (an immutable borrow) is dropped here.

        // --- Scope 2: Write access ---
        {
            // Now we can safely get a mutable borrow.
            let node_mut = self.pool.data_mut(handle).unwrap();
            node_mut.key = new_key;
        }
        // `node_mut` (a mutable borrow) is dropped here.

        // --- Scope 3: Read-only access (again) ---
        // We can now read from the pool again to check for heap property violation.
        if parent.is_some()
            && self.pool.data(handle).unwrap().key < self.pool.data(parent).unwrap().key
        {
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
// This is safe because `drop` only runs once.
impl<K, V> Drop for FibHeap<K, V> {
        fn drop(&mut self) {
            // The pool will be dropped, but we must
            // iterate and drop all the child `PieList` sentinels        // to prevent a "double free" or logic error.
        // A full `clear` is the safest way.
        self.clear();
    }
}
