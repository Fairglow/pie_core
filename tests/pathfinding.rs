use pie_core::FibHeap;
use std::collections::HashMap;

// A simple graph representation for the test:
// Node -> Vec<(Neighbor, Weight)>
type Graph = HashMap<char, Vec<(char, u32)>>;

// Helper to create our test graph
fn create_graph() -> Graph {
    let mut graph = Graph::new();
    // A -> (B, 10), (C, 3)
    // B -> (D, 2)
    // C -> (B, 4), (D, 8), (E, 2)
    // D -> (E, 5)
    // E -> (unconnected)
    graph.insert('A', vec![('B', 10), ('C', 3)]);
    graph.insert('B', vec![('D', 2)]);
    graph.insert('C', vec![('B', 4), ('D', 8), ('E', 2)]);
    graph.insert('D', vec![('E', 5)]);
    graph.insert('E', vec![]);
    graph
}

/// Helper function to print the graph structure for easy debugging.
fn visualize_graph(graph: &Graph) -> String {
    let mut output = String::from("Graph Structure:\n");
    // We sort the keys to ensure a consistent print order
    let mut nodes: Vec<_> = graph.keys().collect();
    nodes.sort();
    for node in nodes {
        let neighbors = graph.get(node).unwrap();
        let neighbors_str: Vec<String> = neighbors.iter()
            .map(|(n, w)| format!("({}, {})", n, w))
            .collect();
        output.push_str(&format!("  {} -> [ {} ]\n", node, neighbors_str.join(", ")));
    }
    output
}

#[test]
fn real_world_pathfinding_test() {
    println!("--- Test Start: Dijkstra's Pathfinding ---");

    // 1. SETUP: Create the heap.
    // The heap's key will be the distance (u32), and the
    // value will be the node's identifier (char).
    let mut heap = FibHeap::new();
    let graph = create_graph();
    let start_node = 'A';
    let nodes = ['A', 'B', 'C', 'D', 'E'];
    println!("{}", visualize_graph(&graph));
    println!("Starting node: {start_node}");
    // We need two maps:
    // 1. `distances`: To store the shortest distance found so far for each node.
    // 2. `handles`: To map a node ('A') to its handle in the heap,
    //    which is required for the `decrease_key` operation.
    let mut distances = HashMap::new();
    let mut handles = HashMap::new();

    // 2. INITIALIZATION:
    // Populate the heap. The start node has a distance of 0,
    // all others have "infinity" (u32::MAX).
    println!("Populating heap with initial distances...");
    for &node in &nodes {
        let dist = if node == start_node { 0 } else { u32::MAX };
        distances.insert(node, dist);
        // Push to the heap and store the returned handle
        let handle = heap.push(dist, node);
        handles.insert(node, handle);
        println!("Pushed: ('{}', prio: {})", node, dist);
    }
    // Verify heap size
    assert_eq!(heap.len(), nodes.len());

    // 3. RUN ALGORITHM:
    // Loop while the heap is not empty
    while let Some((current_dist, current_node)) = heap.pop() {
        println!("\n--- Popped node '{}' with dist {} ---", current_node, current_dist);
        // If the shortest path to this node is already "infinity",
        // it means the remaining nodes are unreachable.
        if current_dist == u32::MAX {
            println!("Node is unreachable. Stopping.");
            break;
        }
        // Iterate over neighbors of the popped node
        if let Some(neighbors) = graph.get(&current_node) {
            for &(neighbor, weight) in neighbors {
                let new_dist = current_dist + weight;

                // 4. DECREASE_KEY SCENARIO:
                // If we found a shorter path to the neighbor...
                if new_dist < distances[&neighbor] {
                    println!("Found shorter path to '{}': {} < {}", neighbor, new_dist, distances[&neighbor]);
                    // Update the distance in our tracking map
                    distances.insert(neighbor, new_dist);
                    // Get the handle for the neighbor node
                    let neighbor_handle = handles[&neighbor];
                    // *** This is the key test ***
                    // Update the node's priority in the heap.
                    heap.decrease_key(neighbor_handle, new_dist).unwrap();
                    println!("Updated heap: ('{}', prio: {})", neighbor, new_dist);
                }
            }
        }
    }

    // 5. VERIFICATION:
    // The heap should now be empty
    assert_eq!(heap.len(), 0);
    println!("\n--- Final Shortest Distances from '{}' ---", start_node);
    println!("Node A: {}", distances[&'A']); // 0
    println!("Node B: {}", distances[&'B']); // 7  (A->C->B)
    println!("Node C: {}", distances[&'C']); // 3  (A->C)
    println!("Node D: {}", distances[&'D']); // 9  (A->C->B->D)
    println!("Node E: {}", distances[&'E']); // 5  (A->C->E)
    // Verify the final calculated distances
    // Path A->C->E (3+2=5) is shorter than A->C->B->D->E (3+4+2+5=14)
    // Path A->C->B (3+4=7) is shorter than A->B (10)
    // Path A->C->B->D (3+4+2=9) is shorter than A->C->D (3+8=11)
    assert_eq!(distances[&'A'], 0);
    assert_eq!(distances[&'B'], 7);
    assert_eq!(distances[&'C'], 3);
    assert_eq!(distances[&'D'], 9);
    assert_eq!(distances[&'E'], 5);
    println!("--- Test Complete ---");
}
