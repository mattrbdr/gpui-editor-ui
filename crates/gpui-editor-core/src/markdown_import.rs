//! Markdown import (CommonMark + GFM strikethrough and task lists) into the document model.
//!
//! The resulting structure matches `@tiptap/markdown`: marks written from the innermost to the
//! outermost, `text` before `marks`, `title: null` on links, a lone image promoted to a block.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{Map, Value};

use crate::{Flavor, Kind, Mark, MarkKind, MarkdownOptions, Node, model::Key};

impl Node {
    /// Document from CommonMark Markdown.
    #[must_use]
    pub fn from_markdown(source: &str) -> Self {
        Self::from_markdown_with(source, &MarkdownOptions::default())
    }

    /// Like [`Node::from_markdown`]; the `TipTapCompat` flavor also reads `++underlined++`.
    #[must_use]
    pub fn from_markdown_with(source: &str, options: &MarkdownOptions) -> Self {
        let compat = options.flavor == Flavor::TipTapCompat;
        let mut flags = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        if compat {
            // Without a table extension, @tiptap/markdown drops tables.
            flags |= Options::ENABLE_TABLES;
        }
        let mut builder = Builder::new();
        let mut table_depth = 0usize;
        for event in Parser::new_ext(source, flags) {
            match &event {
                Event::Start(Tag::Table(_)) if compat => table_depth += 1,
                Event::End(TagEnd::Table) if compat => {
                    table_depth = table_depth.saturating_sub(1);
                    continue;
                }
                _ => {}
            }
            if table_depth == 0 {
                builder.event(event);
            }
        }
        let mut doc = builder.finish();
        if compat {
            underline_pass(&mut doc);
            split_mixed_lists(&mut doc);
        }
        doc
    }
}

struct Image {
    src: String,
    title: Option<String>,
    alt: String,
}

struct Builder {
    stack: Vec<Node>,
    marks: Vec<Mark>,
    code: Option<String>,
    html: Option<String>,
    image: Option<Image>,
    implicit_paragraph: bool,
}

fn node(kind: Kind) -> Node {
    Node::new(kind)
}

fn with_attr(mut node: Node, key: &str, value: Value) -> Node {
    node.attrs.insert(key.to_owned(), value);
    node
}

fn text_node(text: String, marks: Vec<Mark>) -> Node {
    let mut node = Node::text(text);
    if !marks.is_empty() {
        node.marks = marks;
        node.shape.order = Some(vec![Key::Type, Key::Text, Key::Marks]);
    }
    node
}

fn mark_with(kind: MarkKind, attrs: Vec<(&str, Value)>) -> Mark {
    let mut mark = Mark::new(kind);
    for (key, value) in attrs {
        mark.attrs.insert(key.to_owned(), value);
    }
    mark
}

impl Builder {
    fn new() -> Self {
        Self {
            stack: vec![node(Kind::Doc)],
            marks: Vec::new(),
            code: None,
            html: None,
            image: None,
            implicit_paragraph: false,
        }
    }

    fn finish(mut self) -> Node {
        self.close_implicit();
        while self.stack.len() > 1 {
            self.pop();
        }
        let mut doc = self.stack.pop().unwrap_or_else(|| node(Kind::Doc));
        if doc.content.is_empty() {
            doc.shape.content = true;
        }
        doc
    }

    fn top(&mut self) -> &mut Node {
        self.stack
            .last_mut()
            .expect("the stack always contains the document")
    }

    fn push(&mut self, new: Node) {
        self.close_implicit();
        self.stack.push(new);
    }

    /// Closes the current node and appends it to its parent.
    fn pop(&mut self) {
        if self.stack.len() > 1
            && let Some(done) = self.stack.pop()
        {
            self.top().content.push(done);
        }
    }

    fn add(&mut self, child: Node) {
        self.close_implicit();
        self.top().content.push(child);
    }

    fn close_implicit(&mut self) {
        if self.implicit_paragraph && self.stack.last().is_some_and(|n| n.kind == Kind::Paragraph) {
            self.implicit_paragraph = false;
            self.pop();
        }
    }

