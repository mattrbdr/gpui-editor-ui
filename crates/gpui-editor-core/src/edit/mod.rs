//! Headless editing engine: selection, typing, deletion, blocks, marks, lists, undo/redo.
//!
//! A position is a (text block, character offset) pair. Text blocks are numbered in document
//! order; blocks without text (rules, unknown nodes) are kept but cannot hold the cursor.

mod atoms;
mod clipboard;
mod history;
mod input_rules;
mod lists;
mod marks;
mod navigation;
mod search;
mod tree;
mod typing;

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};

use crate::{Kind, Mark, Node};

pub use clipboard::looks_like_markdown;
pub use search::Match;

use atoms::{Atom, atoms_of, flat_text, rebuild};
use tree::{Path, blocks_in_order, node_at, node_at_mut};

/// Cursor position: index of the text block and character offset within that block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    /// Index of the text block, in document order (see [`Editor::block_count`]).
    pub block: usize,
    /// Offset in characters (Unicode scalar values) from the start of the block.
    pub offset: usize,
}

/// Selection: `anchor` is fixed, `head` moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// End that stays in place when the selection is extended.
    pub anchor: Pos,
    /// End that moves, where the caret is shown.
    pub head: Pos,
}

impl Selection {
    /// Empty selection (a caret) at `pos`.
    #[must_use]
    pub fn caret(pos: Pos) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    /// Whether the selection is empty (`anchor == head`).
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// First end of the selection in document order.
    #[must_use]
    pub fn start(&self) -> Pos {
        self.anchor.min(self.head)
    }

    /// Last end of the selection in document order.
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
#[non_exhaustive]
pub enum BlockCommand {
    /// Turns the block into a `paragraph`.
    Paragraph,
    /// Turns the block into a `heading` of the given level.
    Heading(u8),
    /// Wraps the block in a `bulletList`.
    BulletList,
    /// Wraps the block in an `orderedList`.
    OrderedList,
    /// Wraps the block in a `taskList`.
    TaskList,
    /// Wraps the block in a `blockquote`.
    Quote,
    /// Turns the block into a `codeBlock` (with `language: null`).
    CodeBlock,
    /// Inserts a `horizontalRule` after the block (in its place if it is empty), then a paragraph.
    Rule,
}

/// Document being edited, with its selection and history.
///
/// ```
/// use gpui_editor_core::{Editor, MarkKind, Node, Pos, Selection};
///
/// let mut editor = Editor::new(Node::from_markdown("# Notes\n\nHello world"));
/// assert_eq!(editor.block_count(), 2);
///
/// editor.set_selection(Selection::caret(Pos { block: 1, offset: 5 }));
/// editor.insert_text(",");
/// assert_eq!(editor.block_text(1), "Hello, world");
///
/// // Select "world" and make it bold.
/// editor.set_selection(Selection {
///     anchor: Pos { block: 1, offset: 7 },
///     head: Pos { block: 1, offset: 12 },
/// });
/// editor.toggle_mark(MarkKind::Bold);
/// assert_eq!(editor.doc().to_markdown(), "# Notes\n\nHello, **world**");
///
/// editor.undo();
/// editor.undo();
/// assert_eq!(editor.block_text(1), "Hello world");
/// ```
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
    /// Number of changes made to `doc`.
    revision: u64,
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
            revision: 0,
        }
    }

    /// The document as currently edited.
    #[must_use]
    pub fn doc(&self) -> &Node {
        &self.doc
    }

    /// Consumes the editor and returns its document.
    #[must_use]
    pub fn into_doc(self) -> Node {
        self.doc
    }

    /// Current selection.
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
        self.revision += 1;
        self.index.take();
        self.aligned = false;
        &mut self.doc
    }

    /// Mutable access to the node at `path`: only its top-level block stops being shared.
    fn node_mut(&mut self, path: &[usize]) -> &mut Node {
        self.index.take();
        self.unshared_mut(path)
    }

    /// Like [`Editor::node_mut`] for a change that keeps every text block where it is (the inline
    /// content of a text block): the text block index stays valid.
    fn unshared_mut(&mut self, path: &[usize]) -> &mut Node {
        self.revision += 1;
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

    /// Mutable access to the node at `path` for a change of its children.
    fn doc_mut_at(&mut self, path: &[usize]) -> &mut Node {
        if path.is_empty() {
            self.doc_mut()
        } else {
            self.node_mut(path)
        }
    }

    /// Changes whenever the document does; the selection does not count.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Number of text blocks (paragraphs, headings, code blocks); always at least 1.
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
        // Only the inline content changes: the text blocks keep their paths.
        let path = self.path_of(block);
        rebuild(self.unshared_mut(&path), atoms);
    }

    /// Text of the block (a line break for each hard break).
    #[must_use]
    pub fn block_text(&self, block: usize) -> String {
        flat_text(&self.atoms(block))
    }

    /// Length of the block in characters (a hard break or an inline atom counts as one).
    #[must_use]
    pub fn block_len(&self, block: usize) -> usize {
        self.atoms(block).len()
    }

    /// Type of the text block.
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

    /// Sets the selection, clamped to the document, and forgets the marks stored at the caret.
    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = Selection {
            anchor: self.clamp(selection.anchor),
            head: self.clamp(selection.head),
        };
        self.stored_marks = None;
        self.typing = None;
    }
}
