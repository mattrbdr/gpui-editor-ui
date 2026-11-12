use gpui_editor_core::Node;

fn text(source: &str) -> String {
    Node::from_json_str(source).unwrap().plain_text()
}

#[test]
fn blocks_are_separated_by_a_newline_and_the_result_is_trimmed() {
    let json = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Titre"}]},{"type":"paragraph","content":[{"type":"text","text":"un "},{"type":"text","marks":[{"type":"bold"}],"text":"deux"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"a"}]}]},{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"b"}]}]}]}]}"#;
    assert_eq!(text(json), "Titre\nun deux\na\n\nb");
}

#[test]
fn runs_of_blank_lines_collapse_to_one_blank_line() {
    let json = r#"{"type":"doc","content":[{"type":"paragraph"},{"type":"paragraph"},{"type":"paragraph","content":[{"type":"text","text":"x"}]},{"type":"paragraph"},{"type":"paragraph"},{"type":"paragraph"},{"type":"paragraph","content":[{"type":"text","text":"y"}]}]}"#;
    assert_eq!(text(json), "x\n\ny");
}

#[test]
fn hard_breaks_add_no_text_like_the_reference_implementation() {
    let json = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"},{"type":"hardBreak"},{"type":"text","text":"b"}]}]}"#;
    assert_eq!(text(json), "ab");
}

#[test]
fn a_host_hook_can_render_custom_leaf_nodes() {
    let json = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"voir "},{"type":"mention","attrs":{"label":"Page"}}]},{"type":"databaseView","attrs":{}}]}"#;
    let node = Node::from_json_str(json).unwrap();
    let rendered = node.plain_text_with(
        &|n| match n.kind.name() {
            "mention" => n
                .attrs
                .get("label")
                .and_then(|v| v.as_str())
                .map(|l| format!("[[{l}]]")),
            "databaseView" => Some("[vue]".to_owned()),
            _ => None,
        },
        &["databaseView"],
    );
    assert_eq!(rendered, "voir [[Page]]\n[vue]");
}

#[test]
fn code_blocks_and_task_items_end_with_a_newline() {
    let json = r#"{"type":"doc","content":[{"type":"codeBlock","content":[{"type":"text","text":"ls"}]},{"type":"taskList","content":[{"type":"taskItem","attrs":{"checked":false},"content":[{"type":"paragraph","content":[{"type":"text","text":"t"}]}]}]}]}"#;
    assert_eq!(text(json), "ls\nt");
}
