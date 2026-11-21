//! Moteur d'édition sans interface : sélection, saisie, suppression, blocs, marques, listes,
//! annuler/rétablir.
//!
//! Une position est un couple (bloc de texte, décalage en caractères). Les blocs de texte sont
//! numérotés dans l'ordre du document ; les blocs sans texte (règle, nœuds inconnus) sont conservés
//! mais ne portent pas de curseur.

use std::collections::VecDeque;
use std::sync::OnceLock;

use serde_json::{Map, Value};
use unicode_segmentation::UnicodeSegmentation;

use crate::{Kind, Mark, MarkKind, Node};

/// Position du curseur : numéro du bloc de texte et décalage en caractères dans ce bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    pub block: usize,
    pub offset: usize,
}

/// Sélection : `anchor` est fixe, `head` bouge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    #[must_use]
    pub fn caret(pos: Pos) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    #[must_use]
    pub fn start(&self) -> Pos {
        self.anchor.min(self.head)
    }

    #[must_use]
    pub fn end(&self) -> Pos {
        self.anchor.max(self.head)
    }
}

type Path = Vec<usize>;

fn is_container(kind: &Kind) -> bool {
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

fn is_item(kind: &Kind) -> bool {
    matches!(kind, Kind::ListItem | Kind::TaskItem)
}

fn node_at<'a>(root: &'a Node, path: &[usize]) -> &'a Node {
    path.iter().fold(root, |node, index| &node.content[*index])
}

fn node_at_mut<'a>(root: &'a mut Node, path: &[usize]) -> &'a mut Node {
    let mut node = root;
    for index in path {
        node = &mut node.content[*index];
    }
    node
}

