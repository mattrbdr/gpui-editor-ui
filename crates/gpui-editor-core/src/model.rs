use serde_json::{Map, Value};

/// Type d'un nœud.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Kind {
    Doc,
    Paragraph,
    Heading,
    CodeBlock,
    Blockquote,
    BulletList,
    OrderedList,
    TaskList,
    ListItem,
    TaskItem,
    HorizontalRule,
    HardBreak,
    Text,
    /// Type non reconnu, conservé par son nom.
    Other(String),
}

impl Kind {
    /// Type correspondant à un nom ProseMirror (`"heading"`, `"codeBlock"`…).
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

    /// Nom ProseMirror du type.
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

    /// Bloc dont le contenu est du texte en ligne.
    #[must_use]
    pub fn is_textblock(&self) -> bool {
        matches!(self, Self::Paragraph | Self::Heading | Self::CodeBlock)
    }

    /// Nœud sans enfants par construction.
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::HorizontalRule | Self::HardBreak | Self::Text)
    }
}

/// Type d'une marque en ligne.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MarkKind {
    Bold,
    Italic,
    Strike,
    Underline,
    Code,
    Link,
    /// Marque non reconnue, conservée par son nom.
    Other(String),
}

impl MarkKind {
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

/// Marque appliquée à un texte (gras, lien…).
#[derive(Debug, Clone)]
pub struct Mark {
    pub kind: MarkKind,
    pub attrs: Map<String, Value>,
    pub(crate) shape: Shape,
}

impl PartialEq for Mark {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.attrs == other.attrs
    }
}

impl Mark {
    #[must_use]
    pub fn new(kind: MarkKind) -> Self {
        Self {
            kind,
            attrs: Map::new(),
            shape: Shape::default(),
        }
    }
}

/// Clé d'un nœud ou d'une marque JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Type,
    Attrs,
    Content,
    Marks,
    Text,
}

impl Key {
    /// Ordre de ProseMirror : `type`, `attrs`, `content`, `marks`, `text`.
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

/// Particularités de la source à restituer à l'identique : clés vides et ordre des clés.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shape {
    pub attrs: bool,
    pub content: bool,
    pub marks: bool,
    /// Ordre des clés de la source, seulement s'il diffère de l'ordre canonique.
    pub order: Option<Vec<Key>>,
}

/// Nœud du document (même forme que ProseMirror).
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub attrs: Map<String, Value>,
    pub content: Vec<Node>,
    pub marks: Vec<Mark>,
    /// Texte d'un nœud [`Kind::Text`].
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
    /// Nœud vide du type donné.
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

    /// Nœud texte sans marque.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            text: Some(value.into()),
            ..Self::new(Kind::Text)
        }
    }

    /// Parcourt le nœud puis ses descendants, dans l'ordre du document.
    pub fn walk(&self, visit: &mut impl FnMut(&Node)) {
        visit(self);
        for child in &self.content {
            child.walk(visit);
        }
    }
}
