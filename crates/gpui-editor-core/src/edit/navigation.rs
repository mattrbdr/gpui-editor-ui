//! Reading the selection and moving the caret.

use crate::{Mark, MarkKind};

use super::{
    Editor, Pos, Selection,
    atoms::{Atom, MENTION, flat_text, next_bound, prev_bound, word_spans},
};

impl Editor {
    // --- Reading the selection ---

    /// Selected text; blocks are separated by a line break.
    #[must_use]
    pub fn selected_text(&self) -> String {
        let (start, end) = (self.selection.start(), self.selection.end());
        let mut parts = Vec::new();
        for block in start.block..=end.block {
            let atoms = self.atoms(block);
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
            parts.push(flat_text(
                &atoms[from.min(atoms.len())..to.min(atoms.len())],
            ));
        }
        parts.join("\n")
    }

    /// Whether the mark is active over the whole selection (or at the caret).
    #[must_use]
    pub fn is_mark_active(&self, kind: &MarkKind) -> bool {
        if self.selection.is_collapsed() {
            return self.marks_at_caret().iter().any(|mark| mark.kind == *kind);
        }
        self.selected_atoms()
            .iter()
            .all(|atom| atom.marks.iter().any(|mark| mark.kind == *kind))
    }

    pub(super) fn selected_atoms(&self) -> Vec<Atom> {
        let (start, end) = (self.selection.start(), self.selection.end());
        let mut out = Vec::new();
        for block in start.block..=end.block {
            let atoms = self.atoms(block);
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
            out.extend(
                atoms[from.min(atoms.len())..to.min(atoms.len())]
                    .iter()
                    .filter(|a| !a.hard_break)
                    .cloned(),
            );
        }
        out
    }

    pub(super) fn marks_at_caret(&self) -> Vec<Mark> {
        if let Some(stored) = &self.stored_marks {
            return stored.clone();
        }
        let pos = self.selection.head;
        let atoms = self.atoms(pos.block);
        let Some(before) = pos.offset.checked_sub(1).and_then(|i| atoms.get(i)) else {
            return atoms.first().map(|a| a.marks.clone()).unwrap_or_default();
        };
        let after = atoms.get(pos.offset);
        before
            .marks
            .iter()
            .filter(|mark| {
                mark.kind != MarkKind::Other(MENTION.to_owned())
                    && (mark.kind != MarkKind::Link
                        || after.is_some_and(|a| a.marks.contains(mark)))
            })
            .cloned()
            .collect()
    }

    // --- Movement ---

    pub(super) fn go(&mut self, pos: Pos, extend: bool) {
        self.selection.head = pos;
        if !extend {
            self.selection.anchor = pos;
        }
        self.stored_marks = None;
        self.typing = None;
    }

    /// Moves the caret (or extends the selection) to `pos`.
    pub fn move_to(&mut self, pos: Pos, extend: bool) {
        let pos = self.clamp(pos);
        self.go(pos, extend);
    }

    pub fn move_left(&mut self, extend: bool) {
        if !extend && !self.selection.is_collapsed() {
            let start = self.selection.start();
            self.go(start, false);
            return;
        }
        let head = self.selection.head;
        let target = if head.offset > 0 {
            Pos {
                block: head.block,
                offset: prev_bound(&self.block_text(head.block), head.offset),
            }
        } else if head.block > 0 {
            Pos {
                block: head.block - 1,
                offset: self.block_len(head.block - 1),
            }
        } else {
            head
        };
        self.go(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend && !self.selection.is_collapsed() {
            let end = self.selection.end();
            self.go(end, false);
            return;
        }
        let head = self.selection.head;
        let len = self.block_len(head.block);
        let target = if head.offset < len {
            Pos {
                block: head.block,
                offset: next_bound(&self.block_text(head.block), head.offset),
            }
        } else if head.block + 1 < self.block_count() {
            Pos {
                block: head.block + 1,
                offset: 0,
            }
        } else {
            head
        };
        self.go(target, extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        let head = self.selection.head;
        let spans = word_spans(&self.block_text(head.block));
        let target = match spans.iter().rev().find(|(start, _)| *start < head.offset) {
            Some((start, _)) => Pos {
                block: head.block,
                offset: *start,
            },
            None if head.block > 0 && head.offset == 0 => Pos {
                block: head.block - 1,
                offset: self.block_len(head.block - 1),
            },
            None => Pos {
                block: head.block,
                offset: 0,
            },
        };
        self.go(target, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let head = self.selection.head;
        let len = self.block_len(head.block);
        let spans = word_spans(&self.block_text(head.block));
        let target = match spans.iter().find(|(_, end)| *end > head.offset) {
            Some((_, end)) => Pos {
                block: head.block,
                offset: *end,
            },
            None if head.block + 1 < self.block_count() && head.offset == len => Pos {
                block: head.block + 1,
                offset: 0,
            },
            None => Pos {
                block: head.block,
                offset: len,
            },
        };
        self.go(target, extend);
    }

    pub fn move_to_block_start(&mut self, extend: bool) {
        let block = self.selection.head.block;
        self.go(Pos { block, offset: 0 }, extend);
    }

    pub fn move_to_block_end(&mut self, extend: bool) {
        let block = self.selection.head.block;
        let offset = self.block_len(block);
        self.go(Pos { block, offset }, extend);
    }

    pub fn move_to_doc_start(&mut self, extend: bool) {
        self.go(
            Pos {
                block: 0,
                offset: 0,
            },
            extend,
        );
    }

    pub fn move_to_doc_end(&mut self, extend: bool) {
        let block = self.block_count() - 1;
        let offset = self.block_len(block);
        self.go(Pos { block, offset }, extend);
    }

    pub fn select_all(&mut self) {
        let last = self.block_count() - 1;
        self.selection = Selection {
            anchor: Pos {
                block: 0,
                offset: 0,
            },
            head: Pos {
                block: last,
                offset: self.block_len(last),
            },
        };
        self.stored_marks = None;
        self.typing = None;
    }
}
