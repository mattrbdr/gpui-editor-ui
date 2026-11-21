//! Markdown input rules.

use serde_json::{Map, Value};

use crate::{Kind, Mark, MarkKind, Node};

use super::{
    Editor, Pos, Selection,
    atoms::{Atom, find_opening, flat_text},
    tree::{node_at, node_at_mut},
};

impl Editor {
    pub(super) fn block_rule(&mut self) -> bool {
        enum Rule {
            Heading(u64),
            Bullet,
            Ordered(i64),
            Task(bool),
            Quote,
        }
        let head = self.selection.head;
        if !self.selection.is_collapsed() || self.block_kind(head.block) != Kind::Paragraph {
            return false;
        }
        let atoms = self.atoms(head.block);
        let typed: Vec<&Atom> = atoms.iter().take(head.offset).collect();
        if typed.iter().any(|atom| atom.hard_break) {
            return false;
        }
        let prefix: String = typed.iter().map(|atom| atom.ch).collect();
        let Some(marker) = prefix.strip_suffix(' ') else {
            return false;
        };
        let rule = match marker {
            "-" | "*" | "+" => Rule::Bullet,
            ">" => Rule::Quote,
            "[ ]" => Rule::Task(false),
            "[x]" | "[X]" => Rule::Task(true),
            hashes
                if !hashes.is_empty() && hashes.len() <= 6 && hashes.chars().all(|c| c == '#') =>
            {
                Rule::Heading(hashes.len() as u64)
            }
            number => match number
                .strip_suffix('.')
                .filter(|n| !n.is_empty() && n.len() <= 9 && n.chars().all(|c| c.is_ascii_digit()))
            {
                Some(digits) => Rule::Ordered(digits.parse().unwrap_or(1)),
                None => return false,
            },
        };
        self.push_snapshot();
        let rest: Vec<Atom> = atoms[head.offset..].to_vec();
        let block = head.block;
        self.set_atoms(block, &rest);
        self.selection = Selection::caret(Pos { block, offset: 0 });
        match rule {
            Rule::Heading(level) => {
                self.convert_block(block, Kind::Heading, Self::attrs_of(Some(level)));
            }
            Rule::Bullet => self.ensure_list(Kind::BulletList),
            Rule::Ordered(start) => {
                self.ensure_list(Kind::OrderedList);
                if start != 1
                    && let Some((list_path, _)) = self.list_context(block)
                {
                    node_at_mut(self.doc_mut(), &list_path)
                        .attrs
                        .insert("start".into(), Value::from(start));
                }
            }
            Rule::Task(checked) => {
                self.ensure_list(Kind::TaskList);
                if let Some((list_path, item)) = self.list_context(block) {
                    let item_node = &mut node_at_mut(self.doc_mut(), &list_path).content[item];
                    item_node
                        .attrs
                        .insert("checked".into(), Value::Bool(checked));
                }
            }
            Rule::Quote => self.wrap_in_blockquote(block),
        }
        true
    }

    /// Wraps the current block in a list of the given type (leaving it as is if already there).
    pub(super) fn ensure_list(&mut self, kind: Kind) {
        let block = self.selection.head.block;
        let already = self
            .list_context(block)
            .is_some_and(|(list, _)| node_at(&self.doc, &list).kind == kind);
        if !already {
            self.toggle_list_raw(kind);
        }
    }

    pub(super) fn wrap_in_blockquote(&mut self, block: usize) {
        let path = self.path_of(block);
        let Some((index, parent)) = path.split_last() else {
            return;
        };
        let container = node_at_mut(self.doc_mut(), parent);
        let child = container.content.remove(*index);
        let mut quote = Node::new(Kind::Blockquote);
        quote.content.push(child);
        container.content.insert(*index, quote);
    }

    /// Wraps the current block in a blockquote, or unwraps it.
    pub fn toggle_blockquote(&mut self) {
        self.checkpoint();
        let block = self.selection.head.block;
        let path = self.path_of(block);
        let parent_path = &path[..path.len() - 1];
        if node_at(&self.doc, parent_path).kind == Kind::Blockquote {
            if let Some((index, above)) = parent_path.split_last() {
                let quote = node_at(&self.doc, parent_path).clone();
                node_at_mut(self.doc_mut(), above)
                    .content
                    .splice(*index..=*index, quote.content);
            }
        } else {
            self.wrap_in_blockquote(block);
        }
    }

