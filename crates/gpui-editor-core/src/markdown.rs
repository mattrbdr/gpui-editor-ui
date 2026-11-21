//! Markdown export.
//!
//! Two flavors: [`Flavor::CommonMark`] (the default) produces correct CommonMark/GFM;
//! [`Flavor::TipTapCompat`] reproduces the `@tiptap/markdown` serializer (HTML entities, limited
//! escaping, two-space line breaks…) so that an export stays identical to that of a TipTap
//! application.

use serde_json::Value;

use crate::{Kind, Mark, MarkKind, Node};

/// Markdown variant to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flavor {
    /// CommonMark with the GFM extensions (strikethrough, task lists); text is escaped correctly.
    #[default]
    CommonMark,
    /// Reproduces `@tiptap/markdown`, quirks included.
    TipTapCompat,
}

/// Export options.
#[derive(Debug, Clone, Default)]
pub struct MarkdownOptions {
    pub flavor: Flavor,
}

impl Node {
    /// CommonMark Markdown of the node (without a trailing line break).
    #[must_use]
    pub fn to_markdown(&self) -> String {
        self.to_markdown_with(&MarkdownOptions::default())
    }

    /// Markdown of the node in the chosen flavor.
    #[must_use]
    pub fn to_markdown_with(&self, options: &MarkdownOptions) -> String {
        let compat = options.flavor == Flavor::TipTapCompat;
        let out = if self.kind == Kind::Doc {
            blocks(&self.content, compat)
        } else {
            block(self, &[], 0, compat)
        };
        if out
            .replace("&nbsp;", "")
            .replace('\u{a0}', "")
            .trim()
            .is_empty()
        {
            String::new()
        } else {
            out
        }
    }
}

fn is_list(kind: &Kind) -> bool {
    matches!(kind, Kind::BulletList | Kind::OrderedList | Kind::TaskList)
}

