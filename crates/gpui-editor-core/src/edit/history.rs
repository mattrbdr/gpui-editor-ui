//! Undo and redo.
//!
//! A snapshot keeps the top-level blocks behind [`Arc`]s: a block that did not change since the
//! previous snapshot is shared instead of copied, so recording a step costs about the size of
//! the blocks it touched rather than the size of the document.

use std::collections::HashMap;
use std::sync::Arc;

use crate::Node;

use super::{Editor, HISTORY_LIMIT, Snapshot};

impl Editor {
    // --- History ---

    /// Matches the shared blocks of the last snapshot against the current top-level blocks after a
    /// structural change: blocks equal at the start and at the end of the document are kept.
    fn realign(&mut self) {
        let previous = std::mem::take(&mut self.shared);
        let current = &self.doc.content;
        let same = |shared: &Option<Arc<Node>>, node: &Node| {
            shared.as_deref().is_some_and(|shared| shared == node)
        };
        let prefix = previous
            .iter()
            .zip(current)
            .take_while(|(shared, node)| same(shared, node))
            .count();
        let room = previous.len().min(current.len()) - prefix;
        let suffix = previous
            .iter()
            .rev()
            .zip(current.iter().rev())
            .take(room)
            .take_while(|(shared, node)| same(shared, node))
            .count();
        let mut shared = vec![None; current.len()];
        shared[..prefix].clone_from_slice(&previous[..prefix]);
        shared[current.len() - suffix..].clone_from_slice(&previous[previous.len() - suffix..]);
        self.shared = shared;
        self.aligned = true;
    }

    fn snapshot(&mut self) -> Snapshot {
        if !self.aligned || self.shared.len() != self.doc.content.len() {
            self.realign();
        }
        #[cfg(debug_assertions)]
        for (shared, node) in self.shared.iter().zip(&self.doc.content) {
            if let Some(shared) = shared {
                debug_assert!(**shared == *node, "stale shared block");
            }
        }
        let children = self
            .shared
            .iter_mut()
            .zip(&self.doc.content)
            .map(|(shared, node)| Arc::clone(shared.get_or_insert_with(|| Arc::new(node.clone()))))
            .collect();
        let content = std::mem::take(&mut self.doc.content);
        let root = self.doc.clone();
        self.doc.content = content;
        Snapshot {
            root,
            children,
            selection: self.selection,
        }
    }

    /// Restores `snapshot`. Blocks it shares with the current document (same `Arc` as in
    /// `shared`, right after [`Editor::snapshot`]) are moved instead of copied.
    fn restore(&mut self, snapshot: Snapshot) {
        let mut current: HashMap<*const Node, Node> = self
            .shared
            .iter()
            .zip(std::mem::take(&mut self.doc.content))
            .filter_map(|(shared, node)| Some((Arc::as_ptr(shared.as_ref()?), node)))
            .collect();
        let mut doc = snapshot.root;
        doc.content = snapshot
            .children
            .iter()
            .map(|child| {
                current
                    .remove(&Arc::as_ptr(child))
                    .unwrap_or_else(|| (**child).clone())
            })
            .collect();
        self.doc = doc;
        self.shared = snapshot.children.into_iter().map(Some).collect();
        self.aligned = true;
        self.index.take();
        self.selection = snapshot.selection;
        self.stored_marks = None;
        self.typing = None;
    }

    pub(super) fn checkpoint(&mut self) {
        self.push_snapshot();
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
            let current = self.snapshot();
            self.redo.push(current);
            self.restore(snapshot);
        }
    }

    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo.pop() {
            let current = self.snapshot();
            self.undo.push_back(current);
            self.restore(snapshot);
        }
    }

    pub(super) fn push_snapshot(&mut self) {
        let snapshot = self.snapshot();
        self.undo.push_back(snapshot);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
    }
}
