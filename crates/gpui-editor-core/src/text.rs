//! Plain text of a document: text leaves, with a line break after each block.

use crate::model::{Kind, Node};

/// `String.prototype.trim`: Unicode whitespace and BOM, but not U+0085.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// Collapses any run of three or more line breaks into two.
fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        if c == '\n' {
            run += 1;
            if run <= 2 {
                out.push(c);
            }
        } else {
            run = 0;
            out.push(c);
        }
    }
    out
}

fn ends_with_newline(kind: &Kind, extra: &[&str]) -> bool {
    matches!(
        kind,
        Kind::Paragraph
            | Kind::Heading
            | Kind::Blockquote
            | Kind::CodeBlock
            | Kind::ListItem
            | Kind::TaskItem
    ) || extra.contains(&kind.name())
}

impl Node {
    /// Plain text: the text of the leaves, a line break after each block, blank lines collapsed.
    ///
    /// ```
    /// use gpui_editor_core::Node;
    ///
    /// let doc = Node::from_markdown("# Title\n\nSome **bold** text.\n\nThe end.");
    /// assert_eq!(doc.plain_text(), "Title\nSome bold text.\nThe end.");
    /// ```
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.plain_text_with(&|_| None, &[])
    }

    /// Like [`Node::plain_text`], with application-specific rendering for some nodes.
    ///
    /// `leaf` returns the text of a custom node (mention, view…); `extra_blocks` lists the names of
    /// custom nodes that also end with a line break.
    #[must_use]
    pub fn plain_text_with(
        &self,
        leaf: &dyn Fn(&Node) -> Option<String>,
        extra_blocks: &[&str],
    ) -> String {
        fn visit(
            node: &Node,
            leaf: &dyn Fn(&Node) -> Option<String>,
            extra: &[&str],
            out: &mut String,
        ) {
            match node.text.as_deref().filter(|text| !text.is_empty()) {
                Some(text) => out.push_str(text),
                None => {
                    if let Some(custom) = leaf(node) {
                        out.push_str(&custom);
                    }
                }
            }
            for child in &node.content {
                visit(child, leaf, extra, out);
            }
            if ends_with_newline(&node.kind, extra) {
                out.push('\n');
            }
        }
        let mut out = String::new();
        visit(self, leaf, extra_blocks, &mut out);
        js_trim(&collapse_blank_lines(&out)).to_owned()
    }
}