fn blocks(nodes: &[Node], compat: bool) -> String {
    let pieces = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| block(node, nodes, index, compat));
    if compat {
        pieces.collect::<Vec<_>>().join("\n\n")
    } else {
        pieces
            .filter(|piece| !piece.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

fn text_of(node: &Node) -> String {
    node.content
        .iter()
        .filter_map(|child| child.text.as_deref())
        .collect()
}

fn attr_str<'a>(node: &'a Node, key: &str) -> &'a str {
    node.attrs
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn longest_backtick_run(text: &str) -> usize {
    let (mut best, mut run) = (0, 0);
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

fn block(node: &Node, siblings: &[Node], index: usize, compat: bool) -> String {
    match &node.kind {
        Kind::Paragraph => {
            if node.content.is_empty() {
                let previous_empty = index > 0
                    && siblings[index - 1].kind == Kind::Paragraph
                    && siblings[index - 1].content.is_empty();
                if compat && previous_empty {
                    "&nbsp;".to_owned()
                } else {
                    String::new()
                }
            } else {
                inline(&node.content, compat)
            }
        }
        Kind::Heading => {
            if node.content.is_empty() {
                return String::new();
            }
            let level = node
                .attrs
                .get("level")
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
                .unwrap_or(1);
            let level = if compat { level } else { level.min(6) };
            format!(
                "{} {}",
                "#".repeat(usize::try_from(level).unwrap_or(1)),
                inline(&node.content, compat)
            )
        }
        Kind::CodeBlock => code_block(node, compat),
        Kind::Blockquote => {
            if node.content.is_empty() {
                return String::new();
            }
            blocks(&node.content, compat)
                .split('\n')
                .map(|line| {
                    if line.trim().is_empty() {
                        ">".to_owned()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Kind::BulletList | Kind::TaskList | Kind::OrderedList => list(node, compat),
        Kind::HorizontalRule => "---".to_owned(),
        Kind::ListItem | Kind::TaskItem => item_content(node, compat),
        Kind::Other(name) if name == "image" => image(node, compat),
        Kind::Text | Kind::HardBreak => inline(std::slice::from_ref(node), compat),
        Kind::Doc => blocks(&node.content, compat),
        // `[[title]]` mentions are kept in the CommonMark export; the TipTap flavor drops them,
        // like TipTap does.
        Kind::Other(name) if name == "mention" && !compat => ["label", "id"]
            .iter()
            .find_map(|key| {
                node.attrs
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|v| !v.is_empty())
            })
            .map_or_else(String::new, |label| format!("[[{label}]]")),
        Kind::Other(_) => String::new(),
    }
}

fn code_block(node: &Node, compat: bool) -> String {
    let language = attr_str(node, "language");
    let text = text_of(node);
    if compat {
        if node.content.is_empty() {
            format!("```{language}\n\n```")
        } else {
            format!("```{language}\n{text}\n```")
        }
    } else {
        let fence = "`".repeat((longest_backtick_run(&text) + 1).max(3));
        if text.is_empty() {
            format!("{fence}{language}\n{fence}")
        } else {
            format!("{fence}{language}\n{text}\n{fence}")
        }
    }
}

fn image(node: &Node, compat: bool) -> String {
    let (src, alt, title) = (
        attr_str(node, "src"),
        attr_str(node, "alt"),
        attr_str(node, "title"),
    );
    let (alt, title) = if compat {
        (alt.to_owned(), title.to_owned())
    } else {
        (
            escape_link_text(alt),
            title.replace('\\', "\\\\").replace('"', "\\\""),
        )
    };
    let src = if compat {
        src.to_owned()
    } else {
        link_destination(src)
    };
    if title.is_empty() {
        format!("![{alt}]({src})")
    } else {
        format!("![{alt}]({src} \"{title}\")")
    }
}

fn escape_link_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

/// Link destination: wrapped in angle brackets if it contains whitespace or unbalanced parentheses.
fn link_destination(href: &str) -> String {
    let mut depth = 0i32;
    let mut balanced = true;
    for c in href.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                balanced &= depth >= 0;
            }
            _ => {}
        }
    }
    let plain = !href.is_empty()
        && balanced
        && depth == 0
        && !href
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>');
    if plain {
        href.to_owned()
    } else {
        format!(
            "<{}>",
            href.replace('<', "%3C")
                .replace('>', "%3E")
                .replace('\n', "%0A")
        )
    }
}

/// List item content: children separated by a blank line, except nested lists or code blocks.
fn item_content(item: &Node, compat: bool) -> String {
    let mut out = String::new();
    for (index, child) in item.content.iter().enumerate() {
        let piece = block(child, &item.content, index, compat);
        if piece.is_empty() && !compat {
            continue;
        }
        if index > 0 || !out.is_empty() {
            let tight = is_list(&child.kind) || child.kind == Kind::CodeBlock;
            out.push_str(if tight { "\n" } else { "\n\n" });
        }
        out.push_str(&piece);
    }
    out
}

fn list(node: &Node, compat: bool) -> String {
    let start = node.attrs.get("start").and_then(Value::as_i64);
    let first = if compat {
        start.filter(|n| *n != 0).unwrap_or(1)
    } else {
        start.unwrap_or(1)
    };
    let mut lines = Vec::new();
    for (index, item) in node.content.iter().enumerate() {
        let marker = match &node.kind {
            Kind::OrderedList => format!("{}. ", first + i64::try_from(index).unwrap_or(0)),
            _ if item.kind == Kind::TaskItem => {
                if item
                    .attrs
                    .get("checked")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "- [x] ".to_owned()
                } else {
                    "- [ ] ".to_owned()
                }
            }
            _ => "- ".to_owned(),
        };
        let indent = if node.kind == Kind::OrderedList {
            marker.len()
        } else {
            2
        };
        let content = item_content(item, compat);
        let mut rendered = Vec::new();
        for (line_no, line) in content.split('\n').enumerate() {
            if line_no == 0 {
                rendered.push(format!("{marker}{line}"));
            } else if line.is_empty() {
                rendered.push(String::new());
            } else {
                rendered.push(format!("{}{line}", " ".repeat(indent)));
            }
        }
        lines.push(rendered.join("\n"));
    }
    lines.join("\n")
}

// --- Inline text ------------------------------------------------------------------------------

fn is_entity_at(chars: &[char], at: usize) -> bool {
    let rest: String = chars[at + 1..].iter().take(32).collect();
    let Some(end) = rest.find(';') else {
        return false;
    };
    let body = &rest[..end];
    if let Some(number) = body.strip_prefix('#') {
        return match number.strip_prefix(['x', 'X']) {
            Some(hex) => !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()),
            None => !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()),
        };
    }
    body.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && body.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Line start that would become a block marker: returns the index of the character to escape.
fn block_marker_at(line: &[char], last: bool) -> Option<usize> {
    let end_or_space = |at: usize| {
        line.get(at).is_none_or(|c| *c == ' ' || *c == '\t') && (line.get(at).is_some() || last)
    };
    let first = *line.first()?;
    match first {
        '#' => {
            let n = line.iter().take_while(|c| **c == '#').count();
            (n <= 6 && end_or_space(n)).then_some(0)
        }
        '>' => Some(0),
        '-' | '+' => {
            let dashes = line.iter().take_while(|c| **c == first).count();
            let only = line[dashes..].iter().all(|c| *c == ' ' || *c == '\t');
            (end_or_space(1) || (first == '-' && dashes >= 3 && only)).then_some(0)
        }
        '=' => {
            let n = line.iter().take_while(|c| **c == '=').count();
            line[n..].iter().all(|c| *c == ' ').then_some(0)
        }
        c if c.is_ascii_digit() => {
            let n = line.iter().take_while(|c| c.is_ascii_digit()).count();
            (n <= 9 && matches!(line.get(n), Some('.' | ')')) && end_or_space(n + 1)).then_some(n)
        }
        _ => None,
    }
}

