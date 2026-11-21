//! Deletion, typing and splitting blocks.

use serde_json::{Map, Value};

use crate::{Kind, Mark, Node};

use super::{
    BlockCommand, Editor, Pos, Selection,
    atoms::{Atom, mention_atoms, mention_mark, next_bound, prev_bound, rebuild, word_spans},
    tree::{Path, blocks_in_order, is_item, node_at, remove_at},
};

impl Editor {
    // --- Deletion ---

    /// Deletes the `[start, end)` range and returns the position where the blocks were joined.
    pub(super) fn delete_range(&mut self, start: Pos, end: Pos) -> Pos {
        if start == end {
            return start;
        }
        if start.block == end.block {
            let mut atoms = self.atoms(start.block);
            atoms.drain(start.offset.min(atoms.len())..end.offset.min(atoms.len()));
            self.set_atoms(start.block, &atoms);
            return start;
        }
        let textblocks = self.textblocks();
        let (start_path, end_path) = (
            textblocks[start.block].clone(),
            textblocks[end.block].clone(),
        );
        let mut merged = self.atoms(start.block);
        merged.truncate(start.offset);
        let end_atoms = self.atoms(end.block);
        merged.extend(end_atoms.into_iter().skip(end.offset));
        let order = blocks_in_order(&self.doc);
        let from = order
            .iter()
            .position(|(path, _)| *path == start_path)
            .unwrap_or(0);
        let to = order
            .iter()
            .position(|(path, _)| *path == end_path)
            .unwrap_or(from);
        let mut doomed: Vec<Path> = order[from + 1..=to]
            .iter()
            .map(|(path, _)| path.clone())
            .collect();
        rebuild(self.node_mut(&start_path), &merged);
        doomed.sort();
        for path in doomed.iter().rev() {
            remove_at(self.doc_mut(), path);
        }
        self.ensure_textblock();
        start
    }

    pub(super) fn ensure_textblock(&mut self) {
        if !blocks_in_order(&self.doc).iter().any(|(_, text)| *text) {
            self.doc_mut().content.push(Node::new(Kind::Paragraph));
        }
    }

    pub(super) fn delete_selection_inner(&mut self) {
        let (start, end) = (self.selection.start(), self.selection.end());
        let pos = self.delete_range(start, end);
        self.selection = Selection::caret(pos);
    }

    /// Whether an unknown block (database view, image…) lies between text blocks `first` and
    /// `last`.
    pub(super) fn unknown_block_between(&self, first: usize, last: usize) -> bool {
        let textblocks = self.textblocks();
        let order = blocks_in_order(&self.doc);
        let position = |block: usize| {
            order
                .iter()
                .position(|(path, _)| *path == textblocks[block])
        };
        let (Some(from), Some(to)) = (position(first), position(last)) else {
            return false;
        };
        order[from + 1..to]
            .iter()
            .any(|(path, text)| !*text && matches!(node_at(&self.doc, path).kind, Kind::Other(_)))
    }