    /// "Tight" list items hold text without a paragraph, so open one.
    fn ensure_inline_container(&mut self) {
        if self
            .stack
            .last()
            .is_some_and(|n| matches!(n.kind, Kind::ListItem | Kind::TaskItem))
        {
            self.stack.push(node(Kind::Paragraph));
            self.implicit_paragraph = true;
        }
    }

    fn marks_inner_first(&self) -> Vec<Mark> {
        self.marks.iter().rev().cloned().collect()
    }

    fn add_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_inline_container();
        let marks = self.marks_inner_first();
        if let Some(last) = self.top().content.last_mut()
            && last.kind == Kind::Text
            && last.marks == marks
        {
            last.text.get_or_insert_with(String::new).push_str(text);
            return;
        }
        self.top().content.push(text_node(text.to_owned(), marks));
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if let Some(code) = &mut self.code {
                    code.push_str(&text);
                } else if let Some(image) = &mut self.image {
                    image.alt.push_str(&text);
                } else {
                    self.add_text(&text);
                }
            }
            Event::Code(text) => {
                if let Some(image) = &mut self.image {
                    image.alt.push_str(&text);
                    return;
                }
                self.ensure_inline_container();
                let mut marks = vec![Mark::new(MarkKind::Code)];
                marks.extend(self.marks_inner_first());
                self.top()
                    .content
                    .push(text_node(text.into_string(), marks));
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                if let Some(html) = &mut self.html {
                    html.push_str(&text);
                } else {
                    self.add_text(&text);
                }
            }
            Event::SoftBreak => self.add_text("\n"),
            Event::HardBreak => {
                self.ensure_inline_container();
                self.top().content.push(node(Kind::HardBreak));
            }
            Event::Rule => self.add(node(Kind::HorizontalRule)),
            Event::TaskListMarker(checked) => self.task_item(checked),
            _ => {}
        }
    }

    fn task_item(&mut self, checked: bool) {
        let len = self.stack.len();
        if len >= 2 && self.stack[len - 1].kind == Kind::ListItem {
            self.stack[len - 1].kind = Kind::TaskItem;
            self.stack[len - 1]
                .attrs
                .insert("checked".into(), Value::Bool(checked));
            if self.stack[len - 2].kind == Kind::BulletList {
                self.stack[len - 2].kind = Kind::TaskList;
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.push(node(Kind::Paragraph)),
            Tag::Heading { level, .. } => {
                let number = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.push(with_attr(node(Kind::Heading), "level", Value::from(number)));
            }
            Tag::BlockQuote(_) => self.push(node(Kind::Blockquote)),
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(str::to_owned)
                    }
                    CodeBlockKind::Indented => None,
                };
                let value = language.map_or(Value::Null, Value::String);
                self.push(with_attr(node(Kind::CodeBlock), "language", value));
                self.code = Some(String::new());
            }
            Tag::HtmlBlock => {
                self.push(node(Kind::Paragraph));
                self.html = Some(String::new());
            }
            Tag::List(first) => match first {
                Some(start) => {
                    let mut list = node(Kind::OrderedList);
                    if start != 1 {
                        list.attrs.insert("start".into(), Value::from(start));
                    }
                    self.push(list);
                }
                None => self.push(node(Kind::BulletList)),
            },
            Tag::Item => self.push(node(Kind::ListItem)),
            Tag::Emphasis => self.marks.push(Mark::new(MarkKind::Italic)),
            Tag::Strong => self.marks.push(Mark::new(MarkKind::Bold)),
            Tag::Strikethrough => self.marks.push(Mark::new(MarkKind::Strike)),
            Tag::Link {
                dest_url, title, ..
            } => {
                let title = if title.is_empty() {
                    Value::Null
                } else {
                    Value::String(title.into_string())
                };
                self.marks.push(mark_with(
                    MarkKind::Link,
                    vec![
                        ("href", Value::String(dest_url.into_string())),
                        ("title", title),
                    ],
                ));
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                self.image = Some(Image {
                    src: dest_url.into_string(),
                    title: (!title.is_empty()).then(|| title.into_string()),
                    alt: String::new(),
                });
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.promote_lone_image();
                self.pop();
            }
            TagEnd::Heading(_) | TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item => {
                self.close_implicit();
                self.pop();
            }
            TagEnd::CodeBlock => {
                let text = self.code.take().unwrap_or_default();
                let text = text.strip_suffix('\n').unwrap_or(&text);
                let block = self.top();
                if text.is_empty() {
                    block.shape.content = true;
                } else {
                    block.content.push(Node::text(text));
                }
                self.pop();
            }
            TagEnd::HtmlBlock => {
                let html = self.html.take().unwrap_or_default();
                self.add_text(html.trim_end_matches('\n'));
                self.pop();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.marks.pop();
            }
            TagEnd::Image => {
                if let Some(image) = self.image.take() {
                    self.ensure_inline_container();
                    let mut attrs = Map::new();
                    attrs.insert("src".into(), Value::String(image.src));
                    attrs.insert(
                        "title".into(),
                        image.title.map_or(Value::Null, Value::String),
                    );
                    attrs.insert("alt".into(), Value::String(image.alt));
                    let mut node = Node::new(Kind::Other("image".into()));
                    node.attrs = attrs;
                    self.top().content.push(node);
                }
            }
            _ => {}
        }
    }

    /// A paragraph containing only an image becomes an image block (like TipTap).
    fn promote_lone_image(&mut self) {
        let top = self.top();
        if top.kind == Kind::Paragraph
            && top.content.len() == 1
            && top.content[0].kind == Kind::Other("image".into())
        {
            let image = top.content.remove(0);
            *top = image;
        }
    }
}

