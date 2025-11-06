use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use pie_core::{FibHeap, Index, Node};
use std::collections::HashMap;
use std::u64;

type Handle = Index<Node<u64, NodeIndex>>;

/// VISUALIZATION FUNCTION
/// Prints the current state of the algorithm's data structures.
fn visualize_state(
    step_name: &str,
    graph: &Graph<&str, u64>,
    distances: &HashMap<NodeIndex, u64>,
    pq_handles: &HashMap<NodeIndex, Handle>,
) {
    println!("--- Step: {step_name} ---");

    // Collect items that are in the priority queue
    // We can see what's in the heap by looking at the keys in pq_handles
    let mut heap_items: Vec<(String, u64)> = pq_handles
        .keys()
        .map(|node_index| {
            let name = graph[*node_index].to_string();
            // The priority in the heap *should* be the one in our distances map
            let priority = distances[node_index];
            (name, priority)
        })
        .collect();

    // Sort by priority to make it look like a heap
    heap_items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    if heap_items.is_empty() {
        println!("  Heap is empty.");
    } else {
        println!("  Heap contents (Node: Priority):");
        for (name, priority) in heap_items {
            println!("    - {name}: {priority}");
        }
    }

    println!("  All known distances:");
    let mut all_nodes: Vec<NodeIndex> = graph.node_indices().collect();
    all_nodes.sort_by_key(|n| graph[*n]); // Sort by name for clean output
    for node in all_nodes {
        println!("    - {}: {}", graph[node], distances[&node]);
    }
    println!("---------------------------------\n");
}

/// Finds shortest paths from `start` to all other nodes using Dijkstra's algorithm,
/// powered by pie_core::FibHeap.
fn dijkstra_pie_core(
    graph: &Graph<&str, u64>,
    start: NodeIndex,
) -> HashMap<NodeIndex, u64> {
    
    // 1. Initialization
    let mut heap = FibHeap::new();    
    let mut distances = HashMap::new();
    let mut pq_handles = HashMap::new();

    for node in graph.node_indices() {
        distances.insert(node, u64::MAX);
    }
    distances.insert(start, 0);

    // Push start node
    let start_handle = heap.push(0, start);
    pq_handles.insert(start, start_handle);

    // 2. Main Loop
    while let Some((dist, node)) = heap.pop() {
        pq_handles.remove(&node);

        if dist > distances[&node] {
            continue; // Stale entry
        }

        // 3. Explore Neighbors (using petgraph::Graph)
        for edge in graph.edges(node) {
            let neighbor = edge.target();
            let length = *edge.weight();
            let new_dist = dist + length;
            // 4. The "Decrease Key" Moment
            if new_dist < distances[&neighbor] {
                distances.insert(neighbor, new_dist);
                
                if let Some(handle) = pq_handles.get(&neighbor) {
                    heap.decrease_key(*handle, new_dist).unwrap();
                } else {
                    let handle = heap.push(new_dist, neighbor);
                    pq_handles.insert(neighbor, handle);
                }
            }
            // Visualize the change
            let f = graph.node_weight(node).map(|s| s.to_string()).unwrap_or_default();
            let t = graph.node_weight(neighbor).map(|s| s.to_string()).unwrap_or_default();
            visualize_state(&format!("from {f} ({dist}) -> {t} ({length})"), graph, &distances, &pq_handles);
        }
    }
    distances
}

fn main() {
    // Create a petgraph::Graph
    let mut g = Graph::new();
    let a = g.add_node("A");
    let b = g.add_node("B");
    let c = g.add_node("C");
    let d = g.add_node("D");

    g.add_edge(a, b, 10);
    g.add_edge(a, c, 40);
    g.add_edge(b, c, 20);
    g.add_edge(b, d, 50);
    g.add_edge(c, d, 10);

    // Run your algorithm
    let distances = dijkstra_pie_core(&g, a);
    println!("Shortest distances from A:");
    for (node, dist) in distances.iter() {
        println!("{}: {}", g[*node], dist);
    }

    assert_eq!(distances[&a], 0);
    assert_eq!(distances[&b], 10);
    assert_eq!(distances[&c], 30); // A -> B -> C (10 + 20)
    assert_eq!(distances[&d], 40); // A -> B -> C -> D (10 + 20 + 10)
}
