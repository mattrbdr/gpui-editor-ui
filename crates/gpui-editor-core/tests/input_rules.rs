use gpui_editor_core::{Editor, Kind, MarkKind, Node, Pos, Selection};

fn empty() -> Editor {
    Editor::new(Node::from_json_str(r#"{"type":"doc","content":[{"type":"paragraph"}]}"#).unwrap())
}

fn typed(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        editor.insert_text(&c.to_string());
    }
}

fn md(editor: &Editor) -> String {
    editor.doc().to_markdown()
}

#[test]
fn hash_marks_followed_by_a_space_make_a_heading() {
    for level in 1..=6usize {
        let mut e = empty();
        typed(&mut e, &format!("{} ", "#".repeat(level)));
        assert_eq!(e.block_kind(0), Kind::Heading, "niveau {level}");
        assert_eq!(
            e.block_node(0).attrs["level"],
            u64::try_from(level).unwrap()
        );
        assert_eq!(e.block_text(0), "");
        typed(&mut e, "Titre");
        assert_eq!(md(&e), format!("{} Titre", "#".repeat(level)));
    }
    let mut e = empty();
    typed(&mut e, "####### ");
    assert_eq!(
        e.block_kind(0),
        Kind::Paragraph,
        "sept dièses : pas un titre"
    );
}

#[test]
fn list_markers_make_bullet_ordered_and_task_lists() {
    for marker in ["- ", "* ", "+ "] {
        let mut e = empty();
        typed(&mut e, &format!("{marker}un"));
        assert_eq!(md(&e), "- un", "{marker:?}");
    }
    let mut e = empty();
    typed(&mut e, "1. un");
    assert_eq!(md(&e), "1. un");
    let mut e = empty();
    typed(&mut e, "3. trois");
    assert_eq!(md(&e), "3. trois");
    let mut e = empty();
    typed(&mut e, "[ ] à faire");
    assert_eq!(md(&e), "- [ ] à faire");
    let mut e = empty();
    typed(&mut e, "[x] fait");
    assert_eq!(md(&e), "- [x] fait");
}

#[test]
fn a_task_marker_inside_a_bullet_item_turns_the_list_into_tasks() {
    let mut e = empty();
    typed(&mut e, "- ");
    typed(&mut e, "[ ] ");
    typed(&mut e, "t");
    assert_eq!(md(&e), "- [ ] t");
}

#[test]
fn a_quote_marker_wraps_the_block_in_a_blockquote() {
    let mut e = empty();
    typed(&mut e, "> citation");
    assert_eq!(md(&e), "> citation");
}

#[test]
fn markers_do_nothing_away_from_the_start_in_code_or_without_the_space() {
    let mut e = empty();
    typed(&mut e, "a # ");
    assert_eq!(e.block_kind(0), Kind::Paragraph);
    let mut e = empty();
    typed(&mut e, "#x");
    assert_eq!(e.block_text(0), "#x");
    let mut e = empty();
    e.toggle_code_block();
    typed(&mut e, "# ");
    assert_eq!(e.block_kind(0), Kind::CodeBlock);
    assert_eq!(e.block_text(0), "# ");
}

#[test]
fn undo_after_a_conversion_gives_back_the_typed_marker() {
    let mut e = empty();
    typed(&mut e, "# ");
    assert_eq!(e.block_kind(0), Kind::Heading);
    e.undo();
    assert_eq!(e.block_kind(0), Kind::Paragraph);
    assert_eq!(e.block_text(0), "# ");
    e.undo();
    assert_eq!(e.block_text(0), "");
}

#[test]
fn three_backticks_and_enter_open_a_code_block_with_its_language() {
    let mut e = empty();
    typed(&mut e, "```");
    e.split_block();
    assert_eq!(e.block_kind(0), Kind::CodeBlock);
    assert_eq!(e.block_text(0), "");
    typed(&mut e, "ls");
    assert_eq!(md(&e), "```\nls\n```");
    let mut e = empty();
    typed(&mut e, "```js");
    e.split_block();
    typed(&mut e, "a");
    assert_eq!(md(&e), "```js\na\n```");
}

#[test]
fn three_dashes_and_enter_insert_a_horizontal_rule() {
    let mut e = empty();
    typed(&mut e, "avant");
    e.split_block();
    typed(&mut e, "---");
    e.split_block();
    typed(&mut e, "après");
    assert_eq!(md(&e), "avant\n\n---\n\naprès");
    assert_eq!(
        e.selection().head,
        Pos {
            block: 1,
            offset: 5
        }
    );
}

#[test]
fn inline_delimiters_apply_marks_when_closed() {
    let cases = [
        ("a **gras**", "a **gras**", MarkKind::Bold),
        ("a *it*", "a *it*", MarkKind::Italic),
        ("a _it_", "a *it*", MarkKind::Italic),
        ("a `x`", "a `x`", MarkKind::Code),
        ("a ~~s~~", "a ~~s~~", MarkKind::Strike),
    ];
    for (input, expected, kind) in cases {
        let mut e = empty();
        typed(&mut e, input);
        assert_eq!(md(&e), expected, "{input}");
        e.move_to_doc_end(false);
        e.move_left(true);
        assert!(e.is_mark_active(&kind), "{input}");
        assert!(
            !e.block_text(0).contains(['*', '`', '~', '_']),
            "{input} : {}",
            e.block_text(0)
        );
    }
}

#[test]
fn text_typed_after_an_inline_rule_is_not_marked() {
    let mut e = empty();
    typed(&mut e, "**gras** suite");
    assert_eq!(md(&e), "**gras** suite");
}

#[test]
fn delimiters_around_spaces_or_alone_stay_plain_text() {
    for input in [
        "** x**",
        "**x **",
        "a * b *",
        "2 * 3 * 4",
        "snake_case_name",
        "****",
        "``",
    ] {
        let mut e = empty();
        typed(&mut e, input);
        assert_eq!(e.block_text(0), input, "{input}");
    }
}

#[test]
fn inline_rules_are_ignored_in_code_blocks_and_undo_restores_the_delimiters() {
    let mut e = empty();
    e.toggle_code_block();
    typed(&mut e, "**x**");
    assert_eq!(e.block_text(0), "**x**");
    let mut e = empty();
    typed(&mut e, "**x**");
    assert_eq!(e.block_text(0), "x");
    e.undo();
    assert_eq!(e.block_text(0), "**x**");
}

#[test]
fn inline_rules_work_in_the_middle_of_a_block() {
    let mut e = Editor::new(Node::from_markdown("début fin"));
    e.set_selection(Selection::caret(Pos {
        block: 0,
        offset: 6,
    }));
    typed(&mut e, "*m*");
    assert_eq!(md(&e), "début *m*fin");
}

#[test]
fn block_commands_replace_the_typed_text_in_one_undo_step() {
    use gpui_editor_core::BlockCommand;
    let cases = [
        (BlockCommand::Heading(2), "## x"),
        (BlockCommand::BulletList, "- x"),
        (BlockCommand::OrderedList, "1. x"),
        (BlockCommand::TaskList, "- [ ] x"),
        (BlockCommand::Quote, "> x"),
        (BlockCommand::CodeBlock, "```\nx\n```"),
    ];
    for (command, expected) in cases {
        let mut e = empty();
        typed(&mut e, "/tit");
        e.replace_range_with_block(
            Pos {
                block: 0,
                offset: 0,
            },
            Pos {
                block: 0,
                offset: 4,
            },
            command,
        );
        assert_eq!(e.block_text(0), "", "{command:?}");
        typed(&mut e, "x");
        assert_eq!(md(&e), expected, "{command:?}");
        e.undo();
        e.undo();
        assert_eq!(e.block_text(0), "/tit", "{command:?}");
        assert_eq!(e.block_kind(0), Kind::Paragraph, "{command:?}");
    }
}

#[test]
fn the_rule_command_inserts_a_separator_and_moves_the_caret_after_it() {
    use gpui_editor_core::BlockCommand;
    let mut e = empty();
    typed(&mut e, "avant");
    e.split_block();
    typed(&mut e, "/");
    e.replace_range_with_block(
        Pos {
            block: 1,
            offset: 0,
        },
        Pos {
            block: 1,
            offset: 1,
        },
        BlockCommand::Rule,
    );
    typed(&mut e, "après");
    assert_eq!(md(&e), "avant\n\n---\n\naprès");
    let mut e = empty();
    typed(&mut e, "texte /");
    e.replace_range_with_block(
        Pos {
            block: 0,
            offset: 5,
        },
        Pos {
            block: 0,
            offset: 7,
        },
        BlockCommand::Rule,
    );
    assert_eq!(md(&e), "texte\n\n---");
}

#[test]
fn enter_on_an_empty_last_line_leaves_a_code_block() {
    let mut e = empty();
    typed(&mut e, "```sh");
    e.split_block();
    typed(&mut e, "ls");
    e.split_block();
    assert_eq!(
        e.block_text(0),
        "ls\n",
        "un premier Entrée ajoute une ligne"
    );
    e.split_block();
    assert_eq!(e.block_count(), 2, "le second Entrée sort du bloc");
    assert_eq!(e.block_text(0), "ls");
    assert_eq!(e.block_kind(1), Kind::Paragraph);
    assert_eq!(
        e.selection().head,
        Pos {
            block: 1,
            offset: 0
        }
    );
    typed(&mut e, "après");
    assert_eq!(md(&e), "```sh\nls\n```\n\naprès");
}

#[test]
fn enter_inside_a_code_block_keeps_adding_lines_and_a_lone_empty_block_stays() {
    let mut e = empty();
    typed(&mut e, "```");
    e.split_block();
    e.split_block();
    assert_eq!(
        e.block_count(),
        1,
        "un bloc vide garde sa première ligne vide"
    );
    assert_eq!(e.block_text(0), "\n");
    e.split_block();
    assert_eq!(e.block_count(), 2);
    assert_eq!(e.block_kind(0), Kind::CodeBlock);
    assert_eq!(e.block_kind(1), Kind::Paragraph);
    let mut e = empty();
    typed(&mut e, "```");
    e.split_block();
    typed(&mut e, "a");
    e.split_block();
    typed(&mut e, "b");
    e.move_to_block_start(false);
    e.split_block();
    assert_eq!(
        e.block_text(0),
        "\na\nb",
        "Entrée au milieu insère une ligne"
    );
    assert_eq!(e.block_count(), 1);
}
