//! Property tests: random editing sequences must keep the document valid.
//!
//! The regular run checks 256 sequences; for a longer search:
//! `cargo test -p gpui-editor-core --release --test properties -- --ignored`

use gpui_editor_core::{Editor, Kind, MarkKind, Node, Pos, Selection};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

/// Starting documents: Markdown structures, an unknown block, a mention, an inline atom.
fn seeds() -> Vec<Node> {
    let markdown = [
        "",
        "Hello",
        "# Title\n\nSome **bold** and *italic* with a [link](https://example.com).",
        "- one\n- two\n  - nested\n    - deeper\n- three\n\n1. first\n2. second",
        "- [ ] todo\n- [x] done\n\n> quoted\n> lines\n\n```rust\nlet a = 1;\n```\n\n---\n\nEnd",
    ];
    let json = [
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"before"}]},{"type":"databaseView","attrs":{"viewId":"v1"}},{"type":"paragraph","content":[{"type":"text","text":"after"}]}]}"#,
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"See "},{"type":"mention","attrs":{"id":"doc:1","label":"Target"}},{"type":"text","text":" now"}]}]}"#,
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"},{"type":"emoji","attrs":{"name":"smile"}},{"type":"text","text":"b"},{"type":"hardBreak"},{"type":"text","text":"c"}]}]}"#,
    ];
    markdown
        .iter()
        .map(|source| Node::from_markdown(source))
        .chain(
            json.iter()
                .map(|source| Node::from_json_str(source).unwrap()),
        )
        .collect()
}

#[derive(Debug, Clone)]
enum Op {
    Insert(String),
    DeleteBackward,
    DeleteForward,
    Split,
    HardBreak,
    Move(u8, bool),
    Select(usize, usize, usize, usize),
    SelectAll,
    Mark(u8),
    Heading(u64),
    List(u8),
    Quote,
    CodeBlock,
    Indent,
    Outdent,
    ToggleTask,
    Link(Option<&'static str>),
    Undo,
    Redo,
}

fn text_piece() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("a".to_owned()),
        Just("é".to_owned()),
        Just("🇫🇷".to_owned()),
        Just(" ".to_owned()),
        Just("# ".to_owned()),
        Just("- ".to_owned()),
        Just("1. ".to_owned()),
        Just("[ ] ".to_owned()),
        Just("> ".to_owned()),
        Just("**".to_owned()),
        Just("*".to_owned()),
        Just("`".to_owned()),
        Just("~~".to_owned()),
        Just("[[".to_owned()),
        Just("---".to_owned()),
        Just("```".to_owned()),
        Just("word".to_owned()),
        Just("x\ny".to_owned()),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => text_piece().prop_map(Op::Insert),
        3 => Just(Op::DeleteBackward),
        2 => Just(Op::DeleteForward),
        3 => Just(Op::Split),
        1 => Just(Op::HardBreak),
        4 => (0u8..10, any::<bool>()).prop_map(|(kind, extend)| Op::Move(kind, extend)),
        3 => (0usize..12, 0usize..30, 0usize..12, 0usize..30)
            .prop_map(|(a, b, c, d)| Op::Select(a, b, c, d)),
        1 => Just(Op::SelectAll),
        2 => (0u8..5).prop_map(Op::Mark),
        1 => (1u64..4).prop_map(Op::Heading),
        2 => (0u8..3).prop_map(Op::List),
        1 => Just(Op::Quote),
        1 => Just(Op::CodeBlock),
        2 => Just(Op::Indent),
        2 => Just(Op::Outdent),
        1 => Just(Op::ToggleTask),
        1 => prop_oneof![Just(Some("https://example.com")), Just(None)].prop_map(Op::Link),
        2 => Just(Op::Undo),
        1 => Just(Op::Redo),
    ]
}

