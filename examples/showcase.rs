use pielist::FibHeap;

fn main() {
    println!("--- Fibonacci Heap Showcase ---\n");

    // Create a new FibHeap. It can store any key/value pair where the
    // key is comparable (Ord) and both are displayable (Display).
    let mut heap = FibHeap::<i32, &'static str>::new();
    println!("1. Initial empty heap:");
    println!("{heap}");

    // --- PUSH Operations ---
    println!("\n2. Pushing elements 5, 10, and 3.");
    println!("   Each new element becomes a new root in the heap.");
    heap.push(5, "apple");
    heap.push(10, "orange");
    heap.push(3, "banana");
    println!("{heap}");

    // --- POP Operation & CONSOLIDATION ---
    println!("\n3. Pushing more elements (20, 15) and then popping the min (3).");
    println!("   Popping triggers CONSOLIDATION, which links roots of the same degree.");
    let handle_to_decrease = heap.push(20, "grape");
    heap.push(15, "melon");
    heap.pop(); // Pops 'banana' (3), consolidates the rest.
    println!("{heap}");
    println!("   The heap is now a single tree rooted at 'apple'(5).");

    // --- DECREASE_KEY (Simple Cut) ---
    println!("\n4. Decreasing key of 'grape' from 20 to 2.");
    println!("   This violates the heap property, causing a CUT.");
    heap.decrease_key(handle_to_decrease, 2);
    println!("{heap}");
    println!("   - 'grape'(2) is now a root and the new minimum.");
    println!("   - Its former parent, 'melon'(15), is now marked (M) because it lost a child.");


    // --- 5. A True Cascading Cut Demonstration ---
    println!("\n\n--- 5. A True Cascading Cut Demonstration ---");
    println!("   To see a multi-level cascade, we need a tall tree with specific properties.");

    let mut cascade_heap = FibHeap::<i32, &'static str>::new();

    // Step A: Build a guaranteed 4-level tree structure.
    // We create a chain by pushing 8 nodes and forcing a full consolidation.
    // The specific chain that forms is GGP(20) -> GP(60) -> P(80) -> C(90).
    println!("\n   Step A: Build the required tree structure.");

    // Push 8 nodes. We only need handles for the ones in our target chain and their siblings.
    let h20 = cascade_heap.push(20, "Great-Grandparent");
    let _h30 = cascade_heap.push(30, "GGP-Sibling-1");
    let _h40 = cascade_heap.push(40, "GGP-Sibling-2");
    let _h50 = cascade_heap.push(50, "P-Sibling");
    let h60 = cascade_heap.push(60, "Grandparent");
    let h70 = cascade_heap.push(70, "GP-Sibling");
    let h80 = cascade_heap.push(80, "Parent");
    let h90 = cascade_heap.push(90, "Child");

    // Pop a value smaller than all others to force a full consolidation.
    cascade_heap.push(1, "temp");
    cascade_heap.pop();

    println!("{cascade_heap}");
    println!("   The structure now contains the 4-level chain we need for our test:");
    println!("   GGP(20) -> GP(60) -> P(80) -> C(90)");


    // Step B: Mark the Grandparent and Parent nodes.
    // A node is marked when it is not a root and it loses a child.
    println!("\n   Step B: Mark the 'Grandparent' and 'Parent' nodes.");

    // Cut "GP-Sibling"(70) from "Grandparent"(60). This marks the Grandparent.
    cascade_heap.decrease_key(h70, 18);

    // Cut "Child"(90) from "Parent"(80). This marks the Parent.
    cascade_heap.decrease_key(h90, 17);

    println!("{cascade_heap}");
    println!("   - 'GP-Sibling'(18) and 'Child'(17) are now roots.");
    println!("   - 'Parent'(80) is marked (M) because it lost its child.");
    println!("   - 'Grandparent'(60) is also marked (M) for the same reason.");


    // Step C: Trigger the Cascade.
    // Now, we cut 'Parent'(80) from 'Grandparent'(60).
    // Because 'Grandparent'(60) is already marked, the cut will cascade upwards.
    println!("\n   Step C: Cut 'Parent'(80) from the marked 'Grandparent'(60).");
    println!("   This triggers the multi-level cascading cut.");
    cascade_heap.decrease_key(h80, 16);
    println!("{cascade_heap}");
    println!("   - 'Parent'(16) is cut from 'Grandparent' and becomes a root. Its mark is cleared.");
    println!("   - The cut CASCADES: 'Grandparent'(60) was marked and just lost a child, so it is");
    println!("     ALSO cut from 'Great-Grandparent'(20). It becomes a new root and its mark is cleared.");
    println!("   - The cascade stops at 'Great-Grandparent'(20). It lost a child ('Grandparent')");
    println!("     but is already a root and is therefore not marked (M).");
    println!("\n--- Showcase complete ---");
}
