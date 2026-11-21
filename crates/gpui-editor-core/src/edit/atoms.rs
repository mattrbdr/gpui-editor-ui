//! Atoms: the content of a text block, one character at a time.

use serde_json::{Map, Value};
use unicode_segmentation::UnicodeSegmentation;

use crate::{Kind, Mark, MarkKind, Node};

#[derive(Debug, Clone)]
pub(super) struct Atom {
    pub(super) ch: char,
    pub(super) marks: Vec<Mark>,
    pub(super) hard_break: bool,
    /// Nœud en ligne inconnu conservé tel quel (un seul caractère de substitution).
    pub(super) node: Option<Node>,
}

/// Nom de la marque interne qui porte les attributs d'une mention le temps de l'édition.
pub(super) const MENTION: &str = "mention";

pub(super) fn mention_text(attrs: &Map<String, Value>) -> String {
    let label = ["label", "id"]
        .iter()
        .find_map(|key| {
            attrs
                .get(*key)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
        })
        .unwrap_or("Lien");
    format!("[[{label}]]")
}

pub(super) fn mention_mark(atom: &Atom) -> Option<&Mark> {
    atom.marks
        .iter()
        .find(|m| m.kind == MarkKind::Other(MENTION.to_owned()))
}

/// Atomes d'une mention : le texte `[[titre]]`, chaque caractère portant la marque interne.
pub(super) fn mention_atoms(attrs: &Map<String, Value>, extra: &[Mark]) -> Vec<Atom> {
    let mut mark = Mark::new(MarkKind::Other(MENTION.to_owned()));
    mark.attrs.clone_from(attrs);
    let mut marks = vec![mark];
    marks.extend(extra.iter().cloned());
    mention_text(attrs)
        .chars()
        .map(|ch| Atom {
            ch,
            marks: marks.clone(),
            hard_break: false,
            node: None,
        })
        .collect()
}

pub(super) fn atoms_of(block: &Node) -> Vec<Atom> {
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

pub(super) fn rebuild(block: &mut Node, atoms: &[Atom]) {
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
            let whole = atoms
                .get(index..index + expected.len())
                .is_some_and(|slice| {
                    slice
                        .iter()
                        .zip(&expected)
                        .all(|(a, c)| a.ch == *c && mention_mark(a) == Some(mark))
                });
            if whole {
                block.content.extend(run.take());
                let mut node = Node::new(Kind::Other(MENTION.to_owned()));
                node.attrs.clone_from(&mark.attrs);
                node.marks = atom
                    .marks
                    .iter()
                    .filter(|m| m.kind != mark.kind)
                    .cloned()
                    .collect();
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

pub(super) fn flat_text(atoms: &[Atom]) -> String {
    atoms.iter().map(|atom| atom.ch).collect()
}

/// Limites de graphèmes, en décalages de caractères (0 et la longueur inclus).
pub(super) fn grapheme_bounds(text: &str) -> Vec<usize> {
    let mut bounds = vec![0];
    let mut chars = 0;
    for grapheme in text.graphemes(true) {
        chars += grapheme.chars().count();
        bounds.push(chars);
    }
    bounds
}

pub(super) fn prev_bound(text: &str, offset: usize) -> usize {
    grapheme_bounds(text)
        .into_iter()
        .rev()
        .find(|bound| *bound < offset)
        .unwrap_or(0)
}

pub(super) fn next_bound(text: &str, offset: usize) -> usize {
    let len = text.chars().count();
    grapheme_bounds(text)
        .into_iter()
        .find(|bound| *bound > offset)
        .unwrap_or(len)
}

/// Début et fin des mots, en décalages de caractères.
pub(super) fn word_spans(text: &str) -> Vec<(usize, usize)> {
    text.unicode_word_indices()
        .map(|(byte, word)| {
            let start = text[..byte].chars().count();
            (start, start + word.chars().count())
        })
        .collect()
}

/// Début du délimiteur ouvrant qui correspond au délimiteur fermant en fin de `chars`.
pub(super) fn find_opening(chars: &[char], delimiter: &[char]) -> Option<usize> {
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
