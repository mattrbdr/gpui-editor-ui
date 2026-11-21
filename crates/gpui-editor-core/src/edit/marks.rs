//! Block types, marks and links.

use serde_json::{Map, Value};

use crate::{Kind, Mark, MarkKind};

use super::{Editor, Pos, Selection, atoms::rebuild, tree::node_at_mut};

impl Editor {
    // --- Blocs et marques ---

    /// Change le type des blocs de la sélection.
    pub(super) fn convert_block(&mut self, block: usize, kind: Kind, attrs: Map<String, Value>) {
        let mut atoms = self.atoms(block);
        let was_code = self.block_kind(block) == Kind::CodeBlock;
        if kind == Kind::CodeBlock {
            for atom in &mut atoms {
                atom.marks.clear();
                atom.hard_break = false;
            }
        } else if was_code {
            for atom in &mut atoms {
                atom.hard_break = atom.ch == '\n';
            }
        }
        let path = self.path_of(block);
        let node = node_at_mut(self.doc_mut(), &path);
        node.kind = kind;
        node.attrs = attrs;
        node.shape = crate::model::Shape::default();
        rebuild(node, &atoms);
        let len = atoms.len();
        for pos in [&mut self.selection.anchor, &mut self.selection.head] {
            if pos.block == block {
                pos.offset = pos.offset.min(len);
            }
        }
    }

    pub(super) fn selected_blocks(&self) -> std::ops::RangeInclusive<usize> {
        self.selection.start().block..=self.selection.end().block
    }

    pub(super) fn attrs_of(level: Option<u64>) -> Map<String, Value> {
        let mut attrs = Map::new();
        if let Some(level) = level {
            attrs.insert("level".into(), Value::from(level));
        }
        attrs
    }

    /// Titre de niveau `level`, ou paragraphe si les blocs en sont déjà.
    pub fn toggle_heading(&mut self, level: u64) {
        self.checkpoint();
        let all = self.selected_blocks().all(|b| {
            let node = self.block_node(b);
            node.kind == Kind::Heading
                && node.attrs.get("level").and_then(Value::as_u64) == Some(level)
        });
        for block in self.selected_blocks() {
            if all {
                self.convert_block(block, Kind::Paragraph, Map::new());
            } else {
                self.convert_block(block, Kind::Heading, Self::attrs_of(Some(level)));
            }
        }
    }

    pub fn toggle_code_block(&mut self) {
        self.checkpoint();
        let all = self
            .selected_blocks()
            .all(|b| self.block_kind(b) == Kind::CodeBlock);
        for block in self.selected_blocks() {
            if all {
                self.convert_block(block, Kind::Paragraph, Map::new());
            } else {
                let mut attrs = Map::new();
                attrs.insert("language".into(), Value::Null);
                self.convert_block(block, Kind::CodeBlock, attrs);
            }
        }
    }

    pub fn toggle_mark(&mut self, kind: MarkKind) {
        self.toggle_mark_with(&Mark::new(kind));
    }

    pub(super) fn toggle_mark_with(&mut self, mark: &Mark) {
        if self.selection.is_collapsed() {
            let mut marks = self.marks_at_caret();
            if marks.iter().any(|m| m.kind == mark.kind) {
                marks.retain(|m| m.kind != mark.kind);
            } else {
                marks.push(mark.clone());
            }
            self.stored_marks = Some(marks);
            self.typing = None;
            return;
        }
        let active = self.is_mark_active(&mark.kind);
        self.edit_marks(|marks| {
            marks.retain(|m| m.kind != mark.kind);
            if !active {
                marks.push(mark.clone());
            }
        });
    }

    /// Lien touché par `pos` : début, fin et adresse. Le lien s'étend sur tous les caractères voisins
    /// qui portent la même adresse.
    #[must_use]
    pub fn link_at(&self, pos: Pos) -> Option<(Pos, Pos, String)> {
        let pos = self.clamp(pos);
        let atoms = self.atoms(pos.block);
        let href_of = |index: usize| -> Option<String> {
            atoms
                .get(index)?
                .marks
                .iter()
                .find(|mark| mark.kind == MarkKind::Link)
                .and_then(|mark| mark.attrs.get("href"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let anchor = [Some(pos.offset), pos.offset.checked_sub(1)]
            .into_iter()
            .flatten()
            .find(|index| href_of(*index).is_some())?;
        let href = href_of(anchor)?;
        let mut start = anchor;
        while start > 0 && href_of(start - 1).as_deref() == Some(href.as_str()) {
            start -= 1;
        }
        let mut end = anchor + 1;
        while href_of(end).as_deref() == Some(href.as_str()) {
            end += 1;
        }
        Some((
            Pos {
                block: pos.block,
                offset: start,
            },
            Pos {
                block: pos.block,
                offset: end,
            },
            href,
        ))
    }

    /// Sélectionne le lien touché par le curseur et retourne son adresse.
    pub fn select_link_at_caret(&mut self) -> Option<String> {
        let (start, end, href) = self.link_at(self.selection.head)?;
        self.selection = Selection {
            anchor: start,
            head: end,
        };
        Some(href)
    }

    /// Pose (`Some`) ou retire (`None`) un lien sur la sélection.
    pub fn set_link(&mut self, href: Option<&str>) {
        let href = href.map(str::to_owned);
        self.edit_marks(move |marks| {
            marks.retain(|m| m.kind != MarkKind::Link);
            if let Some(href) = &href {
                let mut link = Mark::new(MarkKind::Link);
                link.attrs
                    .insert("href".into(), Value::String(href.clone()));
                link.attrs.insert("title".into(), Value::Null);
                marks.push(link);
            }
        });
    }

    pub(super) fn edit_marks(&mut self, change: impl Fn(&mut Vec<Mark>)) {
        if self.selection.is_collapsed() {
            return;
        }
        self.checkpoint();
        let (start, end) = (self.selection.start(), self.selection.end());
        for block in start.block..=end.block {
            if self.block_kind(block) == Kind::CodeBlock {
                continue;
            }
            let mut atoms = self.atoms(block);
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
            for atom in atoms
                .iter_mut()
                .take(to)
                .skip(from)
                .filter(|a| !a.hard_break)
            {
                change(&mut atom.marks);
            }
            self.set_atoms(block, &atoms);
        }
    }
}
