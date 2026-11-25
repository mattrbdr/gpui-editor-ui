use gpui_editor_core::{Editor, Match, Node, Pos};

fn editor(markdown: &str) -> Editor {
    Editor::new(Node::from_markdown(markdown))
}

fn spans(matches: &[Match]) -> Vec<(usize, usize, usize)> {
    matches
        .iter()
        .map(|m| (m.start.block, m.start.offset, m.end.offset))
        .collect()
}

#[test]
fn search_ignores_case_and_accents_and_reports_character_offsets() {
    let e = editor("Éléphant\n\n- un ELEPHANT, deux éléphants");
    assert_eq!(
        spans(&e.find("elephant")),
        [(0, 0, 8), (1, 3, 11), (1, 18, 26)]
    );
    assert_eq!(spans(&e.find("ÉLÉ")).len(), 3);
}

#[test]
fn a_decomposed_accent_belongs_to_the_match() {
    let e = editor("cafe\u{301} noir");
    assert_eq!(spans(&e.find("café")), [(0, 0, 5)]);
}

#[test]
fn matches_do_not_overlap_and_an_empty_query_finds_nothing() {
    let e = editor("aaaa");
    assert_eq!(spans(&e.find("aa")), [(0, 0, 2), (0, 2, 4)]);
    assert!(e.find("").is_empty());
    assert!(e.find("b").is_empty());
}

#[test]
fn replacing_every_match_is_one_undo_step_and_keeps_the_formatting() {
    let mut e = editor("**chat** et chat\n\nchat");
    let before = e.doc().clone();
    let matches = e.find("chat");
    assert_eq!(e.replace_matches(&matches, "lion"), 3);
    assert_eq!(e.doc().to_markdown(), "**lion** et lion\n\nlion");
    assert_eq!(
        e.selection().head,
        Pos {
            block: 0,
            offset: 4
        }
    );
    e.undo();
    assert_eq!(*e.doc(), before);
}

#[test]
fn replacing_with_nothing_deletes_the_matches() {
    let mut e = editor("a-b-c");
    let matches = e.find("-");
    assert_eq!(e.replace_matches(&matches, ""), 2);
    assert_eq!(e.block_text(0), "abc");
}

#[test]
fn a_line_break_in_the_replacement_does_not_split_the_block() {
    let mut e = editor("a-b");
    let matches = e.find("-");
    e.replace_matches(&matches, "x\ny");
    assert_eq!(e.block_count(), 1);
    assert_eq!(e.block_text(0), "ax yb");
}

#[test]
fn mentions_are_found_but_never_replaced() {
    let doc = Node::from_json_str(
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"document:1","label":"DNS"}},{"type":"text","text":" dns"}]}]}"#,
    )
    .unwrap();
    let mut e = Editor::new(doc);
    let matches = e.find("dns");
    assert_eq!(
        matches.iter().map(|m| m.replaceable).collect::<Vec<_>>(),
        [false, true]
    );
    assert_eq!(e.replace_matches(&matches, "zone"), 1);
    assert_eq!(e.block_text(0), "[[DNS]] zone");
}

#[test]
fn nothing_to_replace_leaves_the_history_alone() {
    let mut e = editor("abc");
    assert_eq!(e.replace_matches(&[], "x"), 0);
    assert!(!e.can_undo());
}