/// Blocs dans l'ordre du document : `(chemin, est_un_bloc_de_texte)`.
fn blocks_in_order(root: &Node) -> Vec<(Path, bool)> {
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

/// Retire un nœud et les conteneurs qui deviennent vides.
fn remove_at(root: &mut Node, path: &[usize]) {
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

// --- Atomes : le texte d'un bloc caractère par caractère -----------------------------------------

#[derive(Debug, Clone)]
struct Atom {
    ch: char,
    marks: Vec<Mark>,
    hard_break: bool,
    /// Nœud en ligne inconnu conservé tel quel (un seul caractère de substitution).
    node: Option<Node>,
}

/// Nom de la marque interne qui porte les attributs d'une mention le temps de l'édition.
const MENTION: &str = "mention";

fn mention_text(attrs: &Map<String, Value>) -> String {
    let label = ["label", "id"]
        .iter()
        .find_map(|key| attrs.get(*key).and_then(Value::as_str).filter(|v| !v.is_empty()))
        .unwrap_or("Lien");
    format!("[[{label}]]")
}

fn mention_mark(atom: &Atom) -> Option<&Mark> {
    atom.marks.iter().find(|m| m.kind == MarkKind::Other(MENTION.to_owned()))
}

/// Atomes d'une mention : le texte `[[titre]]`, chaque caractère portant la marque interne.
fn mention_atoms(attrs: &Map<String, Value>, extra: &[Mark]) -> Vec<Atom> {
    let mut mark = Mark::new(MarkKind::Other(MENTION.to_owned()));
    mark.attrs.clone_from(attrs);
    let mut marks = vec![mark];
    marks.extend(extra.iter().cloned());
    mention_text(attrs)
        .chars()
        .map(|ch| Atom { ch, marks: marks.clone(), hard_break: false, node: None })
        .collect()
}

fn atoms_of(block: &Node) -> Vec<Atom> {
    let mut atoms = Vec::new();
    for child in &block.content {
        match &child.kind {
            Kind::Text => {
                for ch in child.text.as_deref().unwrap_or_default().chars() {
                    atoms.push(Atom {
                        ch,
                        marks: child.marks.clone(),
                        hard_break: false,
                        node: None,
                    });
                }
            }
            Kind::HardBreak => atoms.push(Atom {
                ch: '\n',
                marks: Vec::new(),
                hard_break: true,
                node: None,
            }),
            Kind::Other(name) if name == MENTION => {
                atoms.extend(mention_atoms(&child.attrs, &child.marks));
            }
            Kind::Other(_) => atoms.push(Atom {
                ch: '\u{fffc}',
                marks: Vec::new(),
                hard_break: false,
                node: Some(child.clone()),
            }),
            _ => {}
        }
    }
    atoms
}

fn rebuild(block: &mut Node, atoms: &[Atom]) {
    block.content.clear();
    block.shape.content = false;
    if block.kind == Kind::CodeBlock {
        let text: String = atoms.iter().map(|atom| atom.ch).collect();
        if !text.is_empty() {
            block.content.push(Node::text(text));
        }
        return;
    }
    let mut run: Option<Node> = None;
    let mut index = 0;
    while index < atoms.len() {
        let atom = &atoms[index];
        if let Some(node) = &atom.node {
            block.content.extend(run.take());
            block.content.push(node.clone());
            index += 1;
            continue;
        }
        if atom.hard_break {
            block.content.extend(run.take());
            block.content.push(Node::new(Kind::HardBreak));
            index += 1;
            continue;
        }
        let mut marks = atom.marks.clone();
        if let Some(mark) = mention_mark(atom) {
            let expected: Vec<char> = mention_text(&mark.attrs).chars().collect();
            let whole = atoms.get(index..index + expected.len()).is_some_and(|slice| {
                slice.iter().zip(&expected).all(|(a, c)| a.ch == *c && mention_mark(a) == Some(mark))
            });
            if whole {
                block.content.extend(run.take());
                let mut node = Node::new(Kind::Other(MENTION.to_owned()));
                node.attrs.clone_from(&mark.attrs);
                node.marks = atom.marks.iter().filter(|m| m.kind != mark.kind).cloned().collect();
                block.content.push(node);
                index += expected.len();
                continue;
            }
            // Mention modifiée à la main : elle redevient du texte ordinaire.
            marks.retain(|m| m.kind != mark.kind);
        }
        match &mut run {
            Some(node) if node.marks == marks => {
                node.text.get_or_insert_with(String::new).push(atom.ch);
            }
            _ => {
                block.content.extend(run.take());
                let mut node = Node::text(atom.ch.to_string());
                node.marks = marks;
                run = Some(node);
            }
        }
        index += 1;
    }
    block.content.extend(run);
}

fn flat_text(atoms: &[Atom]) -> String {
    atoms.iter().map(|atom| atom.ch).collect()
}

/// Limites de graphèmes, en décalages de caractères (0 et la longueur inclus).
fn grapheme_bounds(text: &str) -> Vec<usize> {
    let mut bounds = vec![0];
    let mut chars = 0;
    for grapheme in text.graphemes(true) {
        chars += grapheme.chars().count();
        bounds.push(chars);
    }
    bounds
}

fn prev_bound(text: &str, offset: usize) -> usize {
    grapheme_bounds(text)
        .into_iter()
        .rev()
        .find(|bound| *bound < offset)
        .unwrap_or(0)
}

fn next_bound(text: &str, offset: usize) -> usize {
    let len = text.chars().count();
    grapheme_bounds(text)
        .into_iter()
        .find(|bound| *bound > offset)
        .unwrap_or(len)
}

/// Début et fin des mots, en décalages de caractères.
fn word_spans(text: &str) -> Vec<(usize, usize)> {
    text.unicode_word_indices()
        .map(|(byte, word)| {
            let start = text[..byte].chars().count();
            (start, start + word.chars().count())
        })
        .collect()
}

/// Début du délimiteur ouvrant qui correspond au délimiteur fermant en fin de `chars`.
fn find_opening(chars: &[char], delimiter: &[char]) -> Option<usize> {
    let n = delimiter.len();
    let end = chars.len();
    if end < 2 * n + 1 || chars[end - n..] != *delimiter {
        return None;
    }
    let closing = end - n;
    let single_star = delimiter == ['*'];
    if single_star && chars[closing - 1] == '*' {
        return None;
    }
    (0..closing).rev().filter(|i| i + n <= closing).find(|&i| {
        if chars[i..i + n] != *delimiter {
            return false;
        }
        let content = &chars[i + n..closing];
        let (Some(first), Some(last)) = (content.first(), content.last()) else {
            return false;
        };
        if first.is_whitespace() || last.is_whitespace() {
            return false;
        }
        if single_star && ((i > 0 && chars[i - 1] == '*') || *first == '*') {
            return false;
        }
        if delimiter == ['`'] && content.contains(&'`') {
            return false;
        }
        if delimiter == ['_'] && i > 0 && chars[i - 1].is_alphanumeric() {
            return false;
        }
        true
    })
}

// --- Éditeur ----------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Snapshot {
    doc: Node,
    selection: Selection,
}

/// Type de bloc que l'on peut insérer (menu `/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockCommand {
    Paragraph,
    Heading(u8),
    BulletList,
    OrderedList,
    TaskList,
    Quote,
    CodeBlock,
    Rule,
}

/// Document en cours d'édition avec sa sélection et son historique.
#[derive(Debug, Clone)]
pub struct Editor {
    doc: Node,
    /// Paths of the text blocks, in document order; reset by every mutation of `doc`.
    index: OnceLock<Vec<Path>>,
    selection: Selection,
    stored_marks: Option<Vec<Mark>>,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    typing: Option<Pos>,
}

const HISTORY_LIMIT: usize = 500;

impl Editor {
    /// Ouvre un document ; un document sans bloc de texte reçoit un paragraphe vide.
    #[must_use]
    pub fn new(mut doc: Node) -> Self {
        if !blocks_in_order(&doc).iter().any(|(_, text)| *text) {
            doc.content.push(Node::new(Kind::Paragraph));
        }
        Self {
            doc,
            index: OnceLock::new(),
            selection: Selection::default(),
            stored_marks: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
            typing: None,
        }
    }

    #[must_use]
    pub fn doc(&self) -> &Node {
        &self.doc
    }

    #[must_use]
    pub fn into_doc(self) -> Node {
        self.doc
    }

    #[must_use]
    pub fn selection(&self) -> Selection {
        self.selection
    }

    fn textblocks(&self) -> &[Path] {
        let index = self.index.get_or_init(|| {
            blocks_in_order(&self.doc)
                .into_iter()
                .filter(|(_, text)| *text)
                .map(|(path, _)| path)
                .collect()
        });
        #[cfg(debug_assertions)]
        {
            let fresh: Vec<Path> = blocks_in_order(&self.doc)
                .into_iter()
                .filter(|(_, text)| *text)
                .map(|(path, _)| path)
                .collect();
            debug_assert_eq!(*index, fresh, "stale text block index");
        }
        index
    }