fn apply(editor: &mut Editor, op: &Op) {
    match op {
        Op::Insert(text) => editor.insert_text(text),
        Op::DeleteBackward => editor.delete_backward(),
        Op::DeleteForward => editor.delete_forward(),
        Op::Split => editor.split_block(),
        Op::HardBreak => editor.insert_hard_break(),
        Op::Move(kind, extend) => match kind {
            0 => editor.move_left(*extend),
            1 => editor.move_right(*extend),
            2 => editor.move_word_left(*extend),
            3 => editor.move_word_right(*extend),
            4 => editor.move_to_block_start(*extend),
            5 => editor.move_to_block_end(*extend),
            6 => editor.move_to_doc_start(*extend),
            7 => editor.move_to_doc_end(*extend),
            8 => editor.select_word_at(editor.selection().head),
            _ => editor.select_block(editor.selection().head.block),
        },
        Op::Select(a, b, c, d) => editor.set_selection(Selection {
            anchor: Pos {
                block: *a,
                offset: *b,
            },
            head: Pos {
                block: *c,
                offset: *d,
            },
        }),
        Op::SelectAll => editor.select_all(),
        Op::Mark(kind) => editor.toggle_mark(match kind {
            0 => MarkKind::Bold,
            1 => MarkKind::Italic,
            2 => MarkKind::Strike,
            3 => MarkKind::Underline,
            _ => MarkKind::Code,
        }),
        Op::Heading(level) => editor.toggle_heading(*level),
        Op::List(kind) => editor.toggle_list(match kind {
            0 => Kind::BulletList,
            1 => Kind::OrderedList,
            _ => Kind::TaskList,
        }),
        Op::Quote => editor.toggle_blockquote(),
        Op::CodeBlock => editor.toggle_code_block(),
        Op::Indent => editor.indent(),
        Op::Outdent => editor.outdent(),
        Op::ToggleTask => editor.toggle_task_checked(),
        Op::Link(href) => editor.set_link(*href),
        Op::Undo => editor.undo(),
        Op::Redo => editor.redo(),
    }
}

fn check_invariants(editor: &Editor, op: &Op) -> Result<(), TestCaseError> {
    let count = editor.block_count();
    prop_assert!(count >= 1, "no text block left after {op:?}");
    let selection = editor.selection();
    for pos in [selection.anchor, selection.head] {
        prop_assert!(
            pos.block < count,
            "{pos:?} past the last block after {op:?}"
        );
        prop_assert!(
            pos.offset <= editor.block_len(pos.block),
            "{pos:?} past the end of its block after {op:?}"
        );
    }
    let doc = editor.doc();
    let json = doc.to_json_string();
    let reread = Node::from_json_str(&json)
        .map_err(|error| TestCaseError::fail(format!("{error} after {op:?}: {json}")))?;
    prop_assert_eq!(&reread, doc, "JSON round trip after {:?}", op);
    let markdown = doc.to_markdown();
    let _ = Node::from_markdown(&markdown);
    let has_text = (0..count).any(|block| !editor.block_text(block).trim().is_empty());
    if has_text {
        // `plain_text` leaves custom nodes (mentions, views…) to the host: render them by name.
        let plain = doc.plain_text_with(
            &|node| matches!(node.kind, Kind::Other(_)).then(|| node.kind.name().to_owned()),
            &[],
        );
        prop_assert!(
            !plain.trim().is_empty(),
            "plain text is empty but blocks have text after {op:?}"
        );
    }
    Ok(())
}

fn run(seed: &Node, ops: &[Op]) -> Result<(), TestCaseError> {
    let mut editor = Editor::new(seed.clone());
    let original = editor.doc().clone();
    for op in ops {
        apply(&mut editor, op);
        check_invariants(&editor, op)?;
    }
    while editor.can_undo() {
        editor.undo();
    }
    prop_assert_eq!(
        editor.doc(),
        &original,
        "undoing everything after {:?}",
        ops
    );
    Ok(())
}

/// A runner over `cases` cases. The regular runs (256 cases) use a fixed seed, so that CI always
/// checks the same sequences; the long searches draw new ones each time.
fn runner(cases: u32) -> TestRunner {
    let config = Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    };
    if cases <= 256 {
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha))
    } else {
        TestRunner::new(config)
    }
}

fn search(cases: u32) {
    let seeds = seeds();
    let mut runner = runner(cases);
    let strategy = (0..seeds.len(), prop::collection::vec(op(), 1..60));
    runner
        .run(&strategy, |(seed, ops)| run(&seeds[seed], &ops))
        .unwrap();
}

#[test]
fn random_edits_keep_the_document_valid() {
    search(256);
}

#[test]
#[ignore = "long search, run explicitly with --ignored"]
fn random_edits_keep_the_document_valid_long() {
    search(10_000);
}

// --- Importers: arbitrary input ---

