//! Paths in the document tree.

use crate::{Kind, Node};

pub(super) type Path = Vec<usize>;

pub(super) fn is_container(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Doc
            | Kind::Blockquote
            | Kind::BulletList
            | Kind::OrderedList
            | Kind::TaskList
            | Kind::ListItem
            | Kind::TaskItem
    )
}

pub(super) fn is_item(kind: &Kind) -> bool {
    matches!(kind, Kind::ListItem | Kind::TaskItem)
}

pub(super) fn node_at<'a>(root: &'a Node, path: &[usize]) -> &'a Node {
    path.iter().fold(root, |node, index| &node.content[*index])
}

pub(super) fn node_at_mut<'a>(root: &'a mut Node, path: &[usize]) -> &'a mut Node {
    let mut node = root;
    for index in path {
        node = &mut node.content[*index];
    }
    node
}

/// Blocks in document order: `(path, is_text_block)`.
pub(super) fn blocks_in_order(root: &Node) -> Vec<(Path, bool)> {
    fn walk(node: &Node, path: &mut Path, out: &mut Vec<(Path, bool)>) {
        for (index, child) in node.content.iter().enumerate() {
            path.push(index);
            if child.kind.is_textblock() {
                out.push((path.clone(), true));
            } else if is_container(&child.kind) {
                walk(child, path, out);
            } else if !matches!(child.kind, Kind::Text | Kind::HardBreak) {
                out.push((path.clone(), false));
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut out);
    out
}

/// Removes a node and any containers left empty.
pub(super) fn remove_at(root: &mut Node, path: &[usize]) {
    let Some((last, parent)) = path.split_last() else {
        return;
    };
    node_at_mut(root, parent).content.remove(*last);
    let mut parent = parent;
    while let Some((index, above)) = parent.split_last() {
        if !node_at(root, parent).content.is_empty() {
            break;
        }
        node_at_mut(root, above).content.remove(*index);
        parent = above;
    }
}
