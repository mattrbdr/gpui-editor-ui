//! Windowed rendering: only the top-level blocks near the visible area are laid out.
//!
//! Laying out every block on every frame made long documents lag (GPUI's layout of the whole tree
//! dominated). The other top-level blocks are drawn as empty boxes of the height they had the last
//! time they were laid out, so the scroll position, the scrollbar and the coordinates of the
//! visible blocks stay exact. Heights are cached by content: a block that changes, or that was
//! never laid out, is laid out again wherever it is.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use gpui::Pixels;
use gpui_editor_core::{Kind, Node};
use serde_json::Value;

/// Heights of the top-level blocks as last laid out, by content key.
#[derive(Debug, Default)]
pub(crate) struct Heights {
    /// Width of the viewport the heights were measured at: another width wraps lines differently.
    width: Option<Pixels>,
    by_key: HashMap<u64, Pixels>,
}

impl Heights {
    /// Forgets every height (theme, fonts or embeds changed).
    pub(crate) fn clear(&mut self) {
        self.by_key.clear();
    }

    /// Forgets every height if the viewport width changed.
    pub(crate) fn set_width(&mut self, width: Pixels) {
        if self.width != Some(width) {
            self.width = Some(width);
            self.by_key.clear();
        }
    }

    pub(crate) fn get(&self, key: u64) -> Option<Pixels> {
        self.by_key.get(&key).copied()
    }

    pub(crate) fn record(&mut self, key: u64, height: Pixels) {
        self.by_key.insert(key, height);
    }

    /// Keeps only the heights of `keys` (the blocks of the current document).
    pub(crate) fn retain(&mut self, keys: &[u64]) {
        if self.by_key.len() > keys.len() * 2 + 64 {
            let live: std::collections::HashSet<u64> = keys.iter().copied().collect();
            self.by_key.retain(|key, _| live.contains(key));
        }
    }
}

/// Key of a block's content: equal keys render to the same height at the same width and theme.
pub(crate) fn node_key(node: &Node) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_node(node, &mut hasher);
    hasher.finish()
}

fn hash_node(node: &Node, hasher: &mut DefaultHasher) {
    node.kind.name().hash(hasher);
    hash_map(&node.attrs, hasher);
    node.text.hash(hasher);
    node.marks.len().hash(hasher);
    for mark in &node.marks {
        mark.kind.name().hash(hasher);
        hash_map(&mark.attrs, hasher);
    }
    node.content.len().hash(hasher);
    for child in &node.content {
        hash_node(child, hasher);
    }
}

fn hash_map(map: &serde_json::Map<String, Value>, hasher: &mut DefaultHasher) {
    map.len().hash(hasher);
    for (key, value) in map {
        key.hash(hasher);
        hash_value(value, hasher);
    }
}

fn hash_value(value: &Value, hasher: &mut DefaultHasher) {
    match value {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(flag) => (1u8, flag).hash(hasher),
        Value::Number(number) => (2u8, number.to_string()).hash(hasher),
        Value::String(text) => (3u8, text).hash(hasher),
        Value::Array(items) => {
            (4u8, items.len()).hash(hasher);
            for item in items {
                hash_value(item, hasher);
            }
        }
        Value::Object(map) => {
            5u8.hash(hasher);
            hash_map(map, hasher);
        }
    }
}

/// Number of text blocks (caret positions) inside `node`, in the editor's numbering: the same
/// walk as the rendering, which lays out each of them once.
pub(crate) fn text_block_count(node: &Node) -> usize {
    if node.kind.is_textblock() {
        return 1;
    }
    match node.kind {
        Kind::Doc
        | Kind::Blockquote
        | Kind::BulletList
        | Kind::OrderedList
        | Kind::TaskList
        | Kind::ListItem
        | Kind::TaskItem => node.content.iter().map(text_block_count).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_the_content() {
        let a = Node::from_markdown("# Title");
        let b = Node::from_markdown("# Title");
        let c = Node::from_markdown("# Title!");
        let d = Node::from_markdown("## Title");
        assert_eq!(node_key(&a.content[0]), node_key(&b.content[0]));
        assert_ne!(node_key(&a.content[0]), node_key(&c.content[0]));
        assert_ne!(node_key(&a.content[0]), node_key(&d.content[0]));
    }

    #[test]
    fn text_blocks_are_counted_like_the_editor_numbers_them() {
        let doc = Node::from_markdown(
            "# a\n\npara\n\n- one\n- two\n  - three\n\n> quote\n> more\n\n```\ncode\n```\n\n---\n\n- [ ] task",
        );
        let editor = gpui_editor_core::Editor::new(doc.clone());
        let counted: usize = doc.content.iter().map(text_block_count).sum();
        assert_eq!(counted, editor.block_count());
    }

    #[test]
    fn heights_are_forgotten_when_the_width_changes() {
        let mut heights = Heights::default();
        heights.set_width(gpui::px(800.0));
        heights.record(1, gpui::px(24.0));
        heights.set_width(gpui::px(800.0));
        assert_eq!(heights.get(1), Some(gpui::px(24.0)));
        heights.set_width(gpui::px(600.0));
        assert_eq!(heights.get(1), None);
    }
}