/// A list mixing bullets and tasks becomes a sequence of homogeneous lists (like TipTap).
fn split_mixed_lists(node: &mut Node) {
    let children = std::mem::take(&mut node.content);
    let mut out: Vec<Node> = Vec::with_capacity(children.len());
    for mut child in children {
        split_mixed_lists(&mut child);
        let mixed = matches!(child.kind, Kind::BulletList | Kind::TaskList)
            && child.content.iter().any(|i| i.kind == Kind::TaskItem)
            && child.content.iter().any(|i| i.kind == Kind::ListItem);
        if !mixed {
            out.push(child);
            continue;
        }
        let mut run: Option<Node> = None;
        for item in std::mem::take(&mut child.content) {
            let kind = if item.kind == Kind::TaskItem {
                Kind::TaskList
            } else {
                Kind::BulletList
            };
            if run.as_ref().is_some_and(|r| r.kind != kind) {
                out.extend(run.take());
            }
            run.get_or_insert_with(|| node_of(kind)).content.push(item);
        }
        out.extend(run);
    }
    node.content = out;
}

fn node_of(kind: Kind) -> Node {
    Node::new(kind)
}

/// Reads `++text++` in text nodes (TipTap-compatible flavor).
fn underline_pass(node: &mut Node) {
    let children = std::mem::take(&mut node.content);
    let mut out = Vec::with_capacity(children.len());
    for mut child in children {
        underline_pass(&mut child);
        let plain =
            child.kind == Kind::Text && !child.marks.iter().any(|m| m.kind == MarkKind::Code);
        match child.text.as_deref().filter(|t| plain && t.contains("++")) {
            Some(text) => out.extend(split_underline(text, &child.marks)),
            None => out.push(child),
        }
    }
    node.content = out;
}

fn split_underline(text: &str, marks: &[Mark]) -> Vec<Node> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("++") {
        let after = &rest[start + 2..];
        let Some(len) = after.find("++").filter(|len| *len > 0) else {
            break;
        };
        if start > 0 {
            parts.push(text_node(rest[..start].to_owned(), marks.to_vec()));
        }
        let mut underlined = marks.to_vec();
        underlined.push(Mark::new(MarkKind::Underline));
        parts.push(text_node(after[..len].to_owned(), underlined));
        rest = &after[len + 2..];
    }
    if !rest.is_empty() {
        parts.push(text_node(rest.to_owned(), marks.to_vec()));
    }
    parts
}
