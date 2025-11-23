//! A "Piece Table" text editor simulation using `pie_core::PieList`.
//!
//! This example demonstrates how `PieList` can be used to implement a "piece table,"
//! a data structure commonly used in text editors for efficient, non-destructive
//! text manipulation.
//!
//! --- The Piece Table Concept ---
//! A document is represented as a sequence of "pieces." Each piece is a reference
//! to a span of text in an underlying, immutable buffer. All new text is appended
//! to a separate "add buffer."
//!
//! Edits don't modify the original text; they only change the list of pieces.
//!
//! - **Insertion**: To insert text, we find the piece where the insertion occurs,
//!   split it into two, and insert a new piece between them that points to the
//!   newly added text in the "add buffer."
//!
//! - **Deletion**: To delete text, we simply remove or shorten the corresponding pieces
//!   from the list.
//!
//! `PieList` is a perfect fit for this because its `CursorMut` API allows for
//! efficient O(1) splitting and splicing of the list, and its arena-based memory
//! means creating and managing many small `Piece` objects is extremely fast.

use pie_core::{ElemPool, PieList};

// The buffers containing our text.
// In a real editor, the ADD_BUFFER would be a dynamically growing string.
const ORIGINAL_TEXT: &str = "the world is a fine place and worth fighting for.";
const ADD_BUFFER: &str = "beautiful ";

/// Identifies which buffer a `Piece` refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Buffer {
    Original,
    Add,
}

/// A `Piece` is a small struct representing a span of text.
/// It must be `Copy` to be used with `pie_core`.
#[derive(Debug, Clone, Copy)]
struct Piece {
    buffer: Buffer,
    start: usize,
    len: usize,
}

impl Piece {
    /// Helper to get the text this piece represents.
    fn get_text<'a>(&self, original: &'a str, add: &'a str) -> &'a str {
        let text = match self.buffer {
            Buffer::Original => original,
            Buffer::Add => add,
        };
        &text[self.start..self.start + self.len]
    }
}

/// A helper to print the entire document by iterating through the `PieList`.
fn print_document(
    list: &PieList<Piece>,
    pool: &ElemPool<Piece>,
    original: &str,
    add: &str,
) {
    print!("    Reconstructed Text: \"" );
    for piece in list.iter(pool) {
        print!("{}", piece.get_text(original, add));
    }
    println!("\"");
}

/// A helper to print the internal state of the `PieList` itself.
fn print_list_state(list: &PieList<Piece>, pool: &ElemPool<Piece>) {
    println!("    Internal PieList state:");
    print!("      [");
    let mut first = true;
    for piece in list.iter(pool) {
        if !first {
            print!( ", ");
        }
        print!(
            "Piece({:?}, {}, {})",
            piece.buffer,
            piece.start,
            piece.len
        );
        first = false;
    }
    println!("]");
}

