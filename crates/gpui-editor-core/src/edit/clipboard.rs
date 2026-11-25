//! Copying a selection as Markdown and inserting Markdown at the caret.

use crate::{Kind, Node};

use super::{
    Editor, Pos, Selection,
    atoms::{Atom, atoms_of, mention_mark, rebuild},
    tree::{is_container, is_item, node_at},
};

/// Copy of `node` restricted to the text blocks `start.block..=end.block`, the first and last
/// ones cut at the positions; `None` if nothing of it is selected. `next` numbers the text blocks.
fn slice(node: &Node, next: &mut usize, start: Pos, end: Pos) -> Option<Node> {
    if node.kind.is_textblock() {
        let block = *next;
        *next += 1;
        if block < start.block || block > end.block {
            return None;
        }
        let atoms = atoms_of(node);
        let from = if block == start.block {
            start.offset
        } else {
            0
        };
        let to = if block == end.block {
            end.offset
        } else {
            atoms.len()
        };
        let mut copy = node.clone();
        rebuild(
            &mut copy,
            &atoms[from.min(atoms.len())..to.min(atoms.len())],
        );
        return Some(copy);
    }
    if is_container(&node.kind) {
        let content: Vec<Node> = node
            .content
            .iter()
            .filter_map(|child| slice(child, next, start, end))
            .collect();
        if content.is_empty() {
            return None;
        }
        let mut copy = node.clone();
        copy.content = content;
        return Some(copy);
    }
    // Rules and unknown blocks: kept between two selected text blocks.
    (*next > start.block && *next <= end.block).then(|| node.clone())
}

/// Whether the node needs Markdown to keep its meaning: marks, or blocks other than paragraphs.
fn is_formatted(node: &Node) -> bool {
    match node.kind {
        Kind::Text => !node.marks.is_empty(),
        Kind::Doc | Kind::Paragraph => node.content.iter().any(is_formatted),
        Kind::HardBreak => false,
        Kind::Other(ref name) => name != super::atoms::MENTION,
        _ => true,
    }
}

fn text_blocks(node: &Node) -> usize {
    if node.kind.is_textblock() {
        1
    } else if is_container(&node.kind) {
        node.content.iter().map(text_blocks).sum()
    } else {
        0
    }
}

/// Whether a plain text is Markdown that can be interpreted without losing anything: it has
/// Markdown syntax on several lines, and no lines that Markdown would join or turn into code.
#[must_use]
pub fn looks_like_markdown(text: &str) -> bool {
    if !text.trim_end().contains('\n') {
        return false;
    }
    let (mut syntax, mut fenced) = (false, false);
    // The previous line would absorb a following plain line.
    let mut open = false;
    let mut in_list = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            syntax = true;
            open = false;
            continue;
        }
        if fenced {
            continue;
        }
        if trimmed.is_empty() {
            open = false;
            continue;
        }
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        let list = ["- ", "* ", "+ "].iter().any(|m| trimmed.starts_with(m))
            || (digits > 0 && trimmed[digits..].starts_with(". "));
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        let heading = (1..=6).contains(&level) && trimmed[level..].starts_with(' ');
        let quote = trimmed.starts_with('>');
        if trimmed.len() < line.len() && !(in_list && list) {
            return false;
        }
        if list || heading || quote {
            syntax = true;
            open = !heading;
        } else {
            if open {
                return false;
            }
            open = true;
            let paired = |mark: &str| line.matches(mark).count() >= 2;
            syntax |= line.contains("](") || paired("**") || paired("`");
        }
        in_list = list;
    }
    syntax && !fenced
}

impl Editor {
    // --- Markdown clipboard ---

