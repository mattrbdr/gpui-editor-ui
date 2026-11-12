use gpui_editor_core::{Error, Kind, MarkKind, Node};

/// Document au format canonique de ProseMirror (`type`, `attrs`, `content`, `marks`, `text`).
const DOC: &str = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":2,"textAlign":"center"},"content":[{"type":"text","text":"Titre"}]},{"type":"paragraph","attrs":{"textAlign":null},"content":[{"type":"text","marks":[{"type":"bold"},{"type":"link","attrs":{"href":"https://example.com","target":"_blank","rel":null,"class":null}}],"text":"gras lié"},{"type":"hardBreak"},{"type":"text","marks":[{"type":"code"}],"text":"code"},{"type":"text","marks":[{"type":"underline"}],"text":" souligné"}]},{"type":"codeBlock","attrs":{"language":null},"content":[{"type":"text","text":"let x = 1;\nlet y = 2;"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"un"}]}]}]},{"type":"taskList","content":[{"type":"taskItem","attrs":{"checked":true},"content":[{"type":"paragraph","content":[{"type":"text","text":"fait"}]}]}]},{"type":"horizontalRule"},{"type":"databaseView","attrs":{"databaseId":"x","viewId":"y"}},{"type":"paragraph"}]}"#;

#[test]
fn a_canonical_document_round_trips_byte_for_byte() {
    let node = Node::from_json_str(DOC).unwrap();
    assert_eq!(node.to_json_string(), DOC);
}

#[test]
fn an_empty_document_keeps_its_explicit_empty_content() {
    let source = r#"{"type":"doc","content":[]}"#;
    assert_eq!(
        Node::from_json_str(source).unwrap().to_json_string(),
        source
    );
    let bare = r#"{"type":"doc"}"#;
    assert_eq!(Node::from_json_str(bare).unwrap().to_json_string(), bare);
}

#[test]
fn node_and_mark_kinds_are_recognised_and_unknown_ones_are_preserved() {
    let node = Node::from_json_str(DOC).unwrap();
    assert_eq!(node.kind, Kind::Doc);
    let kinds: Vec<&Kind> = node.content.iter().map(|child| &child.kind).collect();
    assert_eq!(
        kinds,
        [
            &Kind::Heading,
            &Kind::Paragraph,
            &Kind::CodeBlock,
            &Kind::BulletList,
            &Kind::TaskList,
            &Kind::HorizontalRule,
            &Kind::Other("databaseView".into()),
            &Kind::Paragraph,
        ]
    );
    let marked = &node.content[1].content[0];
    assert_eq!(marked.kind, Kind::Text);
    assert_eq!(marked.text.as_deref(), Some("gras lié"));
    assert_eq!(marked.marks[0].kind, MarkKind::Bold);
    assert_eq!(marked.marks[1].kind, MarkKind::Link);
    assert_eq!(marked.marks[1].attrs["href"], "https://example.com");
    assert_eq!(node.content[0].attrs["level"], 2);
}

#[test]
fn unknown_marks_and_extra_attributes_survive_the_round_trip() {
    let source = r##"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","marks":[{"type":"highlight","attrs":{"color":"#ff0"}}],"text":"x"}]}]}"##;
    let node = Node::from_json_str(source).unwrap();
    assert_eq!(
        node.content[0].content[0].marks[0].kind,
        MarkKind::Other("highlight".into())
    );
    assert_eq!(node.to_json_string(), source);
}

#[test]
fn unicode_and_escapes_are_preserved() {
    let source = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"é à \"q\" \\ 🙂 日本"}]}]}"#;
    assert_eq!(
        Node::from_json_str(source).unwrap().to_json_string(),
        source
    );
}

#[test]
fn invalid_inputs_are_reported() {
    assert!(matches!(Node::from_json_str("nope"), Err(Error::Json(_))));
    assert!(matches!(Node::from_json_str("[]"), Err(Error::NotAnObject)));
    assert!(matches!(
        Node::from_json_str(r#"{"content":[]}"#),
        Err(Error::MissingType)
    ));
    assert!(matches!(
        Node::from_json_str(r#"{"type":3}"#),
        Err(Error::MissingType)
    ));
    assert!(matches!(
        Node::from_json_str(r#"{"type":"text"}"#),
        Err(Error::MissingText)
    ));
    assert!(matches!(
        Node::from_json_str(r#"{"type":"doc","content":[{"type":"paragraph","content":[7]}]}"#),
        Err(Error::NotAnObject)
    ));
}

#[test]
fn walk_visits_every_node_in_document_order() {
    let node = Node::from_json_str(DOC).unwrap();
    let mut names = Vec::new();
    node.walk(&mut |visited| names.push(visited.kind.name().to_owned()));
    assert_eq!(names[0], "doc");
    assert_eq!(names[1], "heading");
    assert_eq!(names[2], "text");
    assert_eq!(names.len(), 21);
    assert_eq!(names.iter().filter(|name| *name == "text").count(), 7);
}

#[test]
fn equality_ignores_how_empty_collections_were_written() {
    let a = Node::from_json_str(r#"{"type":"doc","content":[]}"#).unwrap();
    let b = Node::from_json_str(r#"{"type":"doc"}"#).unwrap();
    assert_eq!(a, b);
}

#[test]
fn the_original_key_order_of_each_node_is_preserved() {
    // `text` avant `marks` : ordre produit par certains convertisseurs Markdown.
    let text_first = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a","marks":[{"type":"bold"}]}]}]}"#;
    assert_eq!(
        Node::from_json_str(text_first).unwrap().to_json_string(),
        text_first
    );
    let attrs_last = r#"{"content":[{"type":"paragraph"}],"type":"doc"}"#;
    assert_eq!(
        Node::from_json_str(attrs_last).unwrap().to_json_string(),
        attrs_last
    );
    let mark_order = r##"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","marks":[{"attrs":{"href":"#"},"type":"link"}],"text":"a"}]}]}"##;
    assert_eq!(
        Node::from_json_str(mark_order).unwrap().to_json_string(),
        mark_order
    );
}

#[test]
fn keys_added_after_parsing_follow_the_canonical_order() {
    let mut node = Node::from_json_str(r#"{"type":"text","text":"a"}"#).unwrap();
    node.marks.push(gpui_editor_core::Mark::new(MarkKind::Bold));
    assert_eq!(
        node.to_json_string(),
        r#"{"type":"text","marks":[{"type":"bold"}],"text":"a"}"#
    );
}
