use gpui_editor_core::{Editor, Kind, MarkKind, Node, Pos, Selection};

fn doc(blocks: &str) -> Node {
    Node::from_json_str(&format!(r#"{{"type":"doc","content":[{blocks}]}}"#)).unwrap()
}

fn p(text: &str) -> String {
    format!(
        r#"{{"type":"paragraph","content":[{{"type":"text","text":{}}}]}}"#,
        serde_json::to_string(text).unwrap()
    )
}

fn editor(blocks: &[&str]) -> Editor {
    let joined = blocks.iter().map(|t| p(t)).collect::<Vec<_>>().join(",");
    Editor::new(doc(&joined))
}

fn caret(block: usize, offset: usize) -> Selection {
    Selection::caret(Pos { block, offset })
}

fn texts(editor: &Editor) -> Vec<String> {
    (0..editor.block_count())
        .map(|i| editor.block_text(i))
        .collect()
}

#[test]
fn an_empty_document_gets_one_empty_paragraph() {
    let editor = Editor::new(Node::from_json_str(r#"{"type":"doc","content":[]}"#).unwrap());
    assert_eq!(editor.block_count(), 1);
    assert_eq!(editor.block_kind(0), Kind::Paragraph);
    assert_eq!(editor.selection(), caret(0, 0));
}

#[test]
fn typing_inserts_text_at_the_caret_and_moves_it() {
    let mut e = editor(&["héllo"]);
    e.set_selection(caret(0, 2));
    e.insert_text("XY");
    assert_eq!(texts(&e), ["héXYllo"]);
    assert_eq!(e.selection(), caret(0, 4));
}

#[test]
fn typing_over_a_selection_replaces_it() {
    let mut e = editor(&["abcdef"]);
    e.set_selection(Selection {
        anchor: Pos {
            block: 0,
            offset: 1,
        },
        head: Pos {
            block: 0,
            offset: 4,
        },
    });
    e.insert_text("Z");
    assert_eq!(texts(&e), ["aZef"]);
    assert_eq!(e.selection(), caret(0, 2));
}

#[test]
fn backspace_deletes_one_grapheme_even_when_it_is_several_scalars() {
    let mut e = editor(&["ae\u{301}🇫🇷!"]);
    e.set_selection(caret(0, 6));
    e.delete_backward();
    assert_eq!(texts(&e), ["ae\u{301}🇫🇷"]);
    e.delete_backward();
    assert_eq!(texts(&e), ["ae\u{301}"]);
    e.delete_backward();
    assert_eq!(texts(&e), ["a"]);
}

#[test]
fn delete_forward_removes_the_next_grapheme() {
    let mut e = editor(&["e\u{301}x"]);
    e.set_selection(caret(0, 0));
    e.delete_forward();
    assert_eq!(texts(&e), ["x"]);
}

#[test]
fn backspace_at_the_start_of_a_block_joins_it_with_the_previous_one() {
    let mut e = editor(&["un", "deux"]);
    e.set_selection(caret(1, 0));
    e.delete_backward();
    assert_eq!(texts(&e), ["undeux"]);
    assert_eq!(e.selection(), caret(0, 2));
}

#[test]
fn delete_forward_at_the_end_of_a_block_joins_the_next_one() {
    let mut e = editor(&["un", "deux"]);
    e.set_selection(caret(0, 2));
    e.delete_forward();
    assert_eq!(texts(&e), ["undeux"]);
    assert_eq!(e.selection(), caret(0, 2));
}

#[test]
fn enter_splits_a_paragraph() {
    let mut e = editor(&["abcd"]);
    e.set_selection(caret(0, 2));
    e.split_block();
    assert_eq!(texts(&e), ["ab", "cd"]);
    assert_eq!(e.selection(), caret(1, 0));
}

#[test]
fn enter_at_the_end_of_a_heading_starts_a_paragraph() {
    let mut e = Editor::new(doc(
        r#"{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Titre"}]}"#,
    ));
    e.set_selection(caret(0, 5));
    e.split_block();
    assert_eq!(
        (e.block_kind(0), e.block_kind(1)),
        (Kind::Heading, Kind::Paragraph)
    );
    e.set_selection(caret(0, 2));
    e.split_block();
    assert_eq!(
        (e.block_kind(0), e.block_kind(1)),
        (Kind::Heading, Kind::Heading)
    );
}

#[test]
fn enter_in_a_code_block_inserts_a_newline() {
    let mut e = Editor::new(doc(
        r#"{"type":"codeBlock","attrs":{"language":null},"content":[{"type":"text","text":"ab"}]}"#,
    ));
    e.set_selection(caret(0, 1));
    e.split_block();
    assert_eq!(texts(&e), ["a\nb"]);
    assert_eq!(e.block_count(), 1);
}

#[test]
fn typing_a_newline_splits_the_block_like_a_paste() {
    let mut e = editor(&["ab"]);
    e.set_selection(caret(0, 1));
    e.insert_text("x\ny\nz");
    assert_eq!(texts(&e), ["ax", "y", "zb"]);
    assert_eq!(e.selection(), caret(2, 1));
}

#[test]
fn a_hard_break_is_inserted_with_shift_enter() {
    let mut e = editor(&["ab"]);
    e.set_selection(caret(0, 1));
    e.insert_hard_break();
    assert_eq!(e.doc().to_markdown(), "a\\\nb");
}

#[test]
fn deleting_a_selection_across_blocks_merges_the_ends() {
    let mut e = editor(&["premier", "milieu", "dernier"]);
    e.set_selection(Selection {
        anchor: Pos {
            block: 0,
            offset: 3,
        },
        head: Pos {
            block: 2,
            offset: 3,
        },
    });
    e.delete_backward();
    assert_eq!(texts(&e), ["prenier"]);
    assert_eq!(e.selection(), caret(0, 3));
}

#[test]
fn toggling_bold_over_a_selection_marks_and_unmarks_it() {
    let mut e = editor(&["abcdef"]);
    e.set_selection(Selection {
        anchor: Pos {
            block: 0,
            offset: 1,
        },
        head: Pos {
            block: 0,
            offset: 4,
        },
    });
    e.toggle_mark(MarkKind::Bold);
    assert_eq!(e.doc().to_markdown(), "a**bcd**ef");
    assert!(e.is_mark_active(&MarkKind::Bold));
    e.toggle_mark(MarkKind::Bold);
    assert_eq!(e.doc().to_markdown(), "abcdef");
}

#[test]
fn a_mark_toggled_at_a_caret_applies_to_the_next_typed_text() {
    let mut e = editor(&["ab"]);
    e.set_selection(caret(0, 1));
    e.toggle_mark(MarkKind::Italic);
    e.insert_text("X");
    assert_eq!(e.doc().to_markdown(), "a*X*b");
}

#[test]
fn typed_text_inherits_the_marks_before_the_caret() {
    let mut e = Editor::new(Node::from_markdown("a **gras**"));
    e.set_selection(caret(0, 6));
    e.insert_text("!");
    assert_eq!(e.doc().to_markdown(), "a **gras!**");
}

#[test]
fn a_link_can_be_set_and_removed() {
    let mut e = editor(&["voir ici"]);
    e.set_selection(Selection {
        anchor: Pos {
            block: 0,
            offset: 5,
        },
        head: Pos {
            block: 0,
            offset: 8,
        },
    });
    e.set_link(Some("https://e.com"));
    assert_eq!(e.doc().to_markdown(), "voir [ici](https://e.com)");
    e.set_link(None);
    assert_eq!(e.doc().to_markdown(), "voir ici");
}

#[test]
fn blocks_change_type_and_toggle_back_to_paragraphs() {
    let mut e = editor(&["Titre"]);
    e.set_selection(caret(0, 2));
    e.toggle_heading(2);
    assert_eq!(e.doc().to_markdown(), "## Titre");
    e.toggle_heading(2);
    assert_eq!(e.doc().to_markdown(), "Titre");
    e.toggle_code_block();
    assert_eq!(e.doc().to_markdown(), "```\nTitre\n```");
    e.toggle_code_block();
    assert_eq!(e.doc().to_markdown(), "Titre");
}

#[test]
fn lists_can_be_created_split_exited_and_unwrapped() {
    let mut e = editor(&["un"]);
    e.set_selection(caret(0, 2));
    e.toggle_list(Kind::BulletList);
    assert_eq!(e.doc().to_markdown(), "- un");
    e.split_block();
    e.insert_text("deux");
    assert_eq!(e.doc().to_markdown(), "- un\n- deux");
    e.split_block();
    e.split_block();
    assert_eq!(e.doc().to_markdown(), "- un\n- deux");
    assert_eq!(e.block_count(), 3);
    assert_eq!(e.block_kind(2), Kind::Paragraph);
    e.set_selection(caret(0, 0));
    e.toggle_list(Kind::BulletList);
    assert_eq!(e.doc().to_markdown(), "un\n\ndeux");
}

#[test]
fn task_items_can_be_checked() {
    let mut e = editor(&["fait"]);
    e.toggle_list(Kind::TaskList);
    assert_eq!(e.doc().to_markdown(), "- [ ] fait");
    e.toggle_task_checked();
    assert_eq!(e.doc().to_markdown(), "- [x] fait");
}

#[test]
fn list_items_can_be_indented_and_outdented() {
    let mut e = Editor::new(Node::from_markdown("- a\n- b"));
    e.set_selection(caret(1, 0));
    e.indent();
    assert_eq!(e.doc().to_markdown(), "- a\n  - b");
    e.outdent();
    assert_eq!(e.doc().to_markdown(), "- a\n- b");
}

#[test]
fn undo_and_redo_restore_the_document_and_the_selection() {
    let mut e = editor(&["abc"]);
    e.set_selection(caret(0, 3));
    e.split_block();
    e.insert_text("def");
    assert_eq!(texts(&e), ["abc", "def"]);
    e.undo();
    assert_eq!(texts(&e), ["abc", ""]);
    e.undo();
    assert_eq!(texts(&e), ["abc"]);
    assert_eq!(e.selection(), caret(0, 3));
    assert!(!e.can_undo());
    e.redo();
    e.redo();
    assert_eq!(texts(&e), ["abc", "def"]);
    assert!(!e.can_redo());
}

#[test]
fn consecutive_typing_is_one_undo_step() {
    let mut e = editor(&[""]);
    for c in "bonjour".chars() {
        e.insert_text(&c.to_string());
    }
    assert_eq!(texts(&e), ["bonjour"]);
    e.undo();
    assert_eq!(texts(&e), [""]);
}

#[test]
fn a_new_edit_clears_the_redo_stack() {
    let mut e = editor(&["a"]);
    e.set_selection(caret(0, 1));
    e.insert_text("b");
    e.undo();
    assert!(e.can_redo());
    e.insert_text("c");
    assert!(!e.can_redo());
}

#[test]
fn horizontal_movement_follows_graphemes_and_crosses_blocks() {
    let mut e = editor(&["a\u{1F1EB}\u{1F1F7}", "b"]);
    e.set_selection(caret(0, 0));
    e.move_right(false);
    e.move_right(false);
    assert_eq!(e.selection(), caret(0, 3));
    e.move_right(false);
    assert_eq!(e.selection(), caret(1, 0));
    e.move_left(false);
    assert_eq!(e.selection(), caret(0, 3));
    e.move_left(false);
    assert_eq!(e.selection(), caret(0, 1));
}

#[test]
fn extending_a_selection_keeps_the_anchor_and_collapsing_uses_the_edge() {
    let mut e = editor(&["abcdef"]);
    e.set_selection(caret(0, 2));
    e.move_right(true);
    e.move_right(true);
    assert_eq!(
        e.selection(),
        Selection {
            anchor: Pos {
                block: 0,
                offset: 2
            },
            head: Pos {
                block: 0,
                offset: 4
            }
        }
    );
    e.move_left(false);
    assert_eq!(e.selection(), caret(0, 2));
    e.move_right(true);
    e.move_right(false);
    assert_eq!(e.selection(), caret(0, 3));
}

#[test]
fn words_blocks_and_the_whole_document_can_be_selected() {
    let mut e = editor(&["un deux  trois", "fin"]);
    e.set_selection(caret(0, 0));
    e.move_word_right(false);
    assert_eq!(e.selection(), caret(0, 2));
    e.move_word_right(false);
    assert_eq!(e.selection(), caret(0, 7));
    e.move_word_left(false);
    assert_eq!(e.selection(), caret(0, 3));
    e.move_to_block_end(false);
    assert_eq!(e.selection(), caret(0, 14));
    e.move_to_block_start(false);
    assert_eq!(e.selection(), caret(0, 0));
    e.select_all();
    assert_eq!(e.selected_text(), "un deux  trois\nfin");
}

#[test]
fn the_selected_text_spans_blocks() {
    let mut e = editor(&["premier", "second"]);
    e.set_selection(Selection {
        anchor: Pos {
            block: 0,
            offset: 3,
        },
        head: Pos {
            block: 1,
            offset: 2,
        },
    });
    assert_eq!(e.selected_text(), "mier\nse");
}

#[test]
fn unknown_blocks_are_preserved_and_skipped() {
    let blocks = format!(
        r#"{},{{"type":"databaseView","attrs":{{"databaseId":"x"}}}},{}"#,
        p("a"),
        p("b")
    );
    let mut e = Editor::new(doc(&blocks));
    assert_eq!(e.block_count(), 2);
    e.set_selection(caret(1, 0));
    e.delete_backward();
    assert_eq!(texts(&e), ["ab"]);
    assert!(
        !e.doc().to_json_string().contains("databaseView"),
        "supprimé car entre les deux extrémités fusionnées"
    );
    let mut e2 = Editor::new(doc(&blocks));
    e2.set_selection(caret(0, 1));
    e2.insert_text("x");
    assert!(e2.doc().to_json_string().contains("databaseView"));
}

#[test]
fn replace_range_edits_without_history_until_a_batch_is_opened() {
    let mut e = editor(&["abcdef"]);
    e.begin_batch();
    e.replace_range(
        Pos {
            block: 0,
            offset: 1,
        },
        Pos {
            block: 0,
            offset: 3,
        },
        "X",
        false,
    );
    e.replace_range(
        Pos {
            block: 0,
            offset: 1,
        },
        Pos {
            block: 0,
            offset: 2,
        },
        "XYZ",
        false,
    );
    assert_eq!(texts(&e), ["aXYZdef"]);
    assert_eq!(e.selection(), caret(0, 4));
    e.undo();
    assert_eq!(texts(&e), ["abcdef"]);
    assert!(!e.can_undo());
}

#[test]
fn a_recorded_range_replacement_is_one_undo_step() {
    let mut e = editor(&["abcdef"]);
    e.replace_range(
        Pos {
            block: 0,
            offset: 0,
        },
        Pos {
            block: 0,
            offset: 3,
        },
        "",
        true,
    );
    assert_eq!(texts(&e), ["def"]);
    e.undo();
    assert_eq!(texts(&e), ["abcdef"]);
}

#[test]
fn double_click_helpers_select_a_word_or_a_block() {
    let mut e = editor(&["un deux  trois", "fin"]);
    e.select_word_at(Pos {
        block: 0,
        offset: 5,
    });
    assert_eq!(e.selected_text(), "deux");
    e.select_word_at(Pos {
        block: 0,
        offset: 7,
    });
    assert_eq!(e.selected_text(), "deux");
    e.select_block(1);
    assert_eq!(e.selected_text(), "fin");
    e.select_word_at(Pos {
        block: 0,
        offset: 8,
    });
    assert_eq!(e.selection(), caret(0, 8));
}
