//! Complexity guards: an operation on a document 4x larger must not cost more than about 4x
//! (linear operations) or about 1x (operations that only touch the caret's block).
//!
//! Run them one at a time (`--test-threads=1`) so that they do not compete for the CPU.
//!
//! Absolute times depend on the machine; the ratio between two sizes does not, so these tests can
//! run in CI. A linear operation gives a ratio near 4, a quadratic one near 16. They only run in
//! release builds, where the debug consistency checks of the caches are off:
//! `cargo test -p gpui-editor-core --release --test scaling -- --nocapture --test-threads=1`

#[path = "../benches/common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use gpui_editor_core::{Editor, Kind, Node, Pos, Selection};

/// Both sizes are past the CPU caches, so that the ratio reflects the algorithm, not the memory.
const SMALL: usize = 4_000;
const LARGE: usize = 16_000;
const TRIALS: usize = 7;
/// Largest accepted ratio for an operation that may walk the whole document: a linear cost
/// measures between 4 and 7 here (memory-bound), a quadratic one about 16.
const LINEAR: f64 = 10.0;
/// Largest accepted ratio for an operation that should not depend on the document size.
const CONSTANT: f64 = 3.0;

fn editor_at_middle(blocks: usize) -> Editor {
    let mut editor = Editor::new(Node::from_markdown(&common::markdown(blocks)));
    let block = editor.block_count() / 2;
    editor.set_selection(Selection::caret(Pos { block, offset: 0 }));
    editor
}

/// Median over [`TRIALS`] trials (after one warm-up) of the time per call reported by `trial`.
fn median(mut trial: impl FnMut() -> Duration) -> Duration {
    trial();
    let mut times: Vec<Duration> = (0..TRIALS).map(|_| trial()).collect();
    times.sort();
    times[TRIALS / 2]
}

/// Time per call of `op` on `reps` fresh copies of `editor`, the copies being made beforehand.
fn on_copies(editor: &Editor, reps: u32, op: &impl Fn(&mut Editor)) -> Duration {
    median(|| {
        let mut copies: Vec<Editor> = (0..reps).map(|_| editor.clone()).collect();
        let start = Instant::now();
        for copy in &mut copies {
            op(copy);
        }
        start.elapsed() / reps
    })
}

/// Time per call of `op`, repeated `reps` times on the same editor.
fn in_place(editor: &mut Editor, reps: u32, op: &impl Fn(&mut Editor)) -> Duration {
    median(|| {
        let start = Instant::now();
        for _ in 0..reps {
            op(editor);
        }
        start.elapsed() / reps
    })
}

struct Report {
    failures: Vec<String>,
}

impl Report {
    fn check(&mut self, name: &str, limit: f64, small: Duration, large: Duration) {
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-9);
        let verdict = if ratio <= limit { "ok" } else { "TOO SLOW" };
        println!(
            "{name:<22} {small:>11.2?} {large:>11.2?}   x{ratio:>5.2} (max x{limit}) {verdict}"
        );
        if ratio > limit {
            self.failures.push(format!(
                "{name}: x{ratio:.2} for 4x the blocks (max x{limit})"
            ));
        }
    }
}

type EditOp = Box<dyn Fn(&mut Editor)>;
type Run<'a> = Box<dyn Fn() + 'a>;

