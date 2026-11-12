//! Texte brut d'un document : feuilles de texte, saut de ligne après chaque bloc.

use crate::model::{Kind, Node};

/// `String.prototype.trim` : espaces Unicode et BOM, mais pas U+0085.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// Remplace toute suite de trois sauts de ligne ou plus par deux.
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
    /// Texte brut : le texte des feuilles, un saut de ligne après chaque bloc, lignes vides réduites.
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.plain_text_with(&|_| None, &[])
    }

    /// Comme [`Node::plain_text`], avec un rendu propre à l'application pour certains nœuds.
    ///
    /// `leaf` renvoie le texte d'un nœud personnalisé (mention, vue…) ; `extra_blocks` liste les noms
    /// de nœuds personnalisés qui se terminent eux aussi par un saut de ligne.
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