/// Escapes non-code text. `bol` tells whether the text starts a line; `last` whether it ends the
/// block.
fn escape_text(text: &str, compat: bool, bol: bool, last: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut line_start = if compat { None } else { bol.then_some(0usize) };
    let mut escape_at = None;
    for (index, c) in chars.iter().enumerate() {
        if line_start == Some(index) {
            let line_end = chars[index..]
                .iter()
                .position(|c| *c == '\n')
                .map_or(chars.len(), |p| index + p);
            let is_last_line = line_end == chars.len() && last;
            escape_at =
                block_marker_at(&chars[index..line_end], is_last_line).map(|offset| index + offset);
            let spaces = chars[index..line_end]
                .iter()
                .take_while(|c| **c == ' ')
                .count();
            if spaces >= 4 {
                out.push_str("&#32;");
                line_start = None;
                continue;
            }
        }
        if escape_at == Some(index) {
            out.push('\\');
        }
        match c {
            '\n' => {
                out.push('\n');
                if !compat {
                    line_start = Some(index + 1);
                }
            }
            '\\' | '`' | '*' | '_' | '[' | ']' | '~' => {
                out.push('\\');
                out.push(*c);
            }
            '&' if compat => out.push_str("&amp;"),
            '<' if compat => out.push_str("&lt;"),
            '>' if compat => out.push_str("&gt;"),
            '<' => out.push_str("\\<"),
            '&' if is_entity_at(&chars, index) => out.push_str("&amp;"),
            other => out.push(*other),
        }
    }
    out
}

/// Rendered mark: identity key, nesting rank and delimiters.
struct Active {
    key: String,
    kind: MarkKind,
    close: String,
}

fn rank(kind: &MarkKind) -> Option<u8> {
    Some(match kind {
        MarkKind::Link => 0,
        MarkKind::Bold => 1,
        MarkKind::Italic => 2,
        MarkKind::Strike => 3,
        MarkKind::Underline => 4,
        MarkKind::Code => 5,
        MarkKind::Other(_) => return None,
    })
}

fn mark_key(mark: &Mark) -> String {
    format!("{}{}", mark.kind.name(), Value::Object(mark.attrs.clone()))
}

fn shortest_unused_backtick_run(text: &str) -> usize {
    let mut runs = Vec::new();
    let mut run = 0;
    for c in text.chars().chain(std::iter::once(' ')) {
        if c == '`' {
            run += 1;
        } else if run > 0 {
            runs.push(run);
            run = 0;
        }
    }
    (1..=runs.len() + 1)
        .find(|n| !runs.contains(n))
        .unwrap_or(1)
}

fn delimiters(mark: &Mark, compat: bool, html: bool, code_text: &str) -> (String, String) {
    let tags = |name: &str| (format!("<{name}>"), format!("</{name}>"));
    let same = |d: &str| (d.to_owned(), d.to_owned());
    match &mark.kind {
        MarkKind::Bold if html => tags("strong"),
        MarkKind::Bold => same("**"),
        MarkKind::Italic if html => tags("em"),
        MarkKind::Italic => same("*"),
        MarkKind::Strike if html => tags("s"),
        MarkKind::Strike => same("~~"),
        MarkKind::Underline if compat && !html => same("++"),
        MarkKind::Underline => tags("u"),
        MarkKind::Code if html => tags("code"),
        MarkKind::Code => {
            let fence = "`".repeat(shortest_unused_backtick_run(code_text));
            let pad = if code_text.starts_with('`') || code_text.ends_with('`') {
                " "
            } else {
                ""
            };
            (format!("{fence}{pad}"), format!("{pad}{fence}"))
        }
        MarkKind::Link => {
            let href = mark
                .attrs
                .get("href")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let title = mark
                .attrs
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let (href, title) = if compat {
                (href.to_owned(), title.to_owned())
            } else {
                (
                    link_destination(href),
                    title.replace('\\', "\\\\").replace('"', "\\\""),
                )
            };
            let close = if title.is_empty() {
                format!("]({href})")
            } else {
                format!("]({href} \"{title}\")")
            };
            ("[".to_owned(), close)
        }
        MarkKind::Other(_) => (String::new(), String::new()),
    }
}

