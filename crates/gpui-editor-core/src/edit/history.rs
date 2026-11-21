//! Undo and redo.

use super::{Editor, HISTORY_LIMIT, Snapshot};

impl Editor {
    // --- Historique ---

    pub(super) fn checkpoint(&mut self) {
        self.undo.push_back(Snapshot {
            doc: self.doc.clone(),
            selection: self.selection,
        });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.typing = None;
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo.pop_back() {
            self.redo.push(Snapshot {
                doc: self.doc.clone(),
                selection: self.selection,
            });
            self.set_doc(snapshot.doc);
            self.selection = snapshot.selection;
            self.stored_marks = None;
            self.typing = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push_back(Snapshot {
                doc: self.doc.clone(),
                selection: self.selection,
            });
            self.set_doc(snapshot.doc);
            self.selection = snapshot.selection;
            self.stored_marks = None;
            self.typing = None;
        }
    }

    pub(super) fn push_snapshot(&mut self) {
        self.undo.push_back(Snapshot {
            doc: self.doc.clone(),
            selection: self.selection,
        });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
    }
}
