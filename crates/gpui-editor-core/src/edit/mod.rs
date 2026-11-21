//! Headless editing engine: selection, typing, deletion, blocks, marks, lists, undo/redo.
//!
//! A position is a (text block, character offset) pair. Text blocks are numbered in document
//! order; blocks without text (rules, unknown nodes) are kept but cannot hold the cursor.

mod atoms;
mod history;
mod input_rules;
mod lists;
mod marks;
mod navigation;
mod tree;
mod typing;

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};

use crate::{Kind, Mark, Node};

use atoms::{Atom, atoms_of, flat_text, rebuild};
use tree::{Path, blocks_in_order, node_at, node_at_mut};

/// Cursor position: index of the text block and character offset within that block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    pub block: usize,
    pub offset: usize,
}

/// Selection: `anchor` is fixed, `head` moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    #[must_use]
    pub fn caret(pos: Pos) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    #[must_use]
    pub fn start(&self) -> Pos {
        self.anchor.min(self.head)
    }

    #[must_use]
    pub fn end(&self) -> Pos {
        self.anchor.max(self.head)
    }
}

/// A past state of the document. Top-level blocks are shared with the other snapshots and with
/// [`Editor::shared`] as long as they do not change.
#[derive(Debug, Clone)]
struct Snapshot {
    /// The document node without its children.
    root: Node,
    children: Vec<Arc<Node>>,
    selection: Selection,
}

/// Block type that can be inserted (`/` menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockCommand {
    Paragraph,
    Heading(u8),
    BulletList,
    OrderedList,
    TaskList,
    Quote,
    CodeBlock,
    Rule,
}

/// Document being edited, with its selection and history.
#[derive(Debug, Clone)]
pub struct Editor {
    doc: Node,
    /// Paths of the text blocks, in document order; reset by every mutation of `doc`.
    index: OnceLock<Vec<Path>>,
    /// Shared copies of the top-level blocks of `doc`, taken by the last snapshot. When `aligned`,
    /// `shared[i]` is `Some` only while `doc.content[i]` is unchanged; otherwise the entries are
    /// matched again against `doc.content` at the next snapshot.
    shared: Vec<Option<Arc<Node>>>,
    aligned: bool,
    selection: Selection,
    stored_marks: Option<Vec<Mark>>,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    typing: Option<Pos>,
}

const HISTORY_LIMIT: usize = 500;

impl Editor {
    /// Opens a document; a document without any text block gets an empty paragraph.
    #[must_use]
    pub fn new(mut doc: Node) -> Self {
        if !blocks_in_order(&doc).iter().any(|(_, text)| *text) {
            doc.content.push(Node::new(Kind::Paragraph));
        }
        Self {
            doc,
            index: OnceLock::new(),
            shared: Vec::new(),
            aligned: false,
            selection: Selection::default(),
            stored_marks: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
            typing: None,
        }
    }

    #[must_use]
    pub fn doc(&self) -> &Node {
        &self.doc
    }

    #[must_use]
    pub fn into_doc(self) -> Node {
        self.doc
    }

    #[must_use]
    pub fn selection(&self) -> Selection {
        self.selection
    }

    fn textblocks(&self) -> &[Path] {
        let index = self.index.get_or_init(|| {
            blocks_in_order(&self.doc)
                .into_iter()
                .filter(|(_, text)| *text)
                .map(|(path, _)| path)
                .collect()
        });
        #[cfg(debug_assertions)]
        {
            let fresh: Vec<Path> = blocks_in_order(&self.doc)
                .into_iter()
                .filter(|(_, text)| *text)
                .map(|(path, _)| path)
                .collect();
            debug_assert_eq!(*index, fresh, "stale text block index");
        }
        index
    }

    /// Mutable access to the whole document; invalidates the text block index and the alignment
    /// of the shared blocks.
    fn doc_mut(&mut self) -> &mut Node {
        self.index.take();
        self.aligned = false;
        &mut self.doc
    }

    /// Mutable access to the node at `path`: only its top-level block stops being shared.
    fn node_mut(&mut self, path: &[usize]) -> &mut Node {
        self.index.take();
        match path.first() {
            Some(&top) if self.aligned => {
                if let Some(entry) = self.shared.get_mut(top) {
                    *entry = None;
                }
            }
            Some(_) => {}
            None => self.aligned = false,
        }
        node_at_mut(&mut self.doc, path)
    }

    #[must_use]
    pub fn block_count(&self) -> usize {
        self.textblocks().len()
    }

    fn path_of(&self, block: usize) -> Path {
        let blocks = self.textblocks();
        blocks[block.min(blocks.len() - 1)].clone()
    }

    fn atoms(&self, block: usize) -> Vec<Atom> {
        atoms_of(node_at(&self.doc, &self.path_of(block)))
    }

    fn set_atoms(&mut self, block: usize, atoms: &[Atom]) {
        let path = self.path_of(block);
        rebuild(self.node_mut(&path), atoms);
    }

    /// Text of the block (a line break for each hard break).
    #[must_use]
    pub fn block_text(&self, block: usize) -> String {
        flat_text(&self.atoms(block))
    }

    #[must_use]
    pub fn block_len(&self, block: usize) -> usize {
        self.atoms(block).len()
    }

    #[must_use]
    pub fn block_kind(&self, block: usize) -> Kind {
        node_at(&self.doc, &self.path_of(block)).kind.clone()
    }

    /// Node of the text block (for rendering).
    #[must_use]
    pub fn block_node(&self, block: usize) -> &Node {
        node_at(&self.doc, &self.path_of(block))
    }

    fn clamp(&self, pos: Pos) -> Pos {
        let count = self.block_count();
        let block = pos.block.min(count - 1);
        Pos {
            block,
            offset: pos.offset.min(self.block_len(block)),
        }
    }

    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = Selection {
            anchor: self.clamp(selection.anchor),
            head: self.clamp(selection.head),
        };
        self.stored_marks = None;
        self.typing = None;
    }
}