    /// Mutable access to the document; invalidates the text block index.
    fn doc_mut(&mut self) -> &mut Node {
        self.index.take();
        &mut self.doc
    }

    fn set_doc(&mut self, doc: Node) {
        self.index.take();
        self.doc = doc;
    }

    #[must_use]
    pub fn block_count(&self) -> usize {
        self.textblocks().len()
    }

    fn path_of(&self, block: usize) -> Path {
        let blocks = self.textblocks();
        blocks[block.min(blocks.len() - 1)].clone()
    }

    fn atoms(&self, block: usize) -> Vec<Atom> {
        atoms_of(node_at(&self.doc, &self.path_of(block)))
    }

    fn set_atoms(&mut self, block: usize, atoms: &[Atom]) {
        let path = self.path_of(block);
        rebuild(node_at_mut(self.doc_mut(), &path), atoms);
    }

    /// Texte du bloc (un saut de ligne pour chaque saut dur).
    #[must_use]
    pub fn block_text(&self, block: usize) -> String {
        flat_text(&self.atoms(block))
    }

    #[must_use]
    pub fn block_len(&self, block: usize) -> usize {
        self.atoms(block).len()
    }

    #[must_use]
    pub fn block_kind(&self, block: usize) -> Kind {
        node_at(&self.doc, &self.path_of(block)).kind.clone()
    }

    /// Nœud du bloc de texte (pour le rendu).
    #[must_use]
    pub fn block_node(&self, block: usize) -> &Node {
        node_at(&self.doc, &self.path_of(block))
    }

