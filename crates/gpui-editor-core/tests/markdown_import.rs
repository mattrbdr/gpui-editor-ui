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

/// Cas où l'import doit produire exactement le JSON de `@tiptap/markdown`.
const SAME_AS_TIPTAP: &[&str] = &[
    "empty",
    "paragraph",
    "two_paragraphs",
    "headings",
    "marks",
    "nested_marks",
    "link",
    "link_with_bold",
    "hard_break",
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
    "escapes",
    "mixed",
    "soft_break",
    "underscore_emphasis",
    "link_title",
    "image_no_alt",
    "empty_code",
    "indented_code",
    "tilde_fence",
    "setext",
    "loose_list",
    "ordered_ones",
    "nested_quote",
    "quote_list",
    "list_with_code",
    "html_inline",
    "html_block",
    "heading_marks",
    "adjacent_marks",
    "code_span_backticks",
    "intraword",
    "list_two_paragraphs",
    "ordered_alpha_list",
    "mixed_list_nesting",
    "bold_link",
    "strike_bold",
];

#[test]
fn markdown_import_matches_tiptap_on_the_reference_cases() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        if !SAME_AS_TIPTAP.contains(&name) {
            continue;
        }
        checked += 1;
        let markdown = case["markdown"].as_str().unwrap();
        let actual = Node::from_markdown(markdown).to_json_string();
        let expected = case["doc"].to_string();
        if actual != expected {
            failures.push(format!(
                "{name}\n   source  : {markdown:?}\n   attendu : {expected}\n   obtenu  : {actual}"
            ));
        }
    }
    assert_eq!(checked, SAME_AS_TIPTAP.len());
    assert!(
        failures.is_empty(),
        "{} écart(s) :\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_clean_import_keeps_what_tiptap_drops_or_mangles() {
    // Tableau : TipTap l'ignore, on le garde en texte plutôt que de perdre le contenu.
    let table = Node::from_markdown("|a|b|\n|-|-|\n|1|2|");
    assert_eq!(table.plain_text(), "|a|b|\n|-|-|\n|1|2|");
    // Entités : toutes décodées (TipTap ne décode que quelques-unes).
    assert_eq!(
        Node::from_markdown("a &amp; b &copy;").plain_text(),
        "a & b ©"
    );
}

#[test]
fn underline_syntax_is_read_only_in_the_compat_flavor() {
    let compat = MarkdownOptions {
        flavor: Flavor::TipTapCompat,
    };
    let node = Node::from_markdown_with("a ++u++ b", &compat);
    assert_eq!(
        node.to_json_string(),
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a "},{"type":"text","text":"u","marks":[{"type":"underline"}]},{"type":"text","text":" b"}]}]}"#
    );
    assert_eq!(Node::from_markdown("a ++u++ b").plain_text(), "a ++u++ b");
}

#[test]
fn compat_flavor_splits_mixed_lists_and_drops_tables_like_tiptap() {
    let compat = MarkdownOptions {
        flavor: Flavor::TipTapCompat,
    };
    let item = |kind: &str, text: &str, extra: &str| {
        format!(
            r#"{{"type":"{kind}",{extra}"content":[{{"type":"paragraph","content":[{{"type":"text","text":"{text}"}}]}}]}}"#
        )
    };
    let bullets = format!(
        r#"{{"type":"bulletList","content":[{}]}}"#,
        item("listItem", "a", "")
    );
    let tasks = format!(
        r#"{{"type":"taskList","content":[{}]}}"#,
        item("taskItem", "b", r#""attrs":{"checked":false},"#)
    );
    assert_eq!(
        Node::from_markdown_with("- a\n- [ ] b", &compat).to_json_string(),
        format!(r#"{{"type":"doc","content":[{bullets},{tasks}]}}"#)
    );
    assert_eq!(
        Node::from_markdown_with("| a | b |\n|---|---|\n| 1 | 2 |\n\nfin", &compat).plain_text(),
        "fin"
    );
    assert!(
        Node::from_markdown("| a | b |\n|---|---|\n| 1 | 2 |")
            .plain_text()
            .contains("| a | b |")
    );
}

#[test]
fn export_then_import_is_stable_in_both_flavors() {
    let source = "# Titre\n\nUn **gras**, de l'*italique*, du `code` et un [lien](https://e.com).\n\n- a\n- b\n  - c\n\n1. un\n2. deux\n\n- [ ] à faire\n- [x] fait\n\n> citation\n\n```js\nconst a = 1;\n```\n\n---";
    for flavor in [Flavor::CommonMark, Flavor::TipTapCompat] {
        let options = MarkdownOptions { flavor };
        let first = Node::from_markdown_with(source, &options);
        let markdown = first.to_markdown_with(&options);
        assert_eq!(markdown, source, "{flavor:?}");
        let second = Node::from_markdown_with(&markdown, &options);
        assert_eq!(second, first, "{flavor:?}");
    }
}