fn editing_ops() -> Vec<(&'static str, EditOp)> {
    vec![
        ("type_first_char", Box::new(|e| e.insert_text("a"))),
        ("split_block", Box::new(Editor::split_block)),
        ("join_blocks", Box::new(Editor::delete_backward)),
        ("toggle_list", Box::new(|e| e.toggle_list(Kind::BulletList))),
        (
            "delete_all",
            Box::new(|e| {
                e.select_all();
                e.delete_backward();
            }),
        ),
    ]
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timing: run with --release")]
fn editing_scales_linearly_at_most() {
    let (small, large) = (editor_at_middle(SMALL), editor_at_middle(LARGE));
    let mut report = Report {
        failures: Vec::new(),
    };
    println!("operation                {SMALL:>8} blk {LARGE:>8} blk    ratio");
    for (name, op) in editing_ops() {
        let reps = 6;
        report.check(
            name,
            LINEAR,
            on_copies(&small, reps, &op),
            on_copies(&large, reps, &op),
        );
    }
    let undo_redo = |e: &mut Editor| {
        e.undo();
        e.redo();
    };
    let (mut small_edited, mut large_edited) = (small.clone(), large.clone());
    small_edited.split_block();
    large_edited.split_block();
    report.check(
        "undo_redo",
        LINEAR,
        in_place(&mut small_edited, 10, &undo_redo),
        in_place(&mut large_edited, 10, &undo_redo),
    );

    // Operations that only touch the caret's block.
    let (mut small_typing, mut large_typing) = (small.clone(), large.clone());
    small_typing.insert_text("a");
    large_typing.insert_text("a");
    // On copies, so that the block does not grow from one measurement to the next.
    let type_next = |e: &mut Editor| e.insert_text("b");
    report.check(
        "type_next_char",
        CONSTANT,
        on_copies(&small_typing, 30, &type_next),
        on_copies(&large_typing, 30, &type_next),
    );
    let move_right_left = |e: &mut Editor| {
        e.move_right(false);
        e.move_left(false);
    };
    let (mut small_moving, mut large_moving) = (small.clone(), large.clone());
    report.check(
        "move_right_left",
        CONSTANT,
        in_place(&mut small_moving, 2_000, &move_right_left),
        in_place(&mut large_moving, 2_000, &move_right_left),
    );
    let middle_text = |e: &mut Editor| {
        std::hint::black_box(e.block_text(e.selection().head.block));
    };
    report.check(
        "block_text_one",
        CONSTANT,
        in_place(&mut small_moving, 2_000, &middle_text),
        in_place(&mut large_moving, 2_000, &middle_text),
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timing: run with --release")]
fn reading_and_writing_scale_linearly_at_most() {
    let (small_md, large_md) = (common::markdown(SMALL), common::markdown(LARGE));
    let (small, large) = (
        Node::from_markdown(&small_md),
        Node::from_markdown(&large_md),
    );
    let (small_json, large_json) = (small.to_json_string(), large.to_json_string());
    let mut report = Report {
        failures: Vec::new(),
    };
    let time = |run: &dyn Fn()| {
        median(|| {
            let start = Instant::now();
            for _ in 0..5 {
                run();
            }
            start.elapsed() / 5
        })
    };
    let pairs: [(&str, Run, Run); 6] = [
        (
            "parse_json",
            Box::new(|| drop(Node::from_json_str(&small_json).unwrap())),
            Box::new(|| drop(Node::from_json_str(&large_json).unwrap())),
        ),
        (
            "parse_markdown",
            Box::new(|| drop(Node::from_markdown(&small_md))),
            Box::new(|| drop(Node::from_markdown(&large_md))),
        ),
        (
            "export_json",
            Box::new(|| drop(small.to_json_string())),
            Box::new(|| drop(large.to_json_string())),
        ),
        (
            "export_markdown",
            Box::new(|| drop(small.to_markdown())),
            Box::new(|| drop(large.to_markdown())),
        ),
        (
            "plain_text",
            Box::new(|| drop(small.plain_text())),
            Box::new(|| drop(large.plain_text())),
        ),
        (
            "open_editor",
            Box::new(|| {
                std::hint::black_box(Editor::new(small.clone()).block_count());
            }),
            Box::new(|| {
                std::hint::black_box(Editor::new(large.clone()).block_count());
            }),
        ),
    ];
    for (name, run_small, run_large) in &pairs {
        report.check(name, LINEAR, time(run_small), time(run_large));
    }
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
}
