use gpui_editor_core::{Editor, Node, Pos, Selection, looks_like_markdown};

fn editor(markdown: &str) -> Editor {
    Editor::new(Node::from_markdown(markdown))
}

fn select(e: &mut Editor, from: (usize, usize), to: (usize, usize)) {
    e.set_selection(Selection {
        anchor: Pos {
            block: from.0,
            offset: from.1,
        },
        head: Pos {
            block: to.0,
            offset: to.1,
        },
    });
}

fn markdown(e: &Editor) -> String {
    e.doc().to_markdown()
}

#[test]
fn plain_paragraphs_and_code_are_copied_as_plain_text() {
    let mut e = editor("a * b\n\nsuite\n\n```\n**x**\n```");
    select(&mut e, (0, 0), (1, 5));
    assert_eq!(e.selected_markdown(), None);
    select(&mut e, (2, 0), (2, 5));
    assert_eq!(e.selected_markdown(), None);
}

#[test]
fn a_formatted_part_of_a_block_is_copied_as_inline_markdown() {
    let mut e = editor("## Un **mot** gras");
    select(&mut e, (0, 0), (0, 6));
    assert_eq!(e.selected_markdown().as_deref(), Some("Un **mot**"));
}

#[test]
fn several_structured_blocks_keep_their_structure() {
    let mut e = editor("# Titre\n\n- un\n- deux\n\nfin");
    select(&mut e, (0, 0), (2, 2));
    assert_eq!(
        e.selected_markdown().as_deref(),
        Some("# Titre\n\n- un\n- de")
    );
}

#[test]
fn pasted_inline_markdown_joins_the_current_block() {
    let mut e = editor("avant après");
    select(&mut e, (0, 6), (0, 6));
    e.paste_markdown("**gras** ");
    assert_eq!(markdown(&e), "avant **gras** après");
    assert_eq!(e.selection().head.offset, 11);
}

#[test]
fn pasted_blocks_are_inserted_where_the_caret_is_in_one_undo_step() {
    let mut e = editor("avantaprès");
    let before = e.doc().clone();
    select(&mut e, (0, 5), (0, 5));
    e.paste_markdown("# Titre\n\n- un");
    assert_eq!(markdown(&e), "avant\n\n# Titre\n\n- un\n\naprès");
    assert_eq!(
        e.selection().head,
        Pos {
            block: 2,
            offset: 2
        }
    );
    e.undo();
    assert_eq!(*e.doc(), before);
}

#[test]
fn pasting_blocks_into_an_empty_paragraph_replaces_it() {
    let mut e = editor("");
    e.paste_markdown("# Titre\n\ntexte");
    assert_eq!(markdown(&e), "# Titre\n\ntexte");
}

#[test]
fn a_list_pasted_into_a_list_adds_items() {
    let mut e = editor("- un\n- trois");
    select(&mut e, (0, 2), (0, 2));
    e.paste_markdown("- deux\n- deux bis");
    assert_eq!(markdown(&e), "- un\n- deux\n- deux bis\n- trois");
}

#[test]
fn only_unambiguous_markdown_is_recognised() {
    assert!(looks_like_markdown("# Titre\n\ntexte"));
    assert!(looks_like_markdown("- un\n- deux"));
    assert!(looks_like_markdown("```\nls\npwd\n```"));
    assert!(looks_like_markdown("Voir [ici](https://x.fr)\n\n**fin**"));
    // Lines that Markdown would join together stay as they are.
    assert!(!looks_like_markdown("erreur 1\nerreur 2"));
    assert!(!looks_like_markdown("- option\njournal\njournal"));
    assert!(!looks_like_markdown("    indented\n    config"));
    assert!(!looks_like_markdown("une ligne"));
}

#[test]
fn undo_removes_one_word_at_a_time() {
    let mut e = editor("");
    for c in "un deux".chars() {
        e.insert_text(&c.to_string());
    }
    e.undo();
    assert_eq!(e.block_text(0), "un ");
    e.undo();
    assert_eq!(e.block_text(0), "");
}

#[test]
fn the_revision_only_moves_when_the_document_changes() {
    let mut e = editor("a");
    let start = e.revision();
    e.undo();
    e.delete_backward();
    e.indent();
    assert_eq!(e.revision(), start);
    e.insert_text("b");
    assert_ne!(e.revision(), start);
}