    fn clamp(&self, pos: Pos) -> Pos {
        let count = self.block_count();
        let block = pos.block.min(count - 1);
        Pos {
            block,
            offset: pos.offset.min(self.block_len(block)),
        }
    }

    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = Selection {
            anchor: self.clamp(selection.anchor),
            head: self.clamp(selection.head),
        };
        self.stored_marks = None;
        self.typing = None;
    }

    // --- Historique ---

    fn checkpoint(&mut self) {
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

    // --- Lecture de la sélection ---

    /// Texte sélectionné ; un saut de ligne sépare les blocs.
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

    /// La marque est-elle active sur toute la sélection (ou au curseur) ?
    #[must_use]
    pub fn is_mark_active(&self, kind: &MarkKind) -> bool {
        if self.selection.is_collapsed() {
            return self.marks_at_caret().iter().any(|mark| mark.kind == *kind);
        }
        self.selected_atoms()
            .iter()
            .all(|atom| atom.marks.iter().any(|mark| mark.kind == *kind))
    }

    fn selected_atoms(&self) -> Vec<Atom> {
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

    fn marks_at_caret(&self) -> Vec<Mark> {
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

    // --- Déplacements ---

    fn go(&mut self, pos: Pos, extend: bool) {
        self.selection.head = pos;
        if !extend {
            self.selection.anchor = pos;
        }
        self.stored_marks = None;
        self.typing = None;
    }

    /// Place le curseur (ou étend la sélection) à `pos`.
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

    // --- Suppression ---

    /// Supprime l'intervalle `[start, end)` et renvoie la position de fusion.
    fn delete_range(&mut self, start: Pos, end: Pos) -> Pos {
        if start == end {
            return start;
        }
        if start.block == end.block {
            let mut atoms = self.atoms(start.block);
            atoms.drain(start.offset.min(atoms.len())..end.offset.min(atoms.len()));
            self.set_atoms(start.block, &atoms);
            return start;
        }
        let textblocks = self.textblocks();
        let (start_path, end_path) = (
            textblocks[start.block].clone(),
            textblocks[end.block].clone(),
        );
        let mut merged = self.atoms(start.block);
        merged.truncate(start.offset);
        let end_atoms = self.atoms(end.block);
        merged.extend(end_atoms.into_iter().skip(end.offset));
        let order = blocks_in_order(&self.doc);
        let from = order
            .iter()
            .position(|(path, _)| *path == start_path)
            .unwrap_or(0);
        let to = order
            .iter()
            .position(|(path, _)| *path == end_path)
            .unwrap_or(from);
        let mut doomed: Vec<Path> = order[from + 1..=to]
            .iter()
            .map(|(path, _)| path.clone())
            .collect();
        rebuild(node_at_mut(self.doc_mut(), &start_path), &merged);
        doomed.sort();
        for path in doomed.iter().rev() {
            remove_at(self.doc_mut(), path);
        }
        self.ensure_textblock();
        start
    }

    fn ensure_textblock(&mut self) {
        if !blocks_in_order(&self.doc).iter().any(|(_, text)| *text) {
            self.doc_mut().content.push(Node::new(Kind::Paragraph));
        }
    }

    fn delete_selection_inner(&mut self) {
        let (start, end) = (self.selection.start(), self.selection.end());
        let pos = self.delete_range(start, end);
        self.selection = Selection::caret(pos);
    }

    /// Un bloc inconnu (vue de base de données, image…) se trouve entre les blocs de texte `first` et `last`.
    fn unknown_block_between(&self, first: usize, last: usize) -> bool {
        let textblocks = self.textblocks();
        let order = blocks_in_order(&self.doc);
        let position = |block: usize| order.iter().position(|(path, _)| *path == textblocks[block]);
        let (Some(from), Some(to)) = (position(first), position(last)) else {
            return false;
        };
        order[from + 1..to]
            .iter()
            .any(|(path, text)| !*text && matches!(node_at(&self.doc, path).kind, Kind::Other(_)))
    }

    pub fn delete_backward(&mut self) {
        // Fusionner par-dessus un bloc inconnu le supprimerait sans que l'utilisateur le voie : on ne fait rien.
        let head = self.selection.head;
        if self.selection.is_collapsed()
            && head.offset == 0
            && self.block_kind(head.block) == Kind::Paragraph
            && head.block > 0
            && self.unknown_block_between(head.block - 1, head.block)
        {
            return;
        }
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
            return;
        }
        let head = self.selection.head;
        if head.offset > 0 {
            let from = prev_bound(&self.block_text(head.block), head.offset);
            let pos = self.delete_range(
                Pos {
                    block: head.block,
                    offset: from,
                },
                head,
            );
            self.selection = Selection::caret(pos);
        } else if self.block_kind(head.block) != Kind::Paragraph {
            self.convert_block(head.block, Kind::Paragraph, Map::new());
        } else if head.block > 0 {
            let previous = Pos {
                block: head.block - 1,
                offset: self.block_len(head.block - 1),
            };
            let pos = self.delete_range(previous, head);
            self.selection = Selection::caret(pos);
        }
    }

    pub fn delete_forward(&mut self) {
        let head = self.selection.head;
        if self.selection.is_collapsed()
            && head.offset == self.block_len(head.block)
            && head.block + 1 < self.block_count()
            && self.unknown_block_between(head.block, head.block + 1)
        {
            return;
        }
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
            return;
        }
        let head = self.selection.head;
        let len = self.block_len(head.block);
        if head.offset < len {
            let to = next_bound(&self.block_text(head.block), head.offset);
            self.delete_range(
                head,
                Pos {
                    block: head.block,
                    offset: to,
                },
            );
        } else if head.block + 1 < self.block_count() {
            self.delete_range(
                head,
                Pos {
                    block: head.block + 1,
                    offset: 0,
                },
            );
        }
    }

    // --- Saisie ---

    /// Insère du texte (remplace la sélection). Un saut de ligne coupe le bloc, sauf dans du code.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let coalesce = self.selection.is_collapsed()
            && !text.contains('\n')
            && self.typing == Some(self.selection.head);
        if coalesce {
            self.redo.clear();
        } else {
            self.checkpoint();
        }
        self.insert_text_raw(text);
        let converted = match text {
            " " => self.block_rule(),
            "*" | "`" | "~" | "_" => self.inline_rule(text.chars().next().unwrap_or(' ')),
            _ => false,
        };
        self.typing = (!converted && !text.contains('\n')).then_some(self.selection.head);
    }

    fn insert_text_raw(&mut self, text: &str) {
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        let marks = self.marks_at_caret();
        let mut segments = text.split('\n').peekable();
        while let Some(segment) = segments.next() {
            self.insert_in_block(segment, &marks);
            if segments.peek().is_some() {
                self.split_inner();
            }
        }
        self.stored_marks = None;
    }

    /// Identifiant de la mention touchée par une position (le caractère avant ou après), s'il y en a.
    #[must_use]
    pub fn mention_near(&self, pos: Pos) -> Option<String> {
        let pos = self.clamp(pos);
        let atoms = self.atoms(pos.block);
        [Some(pos.offset), pos.offset.checked_sub(1)]
            .into_iter()
            .flatten()
            .filter_map(|index| atoms.get(index))
            .find_map(|atom| mention_mark(atom).and_then(|m| m.attrs.get("id")).and_then(Value::as_str).map(str::to_owned))
    }

    /// Remplace `[start, end)` (par exemple `[[requête`) par une mention de `label` pointant vers
    /// `id`, en un seul pas d'historique ; le curseur se place juste après.
    pub fn insert_mention(&mut self, start: Pos, end: Pos, id: &str, label: &str) {
        self.checkpoint();
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.selection = Selection { anchor: start, head: end };
        if start != end {
            self.delete_selection_inner();
        }
        let head = self.selection.head;
        if self.block_kind(head.block) == Kind::CodeBlock {
            return;
        }
        let mut attrs = Map::new();
        attrs.insert("id".into(), Value::String(id.to_owned()));
        attrs.insert("label".into(), Value::String(label.to_owned()));
        attrs.insert("mentionSuggestionChar".into(), Value::String("[[".to_owned()));
        let inserted = mention_atoms(&attrs, &[]);
        let count = inserted.len();
        let mut atoms = self.atoms(head.block);
        let at = head.offset.min(atoms.len());
        atoms.splice(at..at, inserted);
        self.set_atoms(head.block, &atoms);
        self.selection = Selection::caret(Pos { block: head.block, offset: at + count });
        self.stored_marks = None;
        self.typing = None;
    }

    /// Ouvre un lot : un seul pas d'historique pour les `replace_range` non enregistrés qui suivent
    /// (composition IME).
    pub fn begin_batch(&mut self) {
        self.checkpoint();
    }

    /// Remplace `[start, end)` par `text` et place le curseur après. `record` ajoute un pas
    /// d'historique ; sinon l'édition rejoint le lot ouvert par [`Editor::begin_batch`].
    pub fn replace_range(&mut self, start: Pos, end: Pos, text: &str, record: bool) {
        if record {
            self.checkpoint();
        } else {
            self.redo.clear();
            self.typing = None;
        }
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.selection = Selection {
            anchor: start,
            head: end,
        };
        if text.is_empty() {
            self.delete_selection_inner();
        } else {
            self.insert_text_raw(text);
        }
        self.typing = None;
    }

    /// Retire `[start, end)` (le texte de commande tapé) puis convertit le bloc, en un seul pas
    /// d'historique.
    pub fn replace_range_with_block(&mut self, start: Pos, end: Pos, command: BlockCommand) {
        self.checkpoint();
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.selection = Selection {
            anchor: start,
            head: end,
        };
        self.delete_selection_inner();
        let block = self.selection.head.block;
        match command {
            BlockCommand::Paragraph => self.convert_block(block, Kind::Paragraph, Map::new()),
            BlockCommand::Heading(level) => {
                self.convert_block(block, Kind::Heading, Self::attrs_of(Some(u64::from(level))));
            }
            BlockCommand::BulletList => self.ensure_list(Kind::BulletList),
            BlockCommand::OrderedList => self.ensure_list(Kind::OrderedList),
            BlockCommand::TaskList => self.ensure_list(Kind::TaskList),
            BlockCommand::Quote => self.wrap_in_blockquote(block),
            BlockCommand::CodeBlock => {
                let mut attrs = Map::new();
                attrs.insert("language".into(), Value::Null);
                self.convert_block(block, Kind::CodeBlock, attrs);
            }
            BlockCommand::Rule => self.insert_rule_after(block),
        }
    }

    /// Insère une règle horizontale (remplace le bloc s'il est vide) suivie d'un paragraphe.
    fn insert_rule_after(&mut self, block: usize) {
        let path = self.path_of(block);
        let empty = self.block_len(block) == 0;
        let Some((index, parent)) = path.split_last() else {
            return;
        };
        let container = node_at_mut(self.doc_mut(), parent);
        let at = if empty {
            container.content[*index] = Node::new(Kind::HorizontalRule);
            *index + 1
        } else {
            container
                .content
                .insert(*index + 1, Node::new(Kind::HorizontalRule));
            *index + 2
        };
        container.content.insert(at, Node::new(Kind::Paragraph));
        let mut fresh = parent.to_vec();
        fresh.push(at);
        let target = self
            .textblocks()
            .iter()
            .position(|p| *p == fresh)
            .unwrap_or(block);
        self.selection = Selection::caret(Pos {
            block: target,
            offset: 0,
        });
    }

    /// Sélectionne le mot qui contient `pos` (sans effet hors d'un mot).
    pub fn select_word_at(&mut self, pos: Pos) {
        let pos = self.clamp(pos);
        let spans = word_spans(&self.block_text(pos.block));
        let found = spans
            .iter()
            .find(|(start, end)| *start <= pos.offset && pos.offset <= *end);
        self.selection = match found {
            Some((start, end)) => Selection {
                anchor: Pos {
                    block: pos.block,
                    offset: *start,
                },
                head: Pos {
                    block: pos.block,
                    offset: *end,
                },
            },
            None => Selection::caret(pos),
        };
        self.stored_marks = None;
        self.typing = None;
    }

    /// Sélectionne tout le texte d'un bloc.
    pub fn select_block(&mut self, block: usize) {
        let block = block.min(self.block_count() - 1);
        self.selection = Selection {
            anchor: Pos { block, offset: 0 },
            head: Pos {
                block,
                offset: self.block_len(block),
            },
        };
        self.stored_marks = None;
        self.typing = None;
    }

    fn insert_in_block(&mut self, text: &str, marks: &[Mark]) {
        let head = self.selection.head;
        let mut atoms = self.atoms(head.block);
        let code = self.block_kind(head.block) == Kind::CodeBlock;
        let inserted: Vec<Atom> = text
            .chars()
            .map(|ch| Atom {
                ch,
                marks: if code { Vec::new() } else { marks.to_vec() },
                hard_break: false,
                node: None,
            })
            .collect();
        let count = inserted.len();
        let at = head.offset.min(atoms.len());
        atoms.splice(at..at, inserted);
        self.set_atoms(head.block, &atoms);
        self.selection = Selection::caret(Pos {
            block: head.block,
            offset: at + count,
        });
    }

    pub fn insert_hard_break(&mut self) {
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        let head = self.selection.head;
        if self.block_kind(head.block) == Kind::CodeBlock {
            self.insert_in_block("\n", &[]);
            return;
        }
        let mut atoms = self.atoms(head.block);
        let at = head.offset.min(atoms.len());
        atoms.insert(
            at,
            Atom {
                ch: '\n',
                marks: Vec::new(),
                hard_break: true,
                node: None,
            },
        );
        self.set_atoms(head.block, &atoms);
        self.selection = Selection::caret(Pos {
            block: head.block,
            offset: at + 1,
        });
    }

    /// Touche Entrée.
    pub fn split_block(&mut self) {
        self.checkpoint();
        if !self.selection.is_collapsed() {
            self.delete_selection_inner();
        }
        if self.enter_rule() || self.leave_code_block() {
            return;
        }
        if self.block_kind(self.selection.head.block) == Kind::CodeBlock {
            self.insert_in_block("\n", &[]);
        } else {
            self.split_inner();
        }
    }

    fn split_inner(&mut self) {
        let head = self.selection.head;
        let path = self.path_of(head.block);
        let kind = node_at(&self.doc, &path).kind.clone();
        if kind == Kind::CodeBlock {
            self.insert_in_block("\n", &[]);
            return;
        }
        let atoms = self.atoms(head.block);
        let at = head.offset.min(atoms.len());
        let (left, right) = atoms.split_at(at);
        let parent_path = &path[..path.len() - 1];
        let in_item = is_item(&node_at(&self.doc, parent_path).kind);
        if in_item && atoms.is_empty() && node_at(&self.doc, parent_path).content.len() == 1 {
            self.exit_list(&path);
            return;
        }
        let mut new_block = Node::new(if at == atoms.len() && kind == Kind::Heading {
            Kind::Paragraph
        } else {
            kind.clone()
        });
        if new_block.kind == kind {
            new_block.attrs.clone_from(&node_at(&self.doc, &path).attrs);
        }
        rebuild(&mut new_block, right);
        rebuild(node_at_mut(self.doc_mut(), &path), left);
        let index = path[path.len() - 1];
        let new_path = if in_item {
            let item_index = parent_path[parent_path.len() - 1];
            let list_path = &parent_path[..parent_path.len() - 1];
            let following: Vec<Node> = node_at_mut(self.doc_mut(), parent_path)
                .content
                .drain(index + 1..)
                .collect();
            let mut item = Node::new(node_at(&self.doc, parent_path).kind.clone());
            if item.kind == Kind::TaskItem {
                item.attrs.insert("checked".into(), Value::Bool(false));
            }
            item.content.push(new_block);
            item.content.extend(following);
            node_at_mut(self.doc_mut(), list_path)
                .content
                .insert(item_index + 1, item);
            let mut new_path = list_path.to_vec();
            new_path.extend([item_index + 1, 0]);
            new_path
        } else {
            node_at_mut(self.doc_mut(), parent_path)
                .content
                .insert(index + 1, new_block);
            let mut new_path = parent_path.to_vec();
            new_path.push(index + 1);
            new_path
        };
        let block = self
            .textblocks()
            .iter()
            .position(|p| *p == new_path)
            .unwrap_or(head.block + 1);
        self.selection = Selection::caret(Pos { block, offset: 0 });
    }

    /// Sort d'une liste depuis un élément vide : l'élément devient un paragraphe hors de la liste.
    fn exit_list(&mut self, block_path: &[usize]) {
        let item_path = &block_path[..block_path.len() - 1];
        let list_path = &item_path[..item_path.len() - 1];
        let (item_index, list_parent) = (
            item_path[item_path.len() - 1],
            &list_path[..list_path.len() - 1],
        );
        let list_index = list_path[list_path.len() - 1];
        let list_len = node_at(&self.doc, list_path).content.len();
        let paragraph = Node::new(Kind::Paragraph);
        let target = if list_len == 1 {
            node_at_mut(self.doc_mut(), list_parent).content[list_index] = paragraph;
            list_path.to_vec()
        } else if item_index + 1 == list_len {
            node_at_mut(self.doc_mut(), list_path).content.pop();
            node_at_mut(self.doc_mut(), list_parent)
                .content
                .insert(list_index + 1, paragraph);
            let mut path = list_parent.to_vec();
            path.push(list_index + 1);
            path
        } else if item_index == 0 {
            node_at_mut(self.doc_mut(), list_path).content.remove(0);
            node_at_mut(self.doc_mut(), list_parent)
                .content
                .insert(list_index, paragraph);
            let mut path = list_parent.to_vec();
            path.push(list_index);
            path
        } else {
            let tail: Vec<Node> = node_at_mut(self.doc_mut(), list_path)
                .content
                .drain(item_index..)
                .collect();
            let mut second = Node::new(node_at(&self.doc, list_path).kind.clone());
            second
                .attrs
                .clone_from(&node_at(&self.doc, list_path).attrs);
            second.content = tail.into_iter().skip(1).collect();
            let parent = node_at_mut(self.doc_mut(), list_parent);
            parent.content.insert(list_index + 1, paragraph);
            if !second.content.is_empty() {
                parent.content.insert(list_index + 2, second);
            }
            let mut path = list_parent.to_vec();
            path.push(list_index + 1);
            path
        };
        let block = self
            .textblocks()
            .iter()
            .position(|p| *p == target)
            .unwrap_or(0);
        self.selection = Selection::caret(Pos { block, offset: 0 });
    }

    // --- Blocs et marques ---

    /// Change le type des blocs de la sélection.
    fn convert_block(&mut self, block: usize, kind: Kind, attrs: Map<String, Value>) {
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

    fn selected_blocks(&self) -> std::ops::RangeInclusive<usize> {
        self.selection.start().block..=self.selection.end().block
    }

    fn attrs_of(level: Option<u64>) -> Map<String, Value> {
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

    fn toggle_mark_with(&mut self, mark: &Mark) {
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
        Some((Pos { block: pos.block, offset: start }, Pos { block: pos.block, offset: end }, href))
    }

    /// Sélectionne le lien touché par le curseur et retourne son adresse.
    pub fn select_link_at_caret(&mut self) -> Option<String> {
        let (start, end, href) = self.link_at(self.selection.head)?;
        self.selection = Selection { anchor: start, head: end };
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

    fn edit_marks(&mut self, change: impl Fn(&mut Vec<Mark>)) {
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

    // --- Règles de saisie (Markdown) ---

    fn push_snapshot(&mut self) {
        self.undo.push_back(Snapshot {
            doc: self.doc.clone(),
            selection: self.selection,
        });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    /// `# `, `- `, `1. `, `[ ] `, `> ` au début d'un paragraphe : convertit le bloc.
    fn block_rule(&mut self) -> bool {
        enum Rule {
            Heading(u64),
            Bullet,
            Ordered(i64),
            Task(bool),
            Quote,
        }
        let head = self.selection.head;
        if !self.selection.is_collapsed() || self.block_kind(head.block) != Kind::Paragraph {
            return false;
        }
        let atoms = self.atoms(head.block);
        let typed: Vec<&Atom> = atoms.iter().take(head.offset).collect();
        if typed.iter().any(|atom| atom.hard_break) {
            return false;
        }
        let prefix: String = typed.iter().map(|atom| atom.ch).collect();
        let Some(marker) = prefix.strip_suffix(' ') else {
            return false;
        };
        let rule = match marker {
            "-" | "*" | "+" => Rule::Bullet,
            ">" => Rule::Quote,
            "[ ]" => Rule::Task(false),
            "[x]" | "[X]" => Rule::Task(true),
            hashes
                if !hashes.is_empty() && hashes.len() <= 6 && hashes.chars().all(|c| c == '#') =>
            {
                Rule::Heading(hashes.len() as u64)
            }
            number => match number
                .strip_suffix('.')
                .filter(|n| !n.is_empty() && n.len() <= 9 && n.chars().all(|c| c.is_ascii_digit()))
            {
                Some(digits) => Rule::Ordered(digits.parse().unwrap_or(1)),
                None => return false,
            },
        };
        self.push_snapshot();
        let rest: Vec<Atom> = atoms[head.offset..].to_vec();
        let block = head.block;
        self.set_atoms(block, &rest);
        self.selection = Selection::caret(Pos { block, offset: 0 });
        match rule {
            Rule::Heading(level) => {
                self.convert_block(block, Kind::Heading, Self::attrs_of(Some(level)));
            }
            Rule::Bullet => self.ensure_list(Kind::BulletList),
            Rule::Ordered(start) => {
                self.ensure_list(Kind::OrderedList);
                if start != 1
                    && let Some((list_path, _)) = self.list_context(block)
                {
                    node_at_mut(self.doc_mut(), &list_path)
                        .attrs
                        .insert("start".into(), Value::from(start));
                }
            }
            Rule::Task(checked) => {
                self.ensure_list(Kind::TaskList);
                if let Some((list_path, item)) = self.list_context(block) {
                    let item_node = &mut node_at_mut(self.doc_mut(), &list_path).content[item];
                    item_node
                        .attrs
                        .insert("checked".into(), Value::Bool(checked));
                }
            }
            Rule::Quote => self.wrap_in_blockquote(block),
        }
        true
    }

    /// Place le bloc courant dans une liste du type donné (sans la retirer s'il y est déjà).
    fn ensure_list(&mut self, kind: Kind) {
        let block = self.selection.head.block;
        let already = self
            .list_context(block)
            .is_some_and(|(list, _)| node_at(&self.doc, &list).kind == kind);
        if !already {
            self.toggle_list_raw(kind);
        }
    }

    fn wrap_in_blockquote(&mut self, block: usize) {
        let path = self.path_of(block);
        let Some((index, parent)) = path.split_last() else {
            return;
        };
        let container = node_at_mut(self.doc_mut(), parent);
        let child = container.content.remove(*index);
        let mut quote = Node::new(Kind::Blockquote);
        quote.content.push(child);
        container.content.insert(*index, quote);
    }

    /// Cite ou retire la citation du bloc courant.
    pub fn toggle_blockquote(&mut self) {
        self.checkpoint();
        let block = self.selection.head.block;
        let path = self.path_of(block);
        let parent_path = &path[..path.len() - 1];
        if node_at(&self.doc, parent_path).kind == Kind::Blockquote {
            if let Some((index, above)) = parent_path.split_last() {
                let quote = node_at(&self.doc, parent_path).clone();
                node_at_mut(self.doc_mut(), above)
                    .content
                    .splice(*index..=*index, quote.content);
            }
        } else {
            self.wrap_in_blockquote(block);
        }
    }

    /// Ferme un délimiteur en ligne (`**`, `*`, `_`, `` ` ``, `~~`) : applique la marque au texte
    /// qu'il entoure et retire les délimiteurs.
    fn inline_rule(&mut self, typed: char) -> bool {
        let head = self.selection.head;
        if !self.selection.is_collapsed() || self.block_kind(head.block) == Kind::CodeBlock {
            return false;
        }
        let atoms = self.atoms(head.block);
        let end = head.offset.min(atoms.len());
        let chars: Vec<char> = atoms[..end].iter().map(|atom| atom.ch).collect();
        let candidates: &[(&str, MarkKind)] = match typed {
            '`' => &[("`", MarkKind::Code)],
            '~' => &[("~~", MarkKind::Strike)],
            '_' => &[("_", MarkKind::Italic)],
            '*' => &[("**", MarkKind::Bold), ("*", MarkKind::Italic)],
            _ => return false,
        };
        for (delimiter, kind) in candidates {
            let delimiter: Vec<char> = delimiter.chars().collect();
            if let Some(open) = find_opening(&chars, &delimiter) {
                let n = delimiter.len();
                let closing = end - n;
                let spans_ok = atoms[open..end]
                    .iter()
                    .all(|a| !a.hard_break && !a.marks.iter().any(|m| m.kind == MarkKind::Code));
                if !spans_ok {
                    continue;
                }
                self.push_snapshot();
                let outer: Vec<Mark> = open
                    .checked_sub(1)
                    .map(|i| atoms[i].marks.clone())
                    .unwrap_or_default();
                let mut result: Vec<Atom> = atoms[..open].to_vec();
                for atom in &atoms[open + n..closing] {
                    let mut marked = atom.clone();
                    if *kind == MarkKind::Code {
                        marked.marks = vec![Mark::new(MarkKind::Code)];
                    } else {
                        marked.marks.retain(|m| m.kind != *kind);
                        marked.marks.push(Mark::new(kind.clone()));
                    }
                    result.push(marked);
                }
                let caret = result.len();
                result.extend(atoms[end..].iter().cloned());
                self.set_atoms(head.block, &result);
                self.selection = Selection::caret(Pos {
                    block: head.block,
                    offset: caret,
                });
                self.stored_marks = Some(outer);
                return true;
            }
        }
        false
    }

    /// Entrée sur une ligne vide en fin de bloc de code : retire cette ligne et sort du bloc.
    fn leave_code_block(&mut self) -> bool {
        let head = self.selection.head;
        if self.block_kind(head.block) != Kind::CodeBlock {
            return false;
        }
        let mut atoms = self.atoms(head.block);
        let at_end = head.offset == atoms.len();
        if !at_end || atoms.last().is_none_or(|atom| atom.ch != '\n') {
            return false;
        }
        atoms.pop();
        self.set_atoms(head.block, &atoms);
        let path = self.path_of(head.block);
        let Some((index, parent)) = path.split_last() else {
            return false;
        };
        node_at_mut(self.doc_mut(), parent)
            .content
            .insert(*index + 1, Node::new(Kind::Paragraph));
        let mut fresh = parent.to_vec();
        fresh.push(*index + 1);
        let block = self
            .textblocks()
            .iter()
            .position(|p| *p == fresh)
            .unwrap_or(head.block + 1);
        self.selection = Selection::caret(Pos { block, offset: 0 });
        true
    }

    /// Entrée sur ```` ```langue ```` : bloc de code ; sur `---` : règle horizontale.
    fn enter_rule(&mut self) -> bool {
        let head = self.selection.head;
        if self.block_kind(head.block) != Kind::Paragraph {
            return false;
        }
        let atoms = self.atoms(head.block);
        if head.offset != atoms.len() || atoms.iter().any(|a| a.hard_break) {
            return false;
        }
        let text = flat_text(&atoms);
        if let Some(language) = text.strip_prefix("```")
            && language
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '+' | '#'))
        {
            let mut attrs = Map::new();
            attrs.insert(
                "language".into(),
                if language.is_empty() {
                    Value::Null
                } else {
                    Value::String(language.to_owned())
                },
            );
            self.set_atoms(head.block, &[]);
            self.convert_block(head.block, Kind::CodeBlock, attrs);
            self.selection = Selection::caret(Pos {
                block: head.block,
                offset: 0,
            });
            return true;
        }
        if matches!(text.as_str(), "---" | "***" | "___") {
            let path = self.path_of(head.block);
            let Some((index, parent)) = path.split_last() else {
                return false;
            };
            let container = node_at_mut(self.doc_mut(), parent);
            container.content[*index] = Node::new(Kind::HorizontalRule);
            container
                .content
                .insert(*index + 1, Node::new(Kind::Paragraph));
            let mut fresh = parent.to_vec();
            fresh.push(*index + 1);
            let block = self
                .textblocks()
                .iter()
                .position(|p| *p == fresh)
                .unwrap_or(head.block);
            self.selection = Selection::caret(Pos { block, offset: 0 });
            return true;
        }
        false
    }

    // --- Listes ---

    fn list_context(&self, block: usize) -> Option<(Path, usize)> {
        let path = self.path_of(block);
        let parent = &path[..path.len() - 1];
        if !is_item(&node_at(&self.doc, parent).kind) {
            return None;
        }
        let list = parent[..parent.len() - 1].to_vec();
        Some((list, parent[parent.len() - 1]))
    }

    /// Met le bloc courant dans une liste du type donné, change le type de sa liste, ou en sort.
    pub fn toggle_list(&mut self, kind: Kind) {
        self.checkpoint();
        self.toggle_list_raw(kind);
    }

    fn toggle_list_raw(&mut self, kind: Kind) {
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

    /// Coche ou décoche la tâche du bloc courant.
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

    /// Imbrique l'élément courant dans l'élément précédent.
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

    /// Sort l'élément courant d'une liste imbriquée, juste après son parent.
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

    fn reselect(&mut self, block: usize) {
        let block = block.min(self.block_count() - 1);
        let offset = self.selection.head.offset.min(self.block_len(block));
        self.selection = Selection::caret(Pos { block, offset });
    }
}
