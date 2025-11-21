#![cfg(feature = "serde")]

use pie_core::{ElemPool, FibHeap, PieList};
use serde::{Deserialize, Serialize};

/// A wrapper struct simulating a game state or document model.
/// This is necessary for PieList because the Pool and List are separate.
#[derive(Serialize, Deserialize)]
struct WorldState {
    pool: ElemPool<String>,
    active_quests: PieList<String>,
    completed_quests: PieList<String>,
}

#[test]
fn test_pielist_roundtrip() {
    let mut pool = ElemPool::new();
    let mut list = PieList::new(&mut pool);
    let compl = PieList::new(&mut pool);

    list.push_back("Alpha".to_string(), &mut pool).unwrap();
    list.push_back("Beta".to_string(), &mut pool).unwrap();
    list.push_back("Gamma".to_string(), &mut pool).unwrap();

    // Create a struct to hold both
    let _original_state = WorldState {
        pool: pool, // Ownership moves here
        active_quests: list,
        completed_quests: compl,
    };
    // Fix the hacky init above for the test logic:
    // (We need a fresh empty list for the second field)
    // Let's rebuild strictly:
    let mut pool = ElemPool::new();
    let mut l1 = PieList::new(&mut pool);
    l1.push_back("Alpha".to_string(), &mut pool).unwrap();
    l1.push_back("Beta".to_string(), &mut pool).unwrap();

    let mut l2 = PieList::new(&mut pool);
    l2.push_back("Done".to_string(), &mut pool).unwrap();

    let state = WorldState {
        pool,
        active_quests: l1,
        completed_quests: l2,
    };

    // 1. Serialize to JSON
    let json = serde_json::to_string(&state).expect("Failed to serialize");

    // 2. Deserialize
    let mut loaded_state: WorldState = serde_json::from_str(&json).expect("Failed to deserialize");

    // 3. Verify Data
    let active: Vec<&String> = loaded_state.active_quests.iter(&loaded_state.pool).collect();
    assert_eq!(active, vec!["Alpha", "Beta"]);

    let completed: Vec<&String> = loaded_state.completed_quests.iter(&loaded_state.pool).collect();
    assert_eq!(completed, vec!["Done"]);

    // 4. Verify Operational Integrity (Can we modify the loaded state?)
    loaded_state.active_quests.push_back("Delta".to_string(), &mut loaded_state.pool).unwrap();
    assert_eq!(loaded_state.active_quests.len(), 3);
    assert_eq!(*loaded_state.active_quests.back(&loaded_state.pool).unwrap(), "Delta");
}

#[test]
fn test_fibheap_roundtrip() {
    // FibHeap is self-contained (owns its pool), so we can serialize it directly.
    let mut heap = FibHeap::new();
    heap.push(10, "Low Priority");
    heap.push(1, "High Priority");
    heap.push(5, "Medium Priority");

    // 1. Serialize
    let json = serde_json::to_string(&heap).expect("Failed to serialize heap");

    // 2. Deserialize
    let mut loaded_heap: FibHeap<i32, String> = serde_json::from_str(&json).expect("Failed to deserialize heap");

    // 3. Verify Integrity
    assert_eq!(loaded_heap.len(), 3);

    // Pop elements to ensure the tree structure and priority logic were preserved
    let (k1, v1) = loaded_heap.pop().unwrap();
    assert_eq!(k1, 1);
    assert_eq!(v1, "High Priority");

    let (k2, v2) = loaded_heap.pop().unwrap();
    assert_eq!(k2, 5);
    assert_eq!(v2, "Medium Priority");

    // 4. Verify we can add new items to the loaded heap
    loaded_heap.push(2, "New Item".to_string());
    assert_eq!(loaded_heap.peek().unwrap().0, &2);
}

#[test]
fn test_shrink_before_serialize() {
    // This tests the recommended workflow: Shrink -> Remap -> Save
    let mut pool = ElemPool::new();
    let mut list = PieList::new(&mut pool);

    // Create fragmentation
    list.push_back(10, &mut pool).unwrap();
    let _idx_remove = list.push_back(20, &mut pool).unwrap(); // This will be removed
    list.push_back(30, &mut pool).unwrap();

    // Remove item 20
    list.pop_front(&mut pool); // Removes 10
    // Actually let's use the specific removal logic to ensure a hole in the middle
    // Reset:
    list.clear(&mut pool);
    list.push_back(100, &mut pool).unwrap(); // Index 2 (1 is sentinel)
    let _idx_rm = list.push_back(200, &mut pool).unwrap(); // Index 3
    list.push_back(300, &mut pool).unwrap(); // Index 4

    // Pop 200 (Index 3). Now [100, (Free), 300]
    // To do this cleanly without a remove_at in public API, we swap remove logic:
    // (Simulate hole creation via pop for this test's simplicity)
    list.pop_front(&mut pool); // Removes 100. [ (Free), 200, 300 ]

    let before_cap = pool.capacity();

    // 1. Shrink & Remap
    let map = pool.shrink_to_fit();
    list.remap(&map);

    // 2. Serialize
    // We need a wrapper because PieList isn't self-contained
    #[derive(Serialize, Deserialize)]
    struct Container { p: ElemPool<i32>, l: PieList<i32> }

    let container = Container { p: pool, l: list };
    let json = serde_json::to_string(&container).unwrap();

    // 3. Deserialize
    let loaded: Container = serde_json::from_str(&json).unwrap();

    // 4. Verify
    // The loaded pool should be compact (capacity roughly equals len)
    assert!(loaded.p.capacity() < before_cap);

    let vec: Vec<_> = loaded.l.iter(&loaded.p).copied().collect();
    assert_eq!(vec, vec![200, 300]);
}
