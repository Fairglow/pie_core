#![cfg(feature = "petgraph")]
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use crate::{FibHeap, Index, Node};
use std::collections::HashMap;

type Handle = Index<Node<u64, NodeIndex>>;

/// Finds shortest paths from `start` to all other nodes using Dijkstra's algorithm,
/// powered by pie_core::FibHeap.
pub fn dijkstra_pie_core(
    graph: &Graph<&str, u64>,
    start: NodeIndex,
) -> HashMap<NodeIndex, u64> {
    // 1. Initialization
    let node_count = graph.node_count();
    let mut heap = FibHeap::new();    
    let mut distances = vec![u64::MAX; node_count];
    let mut pq_handles: Vec<Option<Handle>> = vec![None; node_count];
    distances[start.index()] = 0;
    let start_handle = heap.push(0, start);
    pq_handles[start.index()] = Some(start_handle);
    // 2. Main Loop
    while let Some((dist, node)) = heap.pop() {
        pq_handles[node.index()] = None; // Node is visited
        if dist > distances[node.index()] {
            continue; // Stale entry
        }
        // 3. Explore Neighbors (using petgraph::Graph)
        for edge in graph.edges(node) {
            let neighbor = edge.target();
            let length = *edge.weight();
            let new_dist = dist + length;
            // 4. The "Decrease Key" Moment
            let current_dist = distances[neighbor.index()];
            if new_dist < current_dist {
                distances[neighbor.index()] = new_dist;
                if let Some(handle) = pq_handles[neighbor.index()] {
                    heap.decrease_key(handle, new_dist).unwrap();
                } else {
                    let handle = heap.push(new_dist, neighbor);
                    pq_handles[neighbor.index()] = Some(handle);
                }
            }
        }
    }
    let mut final_distances = HashMap::with_capacity(node_count);
    for (i, dist) in distances.into_iter().enumerate() {
        if dist != u64::MAX {
            final_distances.insert(NodeIndex::new(i), dist);
        }
    }
    final_distances
}
