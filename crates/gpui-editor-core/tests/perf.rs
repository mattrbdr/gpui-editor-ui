//! Timing of the editing engine on a large document.
//!
//! Not part of the regular suite:
//! `cargo test -p gpui-editor-core --release --test perf -- --ignored --nocapture`

#[path = "../benches/common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use gpui_editor_core::{Editor, Node, Pos, Selection};

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
    let doc = Node::from_markdown(&common::markdown(1_800));
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
