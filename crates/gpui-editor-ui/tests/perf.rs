//! Interaction cost on large documents, rendering included, and a complexity guard.
//!
//! Each interaction is followed by `run_until_parked`, so the time includes the editor's update,
//! the layout of every block and the paint. The guard checks that typing in a document 4x larger
//! costs at most 10x more (linear: 4x to 7x here, quadratic: about 16x). It only runs in release
//! builds:
//! `cargo test -p gpui-editor-ui --release --test perf -- --nocapture`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use gpui::{AnyWindowHandle, ClipboardItem, TestAppContext, WindowHandle};
use gpui_editor_core::{Node, Pos};
use gpui_editor_ui::{EditorTheme, EditorView, bind_keys};

const SIZES: [usize; 3] = [100, 500, 2_000];

/// A document of about `blocks` text blocks (sections of 9: heading, marks, lists, tasks, quote).
fn document(blocks: usize) -> Node {
    let mut out = String::new();
    for section in 0..blocks.div_ceil(9) {
        writeln!(out, "## Section {section}\n").unwrap();
        out.push_str(
            "Some **bold** text, some *italic* text and a [link](https://example.com).\n\n",
        );
        out.push_str("- First item\n- Second item\n  - Nested item\n\n");
        out.push_str("- [ ] A task\n- [x] Done\n\n> A quote\n\nClosing paragraph.\n\n");
    }
    Node::from_markdown(&out)
}

fn open(cx: &mut TestAppContext, node: Node) -> WindowHandle<EditorView> {
    cx.update(bind_keys);
    let window = cx.add_window(|_, cx| EditorView::new(node, EditorTheme::default(), cx));
    window
        .update(cx, |view, window, cx| view.focus(window, cx))
        .unwrap();
    cx.run_until_parked();
    window
}

fn caret_to_middle(cx: &mut TestAppContext, window: WindowHandle<EditorView>) {
    window
        .update(cx, |view, _, cx| {
            let block = view.editor().block_count() / 2;
            view.set_caret_for_tests(Pos { block, offset: 0 }, cx);
        })
        .unwrap();
    cx.run_until_parked();
}

/// Median time per repetition of `step` (each followed by a full update and repaint).
fn time(cx: &mut TestAppContext, reps: u32, mut step: impl FnMut(&mut TestAppContext)) -> Duration {
    let mut trials: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..reps {
                step(cx);
                cx.run_until_parked();
            }
            start.elapsed() / reps
        })
        .collect();
    trials.sort();
    trials[2]
}

struct Row {
    blocks: usize,
    keystroke: Duration,
    caret: Duration,
    doc_end: Duration,
    slash_menu: Duration,
    paste: Duration,
}

fn measure(cx: &mut TestAppContext, blocks: usize) -> Row {
    let window = open(cx, document(blocks));
    let any: AnyWindowHandle = window.into();
    caret_to_middle(cx, window);
    let keystroke = time(cx, 20, |cx| {
        cx.simulate_input(any, "x");
    });
    let caret = time(cx, 20, |cx| {
        cx.simulate_keystrokes(any, "right");
    });
    let doc_end = time(cx, 4, |cx| {
        cx.simulate_keystrokes(any, "ctrl-end ctrl-home");
    });
    caret_to_middle(cx, window);
    cx.simulate_keystrokes(any, "enter");
    let slash_menu = time(cx, 10, |cx| {
        cx.simulate_input(any, "/");
        cx.simulate_keystrokes(any, "backspace");
    });
    let mut lines = String::new();
    for i in 0..50 {
        writeln!(lines, "Pasted line {i}").unwrap();
    }
    let paste = time(cx, 3, |cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(lines.clone()));
        cx.simulate_keystrokes(any, "ctrl-v");
    });
    Row {
        blocks,
        keystroke,
        caret,
        doc_end,
        slash_menu,
        paste,
    }
}

#[gpui::test]
#[cfg_attr(debug_assertions, ignore = "timing: run with --release")]
fn interactions_on_large_documents(cx: &mut TestAppContext) {
    let rows: Vec<Row> = SIZES.iter().map(|&blocks| measure(cx, blocks)).collect();
    println!("blocks   keystroke       caret   end+home   / menu   paste 50 lines");
    for row in &rows {
        println!(
            "{:>6} {:>11.2?} {:>11.2?} {:>10.2?} {:>8.2?} {:>15.2?}",
            row.blocks, row.keystroke, row.caret, row.doc_end, row.slash_menu, row.paste
        );
    }
    let (medium, large) = (&rows[1], &rows[2]);
    let ratio = large.keystroke.as_secs_f64() / medium.keystroke.as_secs_f64();
    println!("keystroke: x{ratio:.2} for 4x the blocks");
    assert!(
        ratio <= 10.0,
        "typing grows faster than linearly: x{ratio:.2}"
    );
}
