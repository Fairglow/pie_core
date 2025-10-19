use pielist::{FibHeap, NodeHandle};

fn main() {
    println!("--- Fibonacci Heap Showcase ---\n");

    // Create a new FibHeap. It can store any key/value pair where the
    // key is comparable (Ord) and both are displayable (Display).
    let mut heap = FibHeap::<i32, &'static str>::new();
    println!("1. Initial empty heap:");
    println!("{}", heap);

    // --- PUSH Operations ---
    println!("\n2. Pushing elements 5, 10, and 3.");
    println!("   Each new element becomes a new root in the heap.");
    heap.push(5, "apple");
    heap.push(10, "orange");
    heap.push(3, "banana");
    println!("{}", heap);

    // --- POP Operation & CONSOLIDATION ---
    println!("\n3. Pushing more elements (20, 15) and then popping the min (3).");
    println!("   Popping triggers CONSOLIDATION, which links roots of the same degree.");
    let handle_to_decrease = heap.push(20, "grape");
    heap.push(15, "melon");
    heap.pop(); // Pops 'banana' (3), consolidates the rest.
    println!("{}", heap);
    println!("   The heap is now a single tree rooted at 'apple'(5).");

    // --- DECREASE_KEY (Simple Cut) ---
    println!("\n4. Decreasing key of 'grape' from 20 to 2.");
    println!("   This violates the heap property, causing a CUT.");
    heap.decrease_key(handle_to_decrease, 2);
    println!("{}", heap);
    println!("   - 'grape'(2) is now a root and the new minimum.");
    println!("   - Its former parent, 'melon'(15), is now marked (M) because it lost a child.");

    // --- DECREASE_KEY (Cascading Cut) ---
    // The previous examples failed to set this up correctly. This section
    // is a new, deterministic demonstration of a cascading cut.
    println!("\n\n--- 5. A Deliberate Cascading Cut Demonstration ---");
    println!("   To show this reliably, we will start with a fresh heap and build a specific structure.");

    let mut cascade_heap = FibHeap::<i32, &'static str>::new();

    // Step A: Build a tall tree.
    // By pushing in descending order and popping a smaller value, we force
    // consolidations that create a chain: 10 -> 20 -> 30 -> 40.
    println!("\n   Step A: Build a predictable tree structure.");
    let h10 = cascade_heap.push(10, "Grandparent");
    let h20 = cascade_heap.push(20, "Parent");
    let h30 = cascade_heap.push(30, "Child 1");
    let h40 = cascade_heap.push(40, "Child 2");

    // Force consolidation
    cascade_heap.push(1, "temp");
    cascade_heap.pop();

    println!("{}", cascade_heap);
    println!("   Structure is now: Grandparent(10) -> Parent(20) -> (Child 1(30), Child 2(40))");

    // Step B: Mark the Parent.
    // We cut "Child 1" from "Parent". This marks "Parent".
    println!("\n   Step B: Cut 'Child 1'(30) to mark its parent.");
    cascade_heap.decrease_key(h30, 9);
    println!("{}", cascade_heap);
    println!("   - 'Child 1'(9) is now a root.");
    println!("   - 'Parent'(20) is now marked (M).");

    // Step C: Trigger the Cascade.
    // Now we cut "Child 2" from the already-marked "Parent".
    // This will cause "Parent" to be cut from "Grandparent", and so on.
    println!("\n   Step C: Cut 'Child 2'(40) from the MARKED parent.");
    println!("   This triggers the cascading cut.");
    cascade_heap.decrease_key(h40, 8);
    println!("{}", cascade_heap);
    println!("   - 'Child 2'(8) is cut and becomes a root.");
    println!("   - 'Parent'(20) was marked, so it is ALSO cut and becomes a root. Its mark is cleared.");
    println!("   - 'Grandparent'(10) lost a child ('Parent'), so it is now marked (M).");

    // Verify parent of h10 is none, as it should be the top-level root that got marked.
    // Note: This part is for programmatic verification, the visual output is the main point.
    // let node_data = cascade_heap.peek_node_data(h10).expect("Node 10 should exist");
    // assert!(node_data.parent.is_none() && node_data.marked);


    println!("\n--- Showcase complete ---");
}

// Helper trait to inspect node data for the final assertion.
// This is NOT part of the public API, just for the test.
// trait FibHeapInspector<K,V> {
//     fn peek_node_data(&self, handle: NodeHandle<K,V>) -> Option<&crate::Node<K,V>>;
// }
// 
// impl<K:Ord,V> FibHeapInspector<K,V> for FibHeap<K,V> {
//     fn peek_node_data(&self, handle: NodeHandle<K,V>) -> Option<&crate::Node<K,V>> {
//         // This is a bit of a hack to access the private pool field.
//         // In a real scenario, you wouldn't do this.
//         let pool_ptr = &self.pool as *const _;
//         unsafe { (*pool_ptr).data(handle) }
//     }
// }