    /// Closes an inline delimiter (`**`, `*`, `_`, `` ` ``, `~~`): applies the mark to the enclosed
    /// text and removes the delimiters.
    pub(super) fn inline_rule(&mut self, typed: char) -> bool {
        let head = self.selection.head;
        if !self.selection.is_collapsed() || self.block_kind(head.block) == Kind::CodeBlock {
            return false;
        }
        let atoms = self.atoms(head.block);
        let end = head.offset.min(atoms.len());
        let chars: Vec<char> = atoms[..end].iter().map(|atom| atom.ch).collect();
        let candidates: &[(&str, MarkKind)] = match typed {
            '`' => &[("`", MarkKind::Code)],
            '~' => &[("~~", MarkKind::Strike)],
            '_' => &[("_", MarkKind::Italic)],
            '*' => &[("**", MarkKind::Bold), ("*", MarkKind::Italic)],
            _ => return false,
        };
        for (delimiter, kind) in candidates {
            let delimiter: Vec<char> = delimiter.chars().collect();
            if let Some(open) = find_opening(&chars, &delimiter) {
                let n = delimiter.len();
                let closing = end - n;
                let spans_ok = atoms[open..end]
                    .iter()
                    .all(|a| !a.hard_break && !a.marks.iter().any(|m| m.kind == MarkKind::Code));
                if !spans_ok {
                    continue;
                }
                self.push_snapshot();
                let outer: Vec<Mark> = open
                    .checked_sub(1)
                    .map(|i| atoms[i].marks.clone())
                    .unwrap_or_default();
                let mut result: Vec<Atom> = atoms[..open].to_vec();
                for atom in &atoms[open + n..closing] {
                    let mut marked = atom.clone();
                    if *kind == MarkKind::Code {
                        marked.marks = vec![Mark::new(MarkKind::Code)];
                    } else {
                        marked.marks.retain(|m| m.kind != *kind);
                        marked.marks.push(Mark::new(kind.clone()));
                    }
                    result.push(marked);
                }
                let caret = result.len();
                result.extend(atoms[end..].iter().cloned());
                self.set_atoms(head.block, &result);
                self.selection = Selection::caret(Pos {
                    block: head.block,
                    offset: caret,
                });
                self.stored_marks = Some(outer);
                return true;
            }
        }
        false
    }

    /// Enter on an empty line at the end of a code block: removes that line and leaves the block.
    pub(super) fn leave_code_block(&mut self) -> bool {
        let head = self.selection.head;
        if self.block_kind(head.block) != Kind::CodeBlock {
            return false;
        }
        let mut atoms = self.atoms(head.block);
        let at_end = head.offset == atoms.len();
        if !at_end || atoms.last().is_none_or(|atom| atom.ch != '\n') {
            return false;
        }
        atoms.pop();
        self.set_atoms(head.block, &atoms);
        let path = self.path_of(head.block);
        let Some((index, parent)) = path.split_last() else {
            return false;
        };
        node_at_mut(self.doc_mut(), parent)
            .content
            .insert(*index + 1, Node::new(Kind::Paragraph));
        let mut fresh = parent.to_vec();
        fresh.push(*index + 1);
        let block = self
            .textblocks()
            .iter()
            .position(|p| *p == fresh)
            .unwrap_or(head.block + 1);
        self.selection = Selection::caret(Pos { block, offset: 0 });
        true
    }

    /// Enter on ```` ```lang ````: code block; on `---`: horizontal rule.
    pub(super) fn enter_rule(&mut self) -> bool {
        let head = self.selection.head;
        if self.block_kind(head.block) != Kind::Paragraph {
            return false;
        }
        let atoms = self.atoms(head.block);
        if head.offset != atoms.len() || atoms.iter().any(|a| a.hard_break) {
            return false;
        }
        let text = flat_text(&atoms);
        if let Some(language) = text.strip_prefix("```")
            && language
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '+' | '#'))
        {
            let mut attrs = Map::new();
            attrs.insert(
                "language".into(),
                if language.is_empty() {
                    Value::Null
                } else {
                    Value::String(language.to_owned())
                },
            );
            self.set_atoms(head.block, &[]);
            self.convert_block(head.block, Kind::CodeBlock, attrs);
            self.selection = Selection::caret(Pos {
                block: head.block,
                offset: 0,
            });
            return true;
        }
        if matches!(text.as_str(), "---" | "***" | "___") {
            let path = self.path_of(head.block);
            let Some((index, parent)) = path.split_last() else {
                return false;
            };
            let container = node_at_mut(self.doc_mut(), parent);
            container.content[*index] = Node::new(Kind::HorizontalRule);
            container
                .content
                .insert(*index + 1, Node::new(Kind::Paragraph));
            let mut fresh = parent.to_vec();
            fresh.push(*index + 1);
            let block = self
                .textblocks()
                .iter()
                .position(|p| *p == fresh)
                .unwrap_or(head.block);
            self.selection = Selection::caret(Pos { block, offset: 0 });
            return true;
        }
        false
    }
}
