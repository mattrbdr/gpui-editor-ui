//! Find and replace inside the text blocks.

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::{Editor, Pos, Selection, atoms::mention_mark};

/// Occurrence of a searched text, always within one text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// First position of the occurrence.
    pub start: Pos,
    /// Position just after the occurrence.
    pub end: Pos,
    /// Whether [`Editor::replace_matches`] may replace it: not when it touches a mention or an
    /// inline node the editor does not know.
    pub replaceable: bool,
}

/// Comparison form of a character: lower case, without accents. Empty for a combining mark.
fn fold(ch: char) -> impl Iterator<Item = char> {
    std::iter::once(ch)
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
}

impl Editor {
    // --- Search ---

    /// Occurrences of `query` in document order, ignoring case and accents. They do not overlap
    /// and never span two blocks.
    #[must_use]
    pub fn find(&self, query: &str) -> Vec<Match> {
        let needle: Vec<char> = query.chars().flat_map(fold).collect();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut found = Vec::new();
        for block in 0..self.block_count() {
            let atoms = self.atoms(block);
            // Folded characters, each with the offset of the character it comes from.
            let haystack: Vec<(char, usize)> = atoms
                .iter()
                .enumerate()
                .flat_map(|(offset, atom)| fold(atom.ch).map(move |c| (c, offset)))
                .collect();
            let mut at = 0;
            while at + needle.len() <= haystack.len() {
                let window = &haystack[at..at + needle.len()];
                if !window.iter().map(|(c, _)| *c).eq(needle.iter().copied()) {
                    at += 1;
                    continue;
                }
                let start = window[0].1;
                // Up to the next folded character, so that combining accents are included.
                let end = haystack
                    .get(at + needle.len())
                    .map_or(atoms.len(), |(_, offset)| *offset)
                    .max(window[needle.len() - 1].1 + 1);
                let replaceable = atoms[start..end]
                    .iter()
                    .all(|atom| atom.node.is_none() && mention_mark(atom).is_none());
                found.push(Match {
                    start: Pos {
                        block,
                        offset: start,
                    },
                    end: Pos { block, offset: end },
                    replaceable,
                });
                at += needle.len();
            }
        }
        found
    }

    /// Replaces the replaceable occurrences (as returned by [`Editor::find`] on the current
    /// document) with `text`, as a single history step. Each replacement takes the formatting of
    /// the first character it replaces; line breaks in `text` become spaces. The caret is placed
    /// after the first replacement. Returns the number of replacements.
    pub fn replace_matches(&mut self, matches: &[Match], text: &str) -> usize {
        let mut targets: Vec<Match> = matches.iter().copied().filter(|m| m.replaceable).collect();
        if targets.is_empty() {
            return 0;
        }
        targets.sort_by_key(|m| m.start);
        let text = text.replace(['\r', '\n'], " ");
        self.begin_batch();
        // From the last to the first: the earlier positions stay valid.
        for target in targets.iter().rev() {
            let marks = self
                .atoms(target.start.block)
                .get(target.start.offset)
                .map(|atom| atom.marks.clone())
                .unwrap_or_default();
            self.selection = Selection {
                anchor: self.clamp(target.start),
                head: self.clamp(target.end),
            };
            self.delete_selection_inner();
            self.insert_in_block(&text, &marks);
        }
        self.stored_marks = None;
        self.typing = None;
        targets.len()
    }
}