/// Markdown-heavy text: block and inline syntax, entities, HTML, accents and emoji.
fn markdown_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("# ".to_owned()),
        Just("## ".to_owned()),
        Just("- ".to_owned()),
        Just("1. ".to_owned()),
        Just("- [ ] ".to_owned()),
        Just("> ".to_owned()),
        Just("```".to_owned()),
        Just("---".to_owned()),
        Just("**".to_owned()),
        Just("*".to_owned()),
        Just("_".to_owned()),
        Just("~~".to_owned()),
        Just("`".to_owned()),
        Just("[".to_owned()),
        Just("](".to_owned()),
        Just(")".to_owned()),
        Just("![".to_owned()),
        Just("|".to_owned()),
        Just("\\".to_owned()),
        Just("&amp;".to_owned()),
        Just("<u>".to_owned()),
        Just("\n".to_owned()),
        Just("\n\n".to_owned()),
        Just("  ".to_owned()),
        Just("\t".to_owned()),
        Just("word".to_owned()),
        Just("é".to_owned()),
        Just("🇫🇷".to_owned()),
        Just("1".to_owned()),
    ];
    prop::collection::vec(piece, 0..80).prop_map(|pieces| pieces.concat())
}

/// JSON values shaped like ProseMirror nodes, with wrong types and unknown names mixed in.
fn json_node() -> impl Strategy<Value = serde_json::Value> {
    use serde_json::{Value, json};
    let name = prop_oneof![
        Just(json!("doc")),
        Just(json!("paragraph")),
        Just(json!("heading")),
        Just(json!("text")),
        Just(json!("bulletList")),
        Just(json!("listItem")),
        Just(json!("taskItem")),
        Just(json!("codeBlock")),
        Just(json!("hardBreak")),
        Just(json!("mention")),
        Just(json!("somethingNew")),
        Just(json!(42)),
        Just(Value::Null),
    ];
    let scalar = prop_oneof![
        Just(Value::Null),
        Just(json!(true)),
        Just(json!(3)),
        Just(json!("s")),
        Just(json!([])),
        Just(json!({})),
        Just(json!({"level": 2, "checked": true, "href": "https://example.com"})),
    ];
    let leaf = (
        name.clone(),
        scalar.clone(),
        prop::option::of("[a-zé ]{0,6}"),
    )
        .prop_map(|(name, attrs, text)| {
            let mut node = json!({"type": name, "attrs": attrs});
            if let Some(text) = text {
                node["text"] = json!(text);
            }
            node
        });
    leaf.prop_recursive(4, 40, 5, move |inner| {
        (
            name.clone(),
            scalar.clone(),
            prop::collection::vec(inner.clone(), 0..5),
            prop::collection::vec(inner, 0..2),
            any::<bool>(),
        )
            .prop_map(|(name, attrs, content, marks, keys_reversed)| {
                if keys_reversed {
                    json!({"marks": marks, "content": content, "attrs": attrs, "type": name})
                } else {
                    json!({"type": name, "attrs": attrs, "content": content, "marks": marks})
                }
            })
    })
}

fn import_search(cases: u32) {
    let mut runner = runner(cases);
    runner
        .run(&markdown_text(), |source| {
            let first = Node::from_markdown(&source).to_markdown();
            let second = Node::from_markdown(&first).to_markdown();
            let third = Node::from_markdown(&second).to_markdown();
            prop_assert_eq!(&second, &third, "export not stable for {:?}", source);
            Ok(())
        })
        .unwrap();
    runner
        .run(&json_node(), |value| {
            let source = value.to_string();
            if let Ok(node) = Node::from_json_str(&source) {
                let written = node.to_json_string();
                let reread = Node::from_json_str(&written)
                    .map_err(|error| TestCaseError::fail(format!("{error}: {written}")))?;
                prop_assert_eq!(&reread, &node, "JSON round trip of {}", source);
                prop_assert_eq!(reread.to_json_string(), written);
                let _ = node.to_markdown();
                let _ = node.plain_text();
                let mut editor = Editor::new(node);
                editor.select_all();
                editor.insert_text("x");
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn importers_never_panic_and_round_trip() {
    import_search(256);
}

#[test]
#[ignore = "long search, run explicitly with --ignored"]
fn importers_never_panic_and_round_trip_long() {
    import_search(10_000);
}
