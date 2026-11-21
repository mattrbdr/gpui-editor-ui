//! Lists.

use serde_json::{Map, Value};

use crate::{Kind, Node};

use super::{
    Editor, Pos, Selection,
    tree::{Path, is_item, node_at, node_at_mut},
};

impl Editor {
    // --- Lists ---

    pub(super) fn list_context(&self, block: usize) -> Option<(Path, usize)> {
        let path = self.path_of(block);
        let parent = &path[..path.len() - 1];
        if !is_item(&node_at(&self.doc, parent).kind) {
            return None;
        }
        let list = parent[..parent.len() - 1].to_vec();
        Some((list, parent[parent.len() - 1]))
    }

    /// Wraps the current block in a list of the given type, changes its list type, or lifts it out.
    pub fn toggle_list(&mut self, kind: Kind) {
        self.checkpoint();
        self.toggle_list_raw(kind);
    }

    pub(super) fn toggle_list_raw(&mut self, kind: Kind) {
        let block = self.selection.head.block;
        let path = self.path_of(block);
        if let Some((list_path, _)) = self.list_context(block) {
            let list_kind = node_at(&self.doc, &list_path).kind.clone();
            if list_kind == kind {
                let list = node_at(&self.doc, &list_path).clone();
                let blocks: Vec<Node> = list
                    .content
                    .into_iter()
                    .flat_map(|item| item.content)
                    .collect();
                let (index, parent) = list_path
                    .split_last()
                    .map_or((0, Vec::new()), |(i, p)| (*i, p.to_vec()));
                let container = node_at_mut(self.doc_mut(), &parent);
                container.content.splice(index..=index, blocks);
            } else {
                let item_kind = if kind == Kind::TaskList {
                    Kind::TaskItem
                } else {
                    Kind::ListItem
                };
                let list = node_at_mut(self.doc_mut(), &list_path);
                list.kind = kind;
                list.attrs = Map::new();
                for item in &mut list.content {
                    item.kind = item_kind.clone();
                    item.attrs = Map::new();
                    if item_kind == Kind::TaskItem {
                        item.attrs.insert("checked".into(), Value::Bool(false));
                    }
                }
            }
        } else {
            let (index, parent) = path
                .split_last()
                .map_or((0, Vec::new()), |(i, p)| (*i, p.to_vec()));
            let container = node_at_mut(self.doc_mut(), &parent);
            let child = container.content.remove(index);
            let mut item = Node::new(if kind == Kind::TaskList {
                Kind::TaskItem
            } else {
                Kind::ListItem
            });
            if kind == Kind::TaskList {
                item.attrs.insert("checked".into(), Value::Bool(false));
            }
            item.content.push(child);
            let mut list = Node::new(kind);
            list.content.push(item);
            container.content.insert(index, list);
        }
        let len = self.block_len(block.min(self.block_count() - 1));
        self.selection = Selection::caret(Pos {
            block: block.min(self.block_count() - 1),
            offset: self.selection.head.offset.min(len),
        });
    }

    /// Checks or unchecks the task of the current block.
    pub fn toggle_task_checked(&mut self) {
        let block = self.selection.head.block;
        let path = self.path_of(block);
        let item_path = &path[..path.len() - 1];
        if node_at(&self.doc, item_path).kind != Kind::TaskItem {
            return;
        }
        self.checkpoint();
        let item = node_at_mut(self.doc_mut(), item_path);
        let checked = item
            .attrs
            .get("checked")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        item.attrs.insert("checked".into(), Value::Bool(!checked));
    }

    /// Nests the current item inside the previous one.
    pub fn indent(&mut self) {
        let block = self.selection.head.block;
        let Some((list_path, item_index)) = self.list_context(block) else {
            return;
        };
        if item_index == 0 {
            return;
        }
        self.checkpoint();
        let list = node_at_mut(self.doc_mut(), &list_path);
        let item = list.content.remove(item_index);
        let list_kind = list.kind.clone();
        let list_attrs = list.attrs.clone();
        let previous = &mut list.content[item_index - 1];
        if previous
            .content
            .last()
            .is_none_or(|last| last.kind != list_kind)
        {
            let mut sub = Node::new(list_kind);
            sub.attrs = list_attrs;
            previous.content.push(sub);
        }
        if let Some(sub) = previous.content.last_mut() {
            sub.content.push(item);
        }
        self.reselect(block);
    }

    /// Moves the current item out of a nested list, right after its parent.
    pub fn outdent(&mut self) {
        let block = self.selection.head.block;
        let Some((list_path, item_index)) = self.list_context(block) else {
            return;
        };
        let Some(outer_list_path) = list_path
            .split_last()
            .filter(|(_, above)| is_item(&node_at(&self.doc, above).kind))
            .map(|(_, above)| above.to_vec())
        else {
            return;
        };
        self.checkpoint();
        let outer_item_index = outer_list_path[outer_list_path.len() - 1];
        let outer_list_path = outer_list_path[..outer_list_path.len() - 1].to_vec();
        let list = node_at_mut(self.doc_mut(), &list_path);
        let item = list.content.remove(item_index);
        let emptied = list.content.is_empty();
        if emptied {
            let (removed, above) = list_path
                .split_last()
                .map_or((0, Vec::new()), |(i, p)| (*i, p.to_vec()));
            node_at_mut(self.doc_mut(), &above).content.remove(removed);
        }
        node_at_mut(self.doc_mut(), &outer_list_path)
            .content
            .insert(outer_item_index + 1, item);
        self.reselect(block);
    }

    pub(super) fn reselect(&mut self, block: usize) {
        let block = block.min(self.block_count() - 1);
        let offset = self.selection.head.offset.min(self.block_len(block));
        self.selection = Selection::caret(Pos { block, offset });
    }
}
