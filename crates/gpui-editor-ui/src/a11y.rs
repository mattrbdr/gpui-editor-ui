//! Accessibility: the document exposed to assistive technology through AccessKit.
//!
//! The focused editor element has the [`Role::MultilineTextInput`] role. Its text is published as
//! synthetic [`Role::TextRun`] children, one per text block (every block but the last ends with a
//! line break), and the selection as a [`TextSelection`] over those runs. Screen readers can move
//! the selection back with the `SetTextSelection` action.

use gpui::accesskit::{self, NodeId, Role, TextPosition, TextSelection};
use gpui_editor_core::{Editor, Kind, Pos, Selection};

/// One text run per text block, in document order.
pub(crate) fn text_runs(editor: &Editor) -> Vec<accesskit::Node> {
    let count = editor.block_count();
    (0..count)
        .map(|block| {
            let mut text = editor.block_text(block);
            if block + 1 < count {
                text.push('\n');
            }
            let mut run = accesskit::Node::new(Role::TextRun);
            run.set_character_lengths(
                text.chars()
                    .map(|c| u8::try_from(c.len_utf8()).unwrap_or(4))
                    .collect::<Vec<_>>(),
            );
            run.set_value(text);
            if let Some(description) = describe(editor, block) {
                run.set_description(description);
            }
            run
        })
        .collect()
}

/// What kind of block a run belongs to, when it is not a plain paragraph.
fn describe(editor: &Editor, block: usize) -> Option<String> {
    match editor.block_kind(block) {
        Kind::Heading => {
            let level = editor
                .block_node(block)
                .attrs
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            Some(format!("heading level {level}"))
        }
        Kind::CodeBlock => Some("code block".to_owned()),
        _ => None,
    }
}

/// The editor selection over the text runs whose ids are `runs` (one per block).
pub(crate) fn text_selection(selection: Selection, runs: &[NodeId]) -> Option<TextSelection> {
    let position = |pos: Pos| {
        Some(TextPosition {
            node: *runs.get(pos.block)?,
            character_index: pos.offset,
        })
    };
    Some(TextSelection {
        anchor: position(selection.anchor)?,
        focus: position(selection.head)?,
    })
}

/// The editor selection asked for by assistive technology, or `None` if a position does not
/// name one of the runs.
pub(crate) fn selection_from(selection: &TextSelection, runs: &[NodeId]) -> Option<Selection> {
    let pos = |position: &TextPosition| {
        Some(Pos {
            block: runs.iter().position(|id| *id == position.node)?,
            offset: position.character_index,
        })
    };
    Some(Selection {
        anchor: pos(&selection.anchor)?,
        head: pos(&selection.focus)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_editor_core::Node;

    fn ids(count: u64) -> Vec<NodeId> {
        (0..count).map(|i| NodeId(100 + i)).collect()
    }

    #[test]
    fn each_text_block_is_a_run_ending_with_a_line_break_except_the_last() {
        let editor = Editor::new(Node::from_markdown("## Title\n\nHé 🇫🇷\n\n```\nlet a;\n```"));
        let runs = text_runs(&editor);
        let values: Vec<_> = runs.iter().map(|run| run.value().unwrap()).collect();
        assert_eq!(values, ["Title\n", "Hé 🇫🇷\n", "let a;"]);
        assert!(runs.iter().all(|run| run.role() == Role::TextRun));
        assert_eq!(runs[1].character_lengths(), &[1, 2, 1, 4, 4, 1]);
        assert_eq!(runs[0].description(), Some("heading level 2"));
        assert_eq!(runs[1].description(), None);
        assert_eq!(runs[2].description(), Some("code block"));
    }

    #[test]
    fn the_selection_maps_to_run_positions_and_back() {
        let runs = ids(3);
        let selection = Selection {
            anchor: Pos {
                block: 0,
                offset: 2,
            },
            head: Pos {
                block: 2,
                offset: 1,
            },
        };
        let text = text_selection(selection, &runs).unwrap();
        assert_eq!(text.anchor.node, NodeId(100));
        assert_eq!(text.anchor.character_index, 2);
        assert_eq!(text.focus.node, NodeId(102));
        assert_eq!(selection_from(&text, &runs), Some(selection));
    }

    #[test]
    fn positions_outside_the_runs_are_ignored() {
        let text = TextSelection {
            anchor: TextPosition {
                node: NodeId(7),
                character_index: 0,
            },
            focus: TextPosition {
                node: NodeId(100),
                character_index: 0,
            },
        };
        assert_eq!(selection_from(&text, &ids(2)), None);
        assert_eq!(text_selection(Selection::default(), &[]), None);
    }
}