    /// Backspace: deletes the selection or the previous grapheme. At the start of a block, turns
    /// a non-paragraph into a paragraph, or joins a paragraph with the previous block (not across
    /// an unknown block).
    pub fn delete_backward(&mut self) {
        // Joining across an unknown block would delete it without the user seeing it: do nothing.
        let head = self.selection.head;
        if self.selection.is_collapsed()
            && head.offset == 0
            && self.block_kind(head.block) == Kind::Paragraph
            && head.block > 0
            && self.unknown_block_between(head.block - 1, head.block)
        {
            return;
        }
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
            return;
        }
        let head = self.selection.head;
        if head.offset > 0 {
            let from = prev_bound(&self.block_text(head.block), head.offset);
            let pos = self.delete_range(
                Pos {
                    block: head.block,
                    offset: from,
                },
                head,
            );
            self.selection = Selection::caret(pos);
        } else if self.block_kind(head.block) != Kind::Paragraph {
            self.convert_block(head.block, Kind::Paragraph, Map::new());
        } else if head.block > 0 {
            let previous = Pos {
                block: head.block - 1,
                offset: self.block_len(head.block - 1),
            };
            let pos = self.delete_range(previous, head);
            self.selection = Selection::caret(pos);
        }
    }

    /// Delete: deletes the selection or the next grapheme; at the end of a block, joins the next
    /// block (not across an unknown block).
    pub fn delete_forward(&mut self) {
        let head = self.selection.head;
        if self.selection.is_collapsed()
            && head.offset == self.block_len(head.block)
            && head.block + 1 < self.block_count()
            && self.unknown_block_between(head.block, head.block + 1)
        {
            return;
        }
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
            return;
        }
        let head = self.selection.head;
        let len = self.block_len(head.block);
        if head.offset < len {
            let to = next_bound(&self.block_text(head.block), head.offset);
            self.delete_range(
                head,
                Pos {
                    block: head.block,
                    offset: to,
                },
            );
        } else if head.block + 1 < self.block_count() {
            self.delete_range(
                head,
                Pos {
                    block: head.block + 1,
                    offset: 0,
                },
            );
        }
    }

    // --- Typing ---

    /// Inserts text (replacing the selection). A line break splits the block, except in code.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let coalesce = self.selection.is_collapsed()
            && !text.contains('\n')
            && self.typing == Some(self.selection.head);
        if coalesce {
            self.redo.clear();
        } else {
            self.checkpoint();
        }
        self.insert_text_raw(text);
        let converted = match text {
            " " => self.block_rule(),
            "*" | "`" | "~" | "_" => self.inline_rule(text.chars().next().unwrap_or(' ')),
            _ => false,
        };
        self.typing = (!converted && !text.contains('\n')).then_some(self.selection.head);
    }

    pub(super) fn insert_text_raw(&mut self, text: &str) {
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        let marks = self.marks_at_caret();
        let mut segments = text.split('\n').peekable();
        while let Some(segment) = segments.next() {
            self.insert_in_block(segment, &marks);
            if segments.peek().is_some() {
                self.split_inner();
            }
        }
        self.stored_marks = None;
    }

    /// Id of the mention at a position (the character before or after), if any.
    #[must_use]
    pub fn mention_near(&self, pos: Pos) -> Option<String> {
        let pos = self.clamp(pos);
        let atoms = self.atoms(pos.block);
        [Some(pos.offset), pos.offset.checked_sub(1)]
            .into_iter()
            .flatten()
            .filter_map(|index| atoms.get(index))
            .find_map(|atom| {
                mention_mark(atom)
                    .and_then(|m| m.attrs.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }

    /// Replaces `[start, end)` (for example `[[query`) with a `label` mention pointing to `id`, as
    /// a single history step; the caret is placed right after it.
    pub fn insert_mention(&mut self, start: Pos, end: Pos, id: &str, label: &str) {
        self.checkpoint();
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.selection = Selection {
            anchor: start,
            head: end,
        };
        if start != end {
            self.delete_selection_inner();
        }
        let head = self.selection.head;
        if self.block_kind(head.block) == Kind::CodeBlock {
            return;
        }
        let mut attrs = Map::new();
        attrs.insert("id".into(), Value::String(id.to_owned()));
        attrs.insert("label".into(), Value::String(label.to_owned()));
        attrs.insert(
            "mentionSuggestionChar".into(),
            Value::String("[[".to_owned()),
        );
        let inserted = mention_atoms(&attrs, &[]);
        let count = inserted.len();
        let mut atoms = self.atoms(head.block);
        let at = head.offset.min(atoms.len());
        atoms.splice(at..at, inserted);
        self.set_atoms(head.block, &atoms);
        self.selection = Selection::caret(Pos {
            block: head.block,
            offset: at + count,
        });
        self.stored_marks = None;
        self.typing = None;
    }

    /// Opens a batch: a single history step for the unrecorded `replace_range` calls that follow
    /// (IME composition).
    pub fn begin_batch(&mut self) {
        self.checkpoint();
    }

    /// Replaces `[start, end)` with `text` and places the caret after it. `record` adds a history
    /// step; otherwise the edit joins the batch opened by [`Editor::begin_batch`].
    pub fn replace_range(&mut self, start: Pos, end: Pos, text: &str, record: bool) {
        if record {
            self.checkpoint();
        } else {
            self.redo.clear();
            self.typing = None;
        }
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.selection = Selection {
            anchor: start,
            head: end,
        };
        if text.is_empty() {
            self.delete_selection_inner();
        } else {
            self.insert_text_raw(text);
        }
        self.typing = None;
    }

    /// Removes `[start, end)` (the typed command text) and then converts the block, as a single
    /// history step.
    pub fn replace_range_with_block(&mut self, start: Pos, end: Pos, command: BlockCommand) {
        self.checkpoint();
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.selection = Selection {
            anchor: start,
            head: end,
        };
        self.delete_selection_inner();
        let block = self.selection.head.block;
        match command {
            BlockCommand::Paragraph => self.convert_block(block, Kind::Paragraph, Map::new()),
            BlockCommand::Heading(level) => {
                self.convert_block(block, Kind::Heading, Self::attrs_of(Some(u64::from(level))));
            }
            BlockCommand::BulletList => self.ensure_list(Kind::BulletList),
            BlockCommand::OrderedList => self.ensure_list(Kind::OrderedList),
            BlockCommand::TaskList => self.ensure_list(Kind::TaskList),
            BlockCommand::Quote => self.wrap_in_blockquote(block),
            BlockCommand::CodeBlock => {
                let mut attrs = Map::new();
                attrs.insert("language".into(), Value::Null);
                self.convert_block(block, Kind::CodeBlock, attrs);
            }
            BlockCommand::Rule => self.insert_rule_after(block),
        }
    }

    /// Inserts a horizontal rule (replacing the block if it is empty) followed by a paragraph.
    pub(super) fn insert_rule_after(&mut self, block: usize) {
        let path = self.path_of(block);
        let empty = self.block_len(block) == 0;
        let Some((index, parent)) = path.split_last() else {
            return;
        };
        let container = self.node_mut(parent);
        let at = if empty {
            container.content[*index] = Node::new(Kind::HorizontalRule);
            *index + 1
        } else {
            container
                .content
                .insert(*index + 1, Node::new(Kind::HorizontalRule));
            *index + 2
        };
        container.content.insert(at, Node::new(Kind::Paragraph));
        let mut fresh = parent.to_vec();
        fresh.push(at);
        let target = self
            .textblocks()
            .iter()
            .position(|p| *p == fresh)
            .unwrap_or(block);
        self.selection = Selection::caret(Pos {
            block: target,
            offset: 0,
        });
    }

    /// Selects the word containing `pos` (no effect outside a word).
    pub fn select_word_at(&mut self, pos: Pos) {
        let pos = self.clamp(pos);
        let spans = word_spans(&self.block_text(pos.block));
        let found = spans
            .iter()
            .find(|(start, end)| *start <= pos.offset && pos.offset <= *end);
        self.selection = match found {
            Some((start, end)) => Selection {
                anchor: Pos {
                    block: pos.block,
                    offset: *start,
                },
                head: Pos {
                    block: pos.block,
                    offset: *end,
                },
            },
            None => Selection::caret(pos),
        };
        self.stored_marks = None;
        self.typing = None;
    }

    /// Selects all the text of a block.
    pub fn select_block(&mut self, block: usize) {
        let block = block.min(self.block_count() - 1);
        self.selection = Selection {
            anchor: Pos { block, offset: 0 },
            head: Pos {
                block,
                offset: self.block_len(block),
            },
        };
        self.stored_marks = None;
        self.typing = None;
    }

    pub(super) fn insert_in_block(&mut self, text: &str, marks: &[Mark]) {
        let head = self.selection.head;
        let mut atoms = self.atoms(head.block);
        let code = self.block_kind(head.block) == Kind::CodeBlock;
        let inserted: Vec<Atom> = text
            .chars()
            .map(|ch| Atom {
                ch,
                marks: if code { Vec::new() } else { marks.to_vec() },
                hard_break: false,
                node: None,
            })
            .collect();
        let count = inserted.len();
        let at = head.offset.min(atoms.len());
        atoms.splice(at..at, inserted);
        self.set_atoms(head.block, &atoms);
        self.selection = Selection::caret(Pos {
            block: head.block,
            offset: at + count,
        });
    }

    /// Shift+Enter: replaces the selection with a `hardBreak` (a `\n` character in a code block).
    pub fn insert_hard_break(&mut self) {
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        let head = self.selection.head;
        if self.block_kind(head.block) == Kind::CodeBlock {
            self.insert_in_block("\n", &[]);
            return;
        }
        let mut atoms = self.atoms(head.block);
        let at = head.offset.min(atoms.len());
        atoms.insert(
            at,
            Atom {
                ch: '\n',
                marks: Vec::new(),
                hard_break: true,
                node: None,
            },
        );
        self.set_atoms(head.block, &atoms);
        self.selection = Selection::caret(Pos {
            block: head.block,
            offset: at + 1,
        });
    }

    /// Enter key.
    pub fn split_block(&mut self) {
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        if self.enter_rule() || self.leave_code_block() {
            return;
        }
        if self.block_kind(self.selection.head.block) == Kind::CodeBlock {
            self.insert_in_block("\n", &[]);
        } else {
            self.split_inner();
        }
    }

    pub(super) fn split_inner(&mut self) {
        let head = self.selection.head;
        let path = self.path_of(head.block);
        let kind = node_at(&self.doc, &path).kind.clone();
        if kind == Kind::CodeBlock {
            self.insert_in_block("\n", &[]);
            return;
        }
        let atoms = self.atoms(head.block);
        let at = head.offset.min(atoms.len());
        let (left, right) = atoms.split_at(at);
        let parent_path = &path[..path.len() - 1];
        let in_item = is_item(&node_at(&self.doc, parent_path).kind);
        if in_item && atoms.is_empty() && node_at(&self.doc, parent_path).content.len() == 1 {
            self.exit_list(&path);
            return;
        }
        let mut new_block = Node::new(if at == atoms.len() && kind == Kind::Heading {
            Kind::Paragraph
        } else {
            kind.clone()
        });
        if new_block.kind == kind {
            new_block.attrs.clone_from(&node_at(&self.doc, &path).attrs);
        }
        rebuild(&mut new_block, right);
        rebuild(self.node_mut(&path), left);
        let index = path[path.len() - 1];
        let new_path = if in_item {
            let item_index = parent_path[parent_path.len() - 1];
            let list_path = &parent_path[..parent_path.len() - 1];
            let following: Vec<Node> = self
                .node_mut(parent_path)
                .content
                .drain(index + 1..)
                .collect();
            let mut item = Node::new(node_at(&self.doc, parent_path).kind.clone());
            if item.kind == Kind::TaskItem {
                item.attrs.insert("checked".into(), Value::Bool(false));
            }
            item.content.push(new_block);
            item.content.extend(following);
            self.node_mut(list_path)
                .content
                .insert(item_index + 1, item);
            let mut new_path = list_path.to_vec();
            new_path.extend([item_index + 1, 0]);
            new_path
        } else {
            self.node_mut(parent_path)
                .content
                .insert(index + 1, new_block);
            let mut new_path = parent_path.to_vec();
            new_path.push(index + 1);
            new_path
        };
        let block = self
            .textblocks()
            .iter()
            .position(|p| *p == new_path)
            .unwrap_or(head.block + 1);
        self.selection = Selection::caret(Pos { block, offset: 0 });
    }

    /// Leaves a list from an empty item: the item becomes a paragraph outside the list.
    pub(super) fn exit_list(&mut self, block_path: &[usize]) {
        let item_path = &block_path[..block_path.len() - 1];
        let list_path = &item_path[..item_path.len() - 1];
        let (item_index, list_parent) = (
            item_path[item_path.len() - 1],
            &list_path[..list_path.len() - 1],
        );
        let list_index = list_path[list_path.len() - 1];
        let list_len = node_at(&self.doc, list_path).content.len();
        let paragraph = Node::new(Kind::Paragraph);
        let target = if list_len == 1 {
            self.node_mut(list_parent).content[list_index] = paragraph;
            list_path.to_vec()
        } else if item_index + 1 == list_len {
            self.node_mut(list_path).content.pop();
            self.node_mut(list_parent)
                .content
                .insert(list_index + 1, paragraph);
            let mut path = list_parent.to_vec();
            path.push(list_index + 1);
            path
        } else if item_index == 0 {
            self.node_mut(list_path).content.remove(0);
            self.node_mut(list_parent)
                .content
                .insert(list_index, paragraph);
            let mut path = list_parent.to_vec();
            path.push(list_index);
            path
        } else {
            let tail: Vec<Node> = self
                .node_mut(list_path)
                .content
                .drain(item_index..)
                .collect();
            let mut second = Node::new(node_at(&self.doc, list_path).kind.clone());
            second
                .attrs
                .clone_from(&node_at(&self.doc, list_path).attrs);
            second.content = tail.into_iter().skip(1).collect();
            let parent = self.node_mut(list_parent);
            parent.content.insert(list_index + 1, paragraph);
            if !second.content.is_empty() {
                parent.content.insert(list_index + 2, second);
            }
            let mut path = list_parent.to_vec();
            path.push(list_index + 1);
            path
        };
        let block = self
            .textblocks()
            .iter()
            .position(|p| *p == target)
            .unwrap_or(0);
        self.selection = Selection::caret(Pos { block, offset: 0 });
    }
}
