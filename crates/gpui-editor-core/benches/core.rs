//! Benchmarks of the document model and the editing engine on documents of 100 to 5,000 blocks.
//!
//! `cargo bench -p gpui-editor-core` (add `-- <filter>` to run a subset, e.g. `-- edit/`).

mod common;

use std::hint::black_box;
use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use gpui_editor_core::{Editor, Flavor, Kind, MarkdownOptions, Node, Pos, Selection};

const SIZES: [usize; 3] = [100, 1_000, 5_000];

fn document(blocks: usize) -> Node {
    Node::from_markdown(&common::markdown(blocks))
}

/// An editor with its caret at the start of the middle block.
fn editor_at_middle(blocks: usize) -> Editor {
    let mut editor = Editor::new(document(blocks));
    let block = editor.block_count() / 2;
    editor.set_selection(Selection::caret(Pos { block, offset: 0 }));
    editor
}

fn parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for blocks in SIZES {
        let markdown = common::markdown(blocks);
        let json = Node::from_markdown(&markdown).to_json_string();
        group.bench_with_input(BenchmarkId::new("json", blocks), &json, |b, json| {
            b.iter(|| Node::from_json_str(black_box(json)).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("markdown", blocks), &markdown, |b, md| {
            b.iter(|| Node::from_markdown(black_box(md)));
        });
    }
    group.finish();
}

fn export(c: &mut Criterion) {
    let mut group = c.benchmark_group("export");
    let tiptap = MarkdownOptions {
        flavor: Flavor::TipTapCompat,
    };
    for blocks in SIZES {
        let doc = document(blocks);
        group.bench_with_input(BenchmarkId::new("json", blocks), &doc, |b, doc| {
            b.iter(|| black_box(doc).to_json_string());
        });
        group.bench_with_input(BenchmarkId::new("markdown", blocks), &doc, |b, doc| {
            b.iter(|| black_box(doc).to_markdown());
        });
        group.bench_with_input(
            BenchmarkId::new("markdown_tiptap", blocks),
            &doc,
            |b, doc| {
                b.iter(|| black_box(doc).to_markdown_with(&tiptap));
            },
        );
        group.bench_with_input(BenchmarkId::new("plain_text", blocks), &doc, |b, doc| {
            b.iter(|| black_box(doc).plain_text());
        });
    }
    group.finish();
}

/// Times `run` on a fresh copy of `editor` (the copy is made outside the measurement).
fn bench_edit(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    blocks: usize,
    editor: &Editor,
    run: impl Fn(&mut Editor),
) {
    group.bench_function(BenchmarkId::new(name, blocks), |b| {
        b.iter_batched_ref(|| editor.clone(), &run, BatchSize::LargeInput);
    });
}

fn edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("edit");
    for blocks in SIZES {
        let editor = editor_at_middle(blocks);
        bench_edit(&mut group, "type_first_char", blocks, &editor, |e| {
            e.insert_text("a");
        });
        let mut typing = editor.clone();
        typing.insert_text("a");
        bench_edit(&mut group, "type_next_char", blocks, &typing, |e| {
            e.insert_text("b");
        });
        bench_edit(
            &mut group,
            "split_block",
            blocks,
            &editor,
            Editor::split_block,
        );
        bench_edit(
            &mut group,
            "join_blocks",
            blocks,
            &editor,
            Editor::delete_backward,
        );
        bench_edit(&mut group, "toggle_list", blocks, &editor, |e| {
            e.toggle_list(Kind::BulletList);
        });
        let mut edited = editor.clone();
        edited.split_block();
        bench_edit(&mut group, "undo_redo", blocks, &edited, |e| {
            e.undo();
            e.redo();
        });
        bench_edit(&mut group, "delete_all", blocks, &editor, |e| {
            e.select_all();
            e.delete_backward();
        });
    }
    group.finish();
}

fn navigation(c: &mut Criterion) {
    let mut group = c.benchmark_group("navigation");
    for blocks in SIZES {
        let mut editor = editor_at_middle(blocks);
        group.bench_function(BenchmarkId::new("move_right_left", blocks), |b| {
            b.iter(|| {
                editor.move_right(false);
                editor.move_left(false);
            });
        });
        group.bench_function(BenchmarkId::new("move_word_right_left", blocks), |b| {
            b.iter(|| {
                editor.move_word_right(false);
                editor.move_word_left(false);
            });
        });
        group.bench_function(BenchmarkId::new("block_text_all", blocks), |b| {
            b.iter(|| {
                (0..editor.block_count())
                    .map(|block| editor.block_text(block).len())
                    .sum::<usize>()
            });
        });
    }
    group.finish();
}

fn config() -> Criterion {
    Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(1))
}

criterion_group! {
    name = benches;
    config = config();
    targets = parse, export, edit, navigation
}
criterion_main!(benches);