fn main() {
    println!("---\n--- PieList Text Editor Simulation ---\n");

    // 1. Create a shared ElemPool to manage memory for all our `Piece` objects.
    let mut pool = ElemPool::<Piece>::new();

    // 2. Create the main list representing our document.
    let mut document = PieList::new(&mut pool);

    // --- 1. Initial State ---
    // The document starts as a single piece covering the entire original text.
    println!("1. Initial State");
    let initial_piece = Piece {
        buffer: Buffer::Original,
        start: 0,
        len: ORIGINAL_TEXT.len(),
    };
    document.push_back(initial_piece, &mut pool).unwrap();

    print_document(&document, &pool, ORIGINAL_TEXT, ADD_BUFFER);
    print_list_state(&document, &pool);
    println!("   A single piece points to the entire original document.\n");

    // --- 2. Insertion by splitting a piece ---
    // We want to insert "beautiful " into "the world...".
    // This will happen after "the ".
    // Final text: "the beautiful world is a fine place..."
    println!("2. Insertion: Adding 'beautiful ' after 'the '");
    println!("   ACTION: Manually split the first piece and insert a new one.");

    let insertion_offset = 4; // After "the "
    let text_to_insert = ADD_BUFFER;

    // To do this, we get a cursor to the piece we want to modify.
    let mut cursor = document.cursor_mut(&mut pool);

    // Get the data for the piece we are splitting.
    let original_piece = *cursor.peek(&pool).unwrap();

    // Create the three pieces that will replace the original one.
    let before_piece = Piece {
        len: insertion_offset,
        ..original_piece
    };
    let after_piece = Piece {
        start: original_piece.start + insertion_offset,
        len: original_piece.len - insertion_offset,
        ..original_piece
    };
    let add_piece = Piece {
        buffer: Buffer::Add,
        start: 0,
        len: text_to_insert.len(),
    };

    // 1. Modify the current piece to be the `before` part.
    *cursor.peek_mut(&mut pool).unwrap() = before_piece;

    // 2. Insert the `add` and `after` pieces.
    //    `insert_after` places the new element immediately after the cursor
    //    and does NOT move the cursor.
    cursor.insert_after(after_piece, &mut pool).unwrap();
    cursor.insert_after(add_piece, &mut pool).unwrap();

    print_document(&document, &pool, ORIGINAL_TEXT, ADD_BUFFER);
    print_list_state(&document, &pool);
    println!("   The list is now three pieces. No original text was copied or moved.\n");

    // --- 3. Deletion ---
    // Let's delete " and worth fighting for".
    // This text is at the end of the original piece that was split earlier.
    println!("3. Deletion: Removing ' and worth fighting for'");
    println!("   ACTION: Find the piece and shorten it.");

    // Get a cursor to the last piece.
    let mut cursor = document
        .cursor_mut_at(document.len() - 1, &mut pool)
        .unwrap();

    // The piece to delete is at the end of the last piece in the list.
    let piece_to_shorten = cursor.peek(&pool).unwrap();
    let deletion_len = " and worth fighting for".len();
    let new_len = piece_to_shorten.len - deletion_len;

    // We can just shorten the piece by modifying it in place.
    cursor.peek_mut(&mut pool).unwrap().len = new_len;

    // What if we wanted to remove a whole piece? Let's delete "beautiful ".
    println!("\n   ACTION: Now, remove the middle piece ('beautiful ').");
    // Move cursor to the middle piece (it was at the end).
    cursor.move_prev(&mut pool);
    let removed_piece = cursor.remove_current(&mut pool).unwrap();

    print_document(&document, &pool, ORIGINAL_TEXT, ADD_BUFFER);
    print_list_state(&document, &pool);
    println!("   The middle piece was removed (O(1)). The list is back to two pieces.\n");

    // --- 4. Undo using splice_before ---
    // The `splice_before` operation allows moving a whole sequence of nodes from
    // one list to another in O(1) time. This is perfect for undo/redo stacks.
    println!("4. Undo: Use a second list and `splice_before` to restore the deleted piece.");

    // Create a second list to act as our "undo buffer", using the SAME pool.
    let mut undo_buffer = PieList::new(&mut pool);
    undo_buffer.push_back(removed_piece, &mut pool).unwrap();

    println!("   The 'undo_buffer' now holds the piece we removed.");
    print!("    Undo Buffer State: ");
    print!("[Piece({:?}, {}, {})]",
        undo_buffer.front(&pool).unwrap().buffer,
        undo_buffer.front(&pool).unwrap().start,
        undo_buffer.front(&pool).unwrap().len);
    println!();

    println!("\n   ACTION: Splice the undo_buffer back into the document.");

    // Get a cursor and move it to the insertion point (before the second piece).
    let mut cursor = document.cursor_mut(&mut pool);
    cursor.move_next(&mut pool);

    // Splice the entire `undo_buffer` list into the `document` list before the cursor.
    // This is the magic operation: it moves all nodes from one list to another
    // without any allocation or per-node copying.
    cursor.splice_before(&mut undo_buffer, &mut pool).unwrap();
    assert!(undo_buffer.is_empty()); // The undo buffer is now empty.

    print_document(&document, &pool, ORIGINAL_TEXT, ADD_BUFFER);
    print_list_state(&document, &pool);
    println!("   The 'beautiful ' piece is back! The splice was a single O(1) operation.");

    // No need for leak detection, now that we're dropping the pool too
    document.set_leak_check(false);
    println!("\n--- Simulation complete ---");
}
