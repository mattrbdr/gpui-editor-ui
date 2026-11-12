use gpui_editor_core::{Editor, Node, Pos, Selection};

const MENTION: &str = r#"{"type":"mention","attrs":{"id":"document:42","label":"Cible","mentionSuggestionChar":"[["}}"#;

fn editor(inline: &str) -> Editor {
    let source = format!(r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{inline}]}}]}}"#);
    Editor::new(Node::from_json_str(&source).unwrap())
}

fn text(value: &str) -> String {
    format!(r#"{{"type":"text","text":"{value}"}}"#)
}

fn caret(offset: usize) -> Selection {
    Selection::caret(Pos { block: 0, offset })
}

fn json(editor: &Editor) -> String {
    editor.doc().to_json_string()
}

fn paragraph(inline: &str) -> String {
    format!(r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{inline}]}}]}}"#)
}

#[test]
fn a_mention_shows_as_wikilink_text_and_survives_an_edit_next_to_it() {
    let mut e = editor(&format!("{},{}", text("Voir "), MENTION));
    assert_eq!(e.block_text(0), "Voir [[Cible]]");
    e.set_selection(caret(0));
    e.insert_text("Hé ");
    assert_eq!(e.block_text(0), "Hé Voir [[Cible]]");
    assert_eq!(json(&e), paragraph(&format!(r#"{{"type":"text","text":"Hé Voir "}},{MENTION}"#)));
}

#[test]
fn typing_right_after_a_mention_does_not_extend_it() {
    let mut e = editor(MENTION);
    e.set_selection(caret(9));
    e.insert_text("!");
    assert_eq!(json(&e), paragraph(&format!(r#"{MENTION},{{"type":"text","text":"!"}}"#)));
}

#[test]
fn editing_inside_a_mention_turns_it_into_plain_text() {
    let mut e = editor(MENTION);
    e.set_selection(caret(4));
    e.insert_text("z");
    assert_eq!(json(&e), paragraph(&text("[[Cizble]]")));
}

#[test]
fn deleting_a_whole_mention_removes_it() {
    let mut e = editor(&format!("{},{}", MENTION, text(" fin")));
    e.set_selection(Selection { anchor: Pos { block: 0, offset: 0 }, head: Pos { block: 0, offset: 9 } });
    e.delete_backward();
    assert_eq!(json(&e), paragraph(&text(" fin")));
}

#[test]
fn inserting_a_mention_replaces_the_typed_query_and_can_be_undone() {
    let mut e = editor(&text("a [[cib"));
    e.set_selection(caret(7));
    e.insert_mention(Pos { block: 0, offset: 2 }, Pos { block: 0, offset: 7 }, "document:42", "Cible");
    assert_eq!(json(&e), paragraph(&format!(r#"{{"type":"text","text":"a "}},{MENTION}"#)));
    assert_eq!(e.selection(), caret(11));
    e.undo();
    assert_eq!(json(&e), paragraph(&text("a [[cib")));
}

#[test]
fn unknown_inline_nodes_are_kept_when_the_paragraph_is_edited() {
    let inline = r#"{"type":"emoji","attrs":{"name":"smile"}}"#;
    let mut e = editor(&format!("{},{}", text("a"), inline));
    e.set_selection(caret(0));
    e.insert_text("b");
    assert_eq!(json(&e), paragraph(&format!(r#"{{"type":"text","text":"ba"}},{inline}"#)));
}

#[test]
fn a_position_next_to_a_mention_reports_its_target() {
    let e = editor(&format!("{},{}", text("Voir "), MENTION));
    assert_eq!(e.mention_near(Pos { block: 0, offset: 8 }).as_deref(), Some("document:42"));
    assert_eq!(e.mention_near(Pos { block: 0, offset: 5 }).as_deref(), Some("document:42"));
    assert_eq!(e.mention_near(Pos { block: 0, offset: 2 }), None);
}
