//! Timing of the editing engine on a large document.
//!
//! Not part of the regular suite:
//! `cargo test -p gpui-editor-core --release --test perf -- --ignored --nocapture`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use gpui_editor_core::{Editor, Node, Pos, Selection};

/// About 2,000 text blocks: paragraphs, nested lists, tasks and quotes.
fn large_markdown() -> String {
    let mut out = String::new();
    for section in 0..200 {
        writeln!(out, "## Section {section}\n").unwrap();
        out.push_str(
            "Some **bold** text, some *italic* text and a [link](https://example.com).\n\n",
        );
        out.push_str("- First item\n- Second item\n  - Nested item\n\n");
        out.push_str("- [ ] A task\n- [x] Done\n\n");
        out.push_str("> A quote\n\n");
        out.push_str("Closing paragraph of the section.\n\n");
    }
    out
}

fn time(label: &str, run: impl FnOnce()) -> Duration {
    let start = Instant::now();
    run();
    let elapsed = start.elapsed();
    println!("{label:<28} {elapsed:>12.2?}");
    elapsed
}

#[test]
#[ignore = "timing, run explicitly with --ignored --nocapture"]
fn large_document() {
    let doc = Node::from_markdown(&large_markdown());
    let mut editor = Editor::new(doc);
    let blocks = editor.block_count();
    println!("text blocks: {blocks}");
    let middle = Pos {
        block: blocks / 2,
        offset: 0,
    };

    time("500 x insert_text", || {
        editor.set_selection(Selection::caret(middle));
        for _ in 0..500 {
            editor.insert_text("a");
        }
    });
    time("200 x split_block", || {
        for _ in 0..200 {
            editor.split_block();
        }
    });
    time("2000 x move_right", || {
        for _ in 0..2000 {
            editor.move_right(false);
        }
    });
    time("100 x undo + 100 x redo", || {
        for _ in 0..100 {
            editor.undo();
        }
        for _ in 0..100 {
            editor.redo();
        }
    });
    time("block_text on every block", || {
        let mut total = 0;
        for block in 0..editor.block_count() {
            total += editor.block_text(block).len();
        }
        assert!(total > 0);
    });
}
