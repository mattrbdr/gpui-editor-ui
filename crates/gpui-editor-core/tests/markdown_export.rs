use gpui_editor_core::{Flavor, MarkdownOptions, Node};
use serde_json::Value;

fn cases() -> Vec<Value> {
    let source = include_str!("fixtures/tiptap_markdown.json");
    serde_json::from_str::<Value>(source)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn compat(node: &Node) -> String {
    node.to_markdown_with(&MarkdownOptions {
        flavor: Flavor::TipTapCompat,
    })
}

fn clean(json: &str) -> String {
    Node::from_json_str(json).unwrap().to_markdown()
}

#[test]
fn the_compat_flavor_matches_tiptap_on_every_fixture() {
    let mut failures = Vec::new();
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let node = Node::from_json(&case["doc"]).unwrap();
        let expected = case["markdown_out"].as_str().unwrap();
        let actual = compat(&node);
        if actual != expected {
            failures.push(format!(
                "{name}\n   attendu : {expected:?}\n   obtenu  : {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} écart(s) :\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_default_flavor_is_commonmark() {
    assert_eq!(MarkdownOptions::default().flavor, Flavor::CommonMark);
}

#[test]
fn clean_output_agrees_with_tiptap_on_ordinary_documents() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let ordinary = [
            "paragraph",
            "two_paragraphs",
            "headings",
            "marks",
            "nested_marks",
            "link_with_bold",
            "bullet_list",
            "ordered_list",
            "ordered_list_start",
            "task_list",
            "code_block",
            "code_block_plain",
            "blockquote",
            "horizontal_rule",
            "image",
            "unicode",
            "doc_strike",
            "doc_bold_italic",
            "doc_italic_bold",
            "doc_code_bold",
            "doc_bold_adjacent",
            "doc_bold_then_italic",
            "doc_bold_spaces",
            "doc_heading_levels",
            "doc_ordered_in_bullet",
            "doc_item_two_paragraphs",
            "doc_item_code_block",
            "doc_blockquote_list",
            "doc_quote_two_paragraphs",
            "doc_code_backtick",
            "doc_code_two_backticks",
            "doc_two_code_blocks",
            "doc_link_title",
            "doc_heading_marks",
        ];
        if ordinary.contains(&name) {
            let node = Node::from_json(&case["doc"]).unwrap();
            assert_eq!(
                node.to_markdown(),
                case["markdown_out"].as_str().unwrap(),
                "{name}"
            );
        }
    }
}

#[test]
fn clean_output_escapes_only_what_commonmark_needs() {
    let p = |text: &str| {
        format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":{}}}]}}]}}"#,
            serde_json::to_string(text).unwrap()
        )
    };
    assert_eq!(
        clean(&p("a*b_c`d[e]f~g\\h<i")),
        "a\\*b\\_c\\`d\\[e\\]f\\~g\\\\h\\<i"
    );
    assert_eq!(
        clean(&p("a & b &amp; c &#35; d")),
        "a & b &amp;amp; c &amp;#35; d"
    );
    assert_eq!(clean(&p("a > b")), "a > b");
    assert_eq!(clean(&p("# titre")), "\\# titre");
    assert_eq!(clean(&p("- item")), "\\- item");
    assert_eq!(clean(&p("+ item")), "\\+ item");
    assert_eq!(clean(&p("1. item")), "1\\. item");
    assert_eq!(clean(&p("2) item")), "2\\) item");
    assert_eq!(clean(&p("> citation")), "\\> citation");
    assert_eq!(clean(&p("---")), "\\---");
    assert_eq!(clean(&p("1.5 et #hash et a-b")), "1.5 et #hash et a-b");
    assert_eq!(clean(&p("ligne\n# titre")), "ligne\n\\# titre");
}

#[test]
fn clean_output_uses_commonmark_constructs_where_tiptap_does_not() {
    let hard_break = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"},{"type":"hardBreak"},{"type":"text","text":"b"}]}]}"#;
    assert_eq!(clean(hard_break), "a\\\nb");
    let underline = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","marks":[{"type":"underline"}],"text":"u"}]}]}"#;
    assert_eq!(clean(underline), "<u>u</u>");
    let fenced = r#"{"type":"doc","content":[{"type":"codeBlock","content":[{"type":"text","text":"```\nx\n```"}]}]}"#;
    assert_eq!(clean(fenced), "````\n```\nx\n```\n````");
    let empty_paragraphs = r#"{"type":"doc","content":[{"type":"paragraph"},{"type":"paragraph","content":[{"type":"text","text":"x"}]},{"type":"paragraph"},{"type":"paragraph"},{"type":"paragraph","content":[{"type":"text","text":"y"}]}]}"#;
    assert_eq!(clean(empty_paragraphs), "x\n\ny");
    let unknown = r#"{"type":"doc","content":[{"type":"databaseView","attrs":{}},{"type":"paragraph","content":[{"type":"text","text":"y"}]}]}"#;
    assert_eq!(clean(unknown), "y");
    let start_zero = r#"{"type":"doc","content":[{"type":"orderedList","attrs":{"start":0},"content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"a"}]}]}]}]}"#;
    assert_eq!(clean(start_zero), "0. a");
}

#[test]
fn clean_links_are_valid_commonmark() {
    let link = |href: &str, title: &str| {
        format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","marks":[{{"type":"link","attrs":{{"href":{},"title":{}}}}}],"text":"x"}}]}}]}}"#,
            serde_json::to_string(href).unwrap(),
            serde_json::to_string(title).unwrap()
        )
    };
    assert_eq!(clean(&link("https://e.com/a", "")), "[x](https://e.com/a)");
    assert_eq!(
        clean(&link("https://e.com/a b", "")),
        "[x](<https://e.com/a b>)"
    );
    assert_eq!(
        clean(&link("https://e.com/a(b", "")),
        "[x](<https://e.com/a(b>)"
    );
    assert_eq!(
        clean(&link("https://e.com/a(b)c", "")),
        "[x](https://e.com/a(b)c)"
    );
    assert_eq!(
        clean(&link("https://e.com", "dit \"oui\"")),
        "[x](https://e.com \"dit \\\"oui\\\"\")"
    );
}

#[test]
fn crossing_marks_are_reopened_with_inline_html() {
    let source = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","marks":[{"type":"bold"}],"text":"a"},{"type":"text","marks":[{"type":"bold"},{"type":"italic"}],"text":"b"},{"type":"text","marks":[{"type":"italic"}],"text":"c"}]}]}"#;
    assert_eq!(clean(source), "**a*b***<em>c</em>");
}

#[test]
fn whitespace_stays_outside_emphasis_delimiters() {
    let source = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"x"},{"type":"text","marks":[{"type":"bold"}],"text":" a "},{"type":"text","text":"y"}]}]}"#;
    assert_eq!(clean(source), "x **a** y");
}

#[test]
fn commonmark_export_keeps_mentions_as_double_brackets() {
    let doc = Node::from_json_str(
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Voir "},{"type":"mention","attrs":{"id":"document:d1","label":"Procédure"}},{"type":"text","text":"."}]}]}"#,
    )
    .unwrap();
    assert_eq!(doc.to_markdown(), "Voir [[Procédure]].");
}
