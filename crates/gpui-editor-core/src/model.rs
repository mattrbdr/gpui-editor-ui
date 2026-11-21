use serde_json::{Map, Value};

/// Node type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// `doc`: the root node.
    Doc,
    /// `paragraph`: text block.
    Paragraph,
    /// `heading`: text block whose level is in the `level` attribute.
    Heading,
    /// `codeBlock`: text block of unformatted text; language in the `language` attribute.
    CodeBlock,
    /// `blockquote`: container of blocks.
    Blockquote,
    /// `bulletList`: list of [`Kind::ListItem`].
    BulletList,
    /// `orderedList`: numbered list of [`Kind::ListItem`].
    OrderedList,
    /// `taskList`: list of [`Kind::TaskItem`].
    TaskList,
    /// `listItem`: item of a bullet or ordered list.
    ListItem,
    /// `taskItem`: item of a task list; state in the `checked` attribute.
    TaskItem,
    /// `horizontalRule`: thematic break (leaf block).
    HorizontalRule,
    /// `hardBreak`: line break inside a text block (inline leaf).
    HardBreak,
    /// `text`: inline text, in [`Node::text`], with its [`Node::marks`].
    Text,
    /// Unrecognized type, kept by name.
    Other(String),
}

impl Kind {
    /// Type matching a ProseMirror name (`"heading"`, `"codeBlock"`…).
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "doc" => Self::Doc,
            "paragraph" => Self::Paragraph,
            "heading" => Self::Heading,
            "codeBlock" => Self::CodeBlock,
            "blockquote" => Self::Blockquote,
            "bulletList" => Self::BulletList,
            "orderedList" => Self::OrderedList,
            "taskList" => Self::TaskList,
            "listItem" => Self::ListItem,
            "taskItem" => Self::TaskItem,
            "horizontalRule" => Self::HorizontalRule,
            "hardBreak" => Self::HardBreak,
            "text" => Self::Text,
            other => Self::Other(other.to_owned()),
        }
    }

    /// ProseMirror name of the type.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Doc => "doc",
            Self::Paragraph => "paragraph",
            Self::Heading => "heading",
            Self::CodeBlock => "codeBlock",
            Self::Blockquote => "blockquote",
            Self::BulletList => "bulletList",
            Self::OrderedList => "orderedList",
            Self::TaskList => "taskList",
            Self::ListItem => "listItem",
            Self::TaskItem => "taskItem",
            Self::HorizontalRule => "horizontalRule",
            Self::HardBreak => "hardBreak",
            Self::Text => "text",
            Self::Other(name) => name,
        }
    }

    /// Block whose content is inline text.
    #[must_use]
    pub fn is_textblock(&self) -> bool {
        matches!(self, Self::Paragraph | Self::Heading | Self::CodeBlock)
    }

    /// Node that has no children by construction.
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::HorizontalRule | Self::HardBreak | Self::Text)
    }
}

/// Inline mark type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MarkKind {
    /// `bold`.
    Bold,
    /// `italic`.
    Italic,
    /// `strike`: strikethrough.
    Strike,
    /// `underline`.
    Underline,
    /// `code`: inline code.
    Code,
    /// `link`: URL in the `href` attribute.
    Link,
    /// Unrecognized mark, kept by name.
    Other(String),
}

impl MarkKind {
    /// Mark type matching a ProseMirror name (`"bold"`, `"link"`…).
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "bold" => Self::Bold,
            "italic" => Self::Italic,
            "strike" => Self::Strike,
            "underline" => Self::Underline,
            "code" => Self::Code,
            "link" => Self::Link,
            other => Self::Other(other.to_owned()),
        }
    }

    /// ProseMirror name of the mark.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Strike => "strike",
            Self::Underline => "underline",
            Self::Code => "code",
            Self::Link => "link",
            Self::Other(name) => name,
        }
    }
}

/// Mark applied to text (bold, link…).
#[derive(Debug, Clone)]
pub struct Mark {
    /// Mark type.
    pub kind: MarkKind,
    /// Attributes (`attrs`), for example `href` for a link; empty if there are none.
    pub attrs: Map<String, Value>,
    pub(crate) shape: Shape,
}

impl PartialEq for Mark {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.attrs == other.attrs
    }
}

impl Mark {
    /// Mark of the given type, without attributes.
    #[must_use]
    pub fn new(kind: MarkKind) -> Self {
        Self {
            kind,
            attrs: Map::new(),
            shape: Shape::default(),
        }
    }
}

/// Key of a JSON node or mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Type,
    Attrs,
    Content,
    Marks,
    Text,
}

impl Key {
    /// ProseMirror order: `type`, `attrs`, `content`, `marks`, `text`.
    pub(crate) const CANONICAL: [Self; 5] = [
        Self::Type,
        Self::Attrs,
        Self::Content,
        Self::Marks,
        Self::Text,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Attrs => "attrs",
            Self::Content => "content",
            Self::Marks => "marks",
            Self::Text => "text",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::CANONICAL.into_iter().find(|key| key.name() == name)
    }
}

/// Source quirks to reproduce exactly: empty keys and key order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shape {
    pub attrs: bool,
    pub content: bool,
    pub marks: bool,
    /// Key order of the source, only if it differs from the canonical order.
    pub order: Option<Vec<Key>>,
}

/// Document node (same shape as ProseMirror).
#[derive(Debug, Clone)]
pub struct Node {
    /// Node type.
    pub kind: Kind,
    /// Attributes (`attrs`); empty if there are none.
    pub attrs: Map<String, Value>,
    /// Child nodes (`content`), in order.
    pub content: Vec<Node>,
    /// Marks of an inline node (`marks`), usually a [`Kind::Text`] node.
    pub marks: Vec<Mark>,
    /// Text of a [`Kind::Text`] node.
    pub text: Option<String>,
    pub(crate) shape: Shape,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.attrs == other.attrs
            && self.content == other.content
            && self.marks == other.marks
            && self.text == other.text
    }
}

impl Node {
    /// Empty node of the given type.
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            attrs: Map::new(),
            content: Vec::new(),
            marks: Vec::new(),
            text: None,
            shape: Shape::default(),
        }
    }

    /// Text node without marks.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            text: Some(value.into()),
            ..Self::new(Kind::Text)
        }
    }

    /// Visits the node and then its descendants, in document order.
    pub fn walk(&self, visit: &mut impl FnMut(&Node)) {
        visit(self);
        for child in &self.content {
            child.walk(visit);
        }
    }
}
