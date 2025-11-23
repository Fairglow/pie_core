use pie_core::{ElemPool, PieList};
use std::fmt::Display;

/// A helper function to visualize the contents of a PieList.
///
/// NOTE: This test assumes an iterator API like `list.iter(&pool)`
/// exists. If your crate's API is different (e.g., `pool.iter(list)`
/// or `list.cursor_head(&pool)` and looping), you will need to
/// adjust this helper function to match.
fn visualize_list<T: Display>(list: &PieList<T>, pool: &ElemPool<T>) -> String {
    list.iter(pool).map(|item| item.to_string()).collect()
}

#[test]
fn real_world_editor_test() {
    // 1. SETUP: Create the shared memory pool for all our buffers.
    println!("--- Test Start: Editor Simulation ---");
    let mut pool = ElemPool::<char>::new();

    // 2. CREATE BUFFERS: Create two "file buffers" (lists).
    // They are lightweight handles and are both empty.
    let mut file_a = PieList::new(&mut pool);
    let mut file_b = PieList::new(&mut pool);

    println!("Pool starting len: {}", pool.len());
    assert_eq!(pool.len(), 0);
    assert_eq!(file_a.len(), 0);
    assert_eq!(file_b.len(), 0);

    // 3. SCENARIO: "Typing" into File A
    // We append characters to the first file.
    file_a.push_back('L', &mut pool).unwrap();
    file_a.push_back('i', &mut pool).unwrap();
    file_a.push_back('n', &mut pool).unwrap();
    file_a.push_back('e', &mut pool).unwrap();
    file_a.push_back(' ', &mut pool).unwrap();
    file_a.push_back('1', &mut pool).unwrap();
    file_a.push_back('\n', &mut pool).unwrap();
    file_a.push_back('L', &mut pool).unwrap();
    file_a.push_back('i', &mut pool).unwrap();
    file_a.push_back('n', &mut pool).unwrap();
    file_a.push_back('e', &mut pool).unwrap();
    file_a.push_back(' ', &mut pool).unwrap();
    file_a.push_back('2', &mut pool).unwrap();
    file_a.push_back('\n', &mut pool).unwrap();

    let viz_a = visualize_list(&file_a, &pool);
    println!("File A content:\n{}", viz_a);
    assert_eq!(viz_a, "Line 1\nLine 2\n");
    assert_eq!(file_a.len(), 14);
    assert_eq!(pool.len(), 14); // Pool now contains 14 chars

    // 4. SCENARIO: "Typing" into File B
    // We append characters to the *second* file.
    // This demonstrates the shared pool.
    file_b.push_back('H', &mut pool).unwrap();
    file_b.push_back('e', &mut pool).unwrap();
    file_b.push_back('l', &mut pool).unwrap();
    file_b.push_back('l', &mut pool).unwrap();
    file_b.push_back('o', &mut pool).unwrap();
    file_b.push_back('\n', &mut pool).unwrap();

    let viz_b = visualize_list(&file_b, &pool);
    println!("File B content:\n{}", viz_b);
    assert_eq!(viz_b, "Hello\n");
    assert_eq!(file_b.len(), 6);
    assert_eq!(pool.len(), 20); // Pool total is 14 + 6

    // 5. SCENARIO: "Cut" line 1 from File A
    // This is the core test of the `split_before` operation.
    // We will move a cursor to the start of "Line 2" and split the list.

    // Get a cursor pointing to the head of File A
    let mut cursor_a = file_a.cursor_mut(&mut pool);

    // Move 7 positions to the right (to the 'L' of "Line 2")
    for _ in 0..7 {
        cursor_a.move_next(&mut pool);
    }

    // Verify cursor is where we expect
    assert_eq!(cursor_a.peek(&pool), Some(&'L'));

    // Perform the "cut". This splits `file_a` at the cursor.
    // `file_a` retains the elements *before* the cursor ("Line 1\n").
    // `cut_buffer` becomes a new list with elements *from* the
    // cursor onward ("Line 2\n"). This is an O(1) operation.
    let mut cut_buffer = cursor_a.split_before(&mut pool).unwrap();

    // --- VERIFICATION AND VISUALIZATION after CUT ---
    let viz_a_after_cut = visualize_list(&file_a, &pool);
    let viz_cut = visualize_list(&cut_buffer, &pool);

    println!("File A after cut:\n{}", viz_a_after_cut);
    println!("Cut buffer content:\n{}", viz_cut);

    // Verify File A
    assert_eq!(viz_a_after_cut, "Line 2\n");
    assert_eq!(file_a.len(), 7);

    // Verify the new "cut" list
    assert_eq!(viz_cut, "Line 1\n");
    assert_eq!(cut_buffer.len(), 7);

    // Verify the pool. No elements were added or removed,
    // just moved from one list handle to another.
    assert_eq!(pool.len(), 20);

    // 6. SCENARIO: "Paste" the cut buffer into File B
    // We will paste the `cut_buffer` at the *beginning* of File B.
    // This tests the `splice_before` operation.

    // Get a cursor pointing to the head of File B
    let mut cursor_b = file_b.cursor_mut(&mut pool);

    // Splice the *entire* `cut_buffer` into `file_b` before
    // the cursor's current position. This is also O(1).
    // `cut_buffer` will be empty after this.
    cursor_b.splice_before(&mut cut_buffer, &mut pool).unwrap();

    // --- VERIFICATION AND VISUALIZATION after PASTE ---
    let viz_b_after_paste = visualize_list(&file_b, &pool);

    println!("File B after paste:\n{}", viz_b_after_paste);

    // Verify File B
    let expected_b = "Line 1\nHello\n";
    assert_eq!(viz_b_after_paste, expected_b);
    assert_eq!(file_b.len(), 13); // 6 original + 7 pasted

    // Verify the pool again. Still 20 total elements.
    assert_eq!(pool.len(), 20);

    // Verify the cut buffer is now empty
    assert!(cut_buffer.is_empty());

    // --- CLEANUP ---
    // Since we are dropping the pool immediately after this, we tell the
    // lists to ignore the leak check.
    file_a.set_leak_check(false);
    file_b.set_leak_check(false);

    println!("--- Test Complete ---");
}