    /// Markdown of the selection when plain text would lose its formatting: marks, or several
    /// blocks with structure. `None` for plain text and inside a code block.
    #[must_use]
    pub fn selected_markdown(&self) -> Option<String> {
        let (start, end) = (self.selection.start(), self.selection.end());
        if start == end {
            return None;
        }
        if start.block == end.block {
            if self.block_kind(start.block) == Kind::CodeBlock {
                return None;
            }
            let atoms = self.atoms(start.block);
            let part = &atoms[start.offset.min(atoms.len())..end.offset.min(atoms.len())];
            let marked = |atom: &Atom| atom.marks.len() > usize::from(mention_mark(atom).is_some());
            if !part.iter().any(marked) {
                return None;
            }
            let mut paragraph = Node::new(Kind::Paragraph);
            rebuild(&mut paragraph, part);
            return Some(paragraph.to_markdown());
        }
        let doc = slice(&self.doc, &mut 0, start, end)?;
        is_formatted(&doc).then(|| doc.to_markdown())
    }

    /// Inserts Markdown at the caret, replacing the selection, as one history step. A single
    /// paragraph joins the current block; in a code block the source is inserted as is.
    pub fn paste_markdown(&mut self, source: &str) {
        if self.block_kind(self.selection.start().block) == Kind::CodeBlock {
            self.insert_text(source);
            return;
        }
        let blocks = Node::from_markdown(source).content;
        let Some(first) = blocks.first() else {
            return;
        };
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        if blocks.len() == 1 && first.kind == Kind::Paragraph {
            let head = self.selection.head;
            let mut atoms = self.atoms(head.block);
            let inserted = atoms_of(first);
            let at = head.offset.min(atoms.len());
            let count = inserted.len();
            atoms.splice(at..at, inserted);
            // A space typed after the paste must survive the Markdown trimming.
            if source.ends_with(' ') {
                atoms.insert(
                    at + count,
                    Atom {
                        ch: ' ',
                        marks: Vec::new(),
                        hard_break: false,
                        node: None,
                    },
                );
            }
            self.set_atoms(head.block, &atoms);
            self.selection = Selection::caret(Pos {
                block: head.block,
                offset: at + count + usize::from(source.ends_with(' ')),
            });
        } else {
            self.insert_blocks(blocks);
        }
        self.stored_marks = None;
        self.typing = None;
    }

    fn insert_blocks(&mut self, mut blocks: Vec<Node>) {
        let head = self.selection.head;
        let path = self.path_of(head.block);
        let len = self.block_len(head.block);
        // A list pasted into a list of the same kind adds items to it.
        let item = path.len() >= 3 && is_item(&node_at(&self.doc, &path[..path.len() - 1]).kind);
        let into_list = item
            && blocks.len() == 1
            && node_at(&self.doc, &path[..path.len() - 2]).kind == blocks[0].kind;
        let (parent, mut index, replace) = if into_list || path.len() == 1 {
            let depth = if into_list { path.len() - 2 } else { 0 };
            let parent = path[..depth].to_vec();
            let index = path[depth];
            if into_list {
                blocks = blocks.remove(0).content;
            }
            let replace =
                len == 0 && (into_list || node_at(&self.doc, &path).kind == Kind::Paragraph);
            let index = if replace || head.offset == 0 {
                index
            } else {
                if head.offset < len {
                    self.split_inner();
                }
                index + 1
            };
            (parent, index, replace)
        } else {
            // Inside a quote or another nested block: after the whole top-level block.
            (Vec::new(), path[0] + 1, false)
        };
        let count = blocks.len();
        let inside: usize = blocks.iter().map(text_blocks).sum();
        let siblings = &mut self.doc_mut_at(&parent).content;
        index = index.min(siblings.len());
        if replace {
            siblings.remove(index);
        }
        siblings.splice(index..index, blocks);
        let mut first = parent.clone();
        first.push(index);
        // First text block at or after the inserted nodes.
        let start = self
            .textblocks()
            .iter()
            .position(|p| p[..] >= first[..])
            .unwrap_or(self.block_count() - 1);
        let block = (start + inside.max(1) - 1).min(self.block_count() - 1);
        let offset = if inside == 0 || count == 0 {
            0
        } else {
            self.block_len(block)
        };
        self.selection = Selection::caret(Pos { block, offset });
    }
}