/// Closes the active marks from `from` on; trailing whitespace stays outside the delimiters.
fn close_from(out: &mut String, active: &mut Vec<Active>, from: usize) {
    if active.len() <= from {
        return;
    }
    let closing: Vec<Active> = active.drain(from..).rev().collect();
    let keep_whitespace_inside = closing.iter().any(|mark| mark.kind == MarkKind::Code);
    let trimmed = out.trim_end_matches(char::is_whitespace).len();
    let trailing = if keep_whitespace_inside {
        String::new()
    } else {
        out.split_off(trimmed)
    };
    for mark in &closing {
        out.push_str(&mark.close);
    }
    out.push_str(&trailing);
}

/// Renders a sequence of inline nodes; the mark algorithm is kept in one piece for readability.
#[allow(clippy::too_many_lines)]
fn inline(nodes: &[Node], compat: bool) -> String {
    let supported = |node: &Node| -> Vec<Mark> {
        let mut seen: Vec<String> = Vec::new();
        node.marks
            .iter()
            .filter(|mark| rank(&mark.kind).is_some())
            .filter(|mark| {
                let key = mark_key(mark);
                let fresh = !seen.contains(&key);
                seen.push(key);
                fresh
            })
            .cloned()
            .collect()
    };
    let per_node: Vec<Vec<Mark>> = nodes
        .iter()
        .map(|node| {
            if node.kind == Kind::Text {
                supported(node)
            } else {
                Vec::new()
            }
        })
        .collect();
    let has = |at: usize, key: &str| {
        per_node
            .get(at)
            .is_some_and(|marks| marks.iter().any(|m| mark_key(m) == key))
    };
    let span = |from: usize, key: &str| (from..nodes.len()).take_while(|at| has(*at, key)).count();

    let mut out = String::new();
    let mut active: Vec<Active> = Vec::new();
    let mut bol = true;
    for (index, node) in nodes.iter().enumerate() {
        if node.kind != Kind::Text {
            close_from(&mut out, &mut active, 0);
            if node.kind == Kind::HardBreak {
                out.push_str(if compat { "  \n" } else { "\\\n" });
                bol = true;
            } else {
                out.push_str(&block(node, &[], 0, compat));
                bol = false;
            }
            continue;
        }
        let raw = node.text.as_deref().unwrap_or_default();
        let marks = &per_node[index];
        let in_code = marks.iter().any(|m| m.kind == MarkKind::Code);
        let mut desired: Vec<&Mark> = marks.iter().collect();
        if raw.trim().is_empty() && !in_code {
            desired.retain(|mark| active.iter().any(|a| a.key == mark_key(mark)));
        }
        let desired_keys: Vec<String> = desired.iter().map(|mark| mark_key(mark)).collect();
        let keep = active
            .iter()
            .take_while(|a| desired_keys.contains(&a.key))
            .count();
        let reopened: Vec<String> = active[keep..]
            .iter()
            .map(|a| a.key.clone())
            .filter(|key| desired_keys.contains(key))
            .collect();
        close_from(&mut out, &mut active, keep);
        let mut to_open: Vec<&Mark> = desired
            .iter()
            .copied()
            .filter(|mark| !active.iter().any(|a| a.key == mark_key(mark)))
            .collect();
        to_open.sort_by_key(|mark| {
            (
                std::cmp::Reverse(span(index, &mark_key(mark))),
                rank(&mark.kind),
            )
        });
        let escaped = if in_code {
            raw.to_owned()
        } else {
            escape_text(raw, compat, bol, index + 1 == nodes.len())
        };
        let mut text = escaped.as_str();
        if to_open.iter().any(|mark| mark.kind != MarkKind::Code) {
            let body = text.trim_start_matches(char::is_whitespace);
            out.push_str(&text[..text.len() - body.len()]);
            text = body;
        }
        for mark in to_open {
            let key = mark_key(mark);
            let code_text: String = (index..nodes.len())
                .take_while(|at| has(*at, &key))
                .filter_map(|at| nodes[at].text.as_deref())
                .collect();
            let html = reopened.contains(&key) && mark.kind != MarkKind::Link;
            let (open, close) = delimiters(mark, compat, html, &code_text);
            if open == "["
                && out.ends_with('!')
                && out.len() - out.trim_end_matches('\\').len() == 0
            {
                let backslashes = out[..out.len() - 1]
                    .chars()
                    .rev()
                    .take_while(|c| *c == '\\')
                    .count();
                if backslashes % 2 == 0 {
                    out.pop();
                    out.push_str("\\!");
                }
            }
            out.push_str(&open);
            active.push(Active {
                key,
                kind: mark.kind.clone(),
                close,
            });
        }
        out.push_str(text);
        bol = escaped.ends_with('\n');
    }
    close_from(&mut out, &mut active, 0);
    out
}
