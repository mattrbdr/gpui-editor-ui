//! Lecture et écriture du JSON ProseMirror/TipTap.

use serde_json::{Map, Value};

use crate::{
    Error,
    model::{Key, Kind, Mark, MarkKind, Node, Shape},
};

fn object(value: &Value) -> Result<&Map<String, Value>, Error> {
    value.as_object().ok_or(Error::NotAnObject)
}

fn name_of(map: &Map<String, Value>) -> Result<&str, Error> {
    map.get("type")
        .and_then(Value::as_str)
        .ok_or(Error::MissingType)
}

/// Ordre des clés connues de la source, `None` s'il est déjà canonique.
fn order_of(map: &Map<String, Value>) -> Option<Vec<Key>> {
    let found: Vec<Key> = map.keys().filter_map(|name| Key::from_name(name)).collect();
    let canonical: Vec<Key> = Key::CANONICAL
        .into_iter()
        .filter(|key| found.contains(key))
        .collect();
    (found != canonical).then_some(found)
}

fn attrs_of(map: &Map<String, Value>) -> (Map<String, Value>, bool) {
    match map.get("attrs") {
        Some(Value::Object(attrs)) => (attrs.clone(), attrs.is_empty()),
        _ => (Map::new(), false),
    }
}

fn mark_from(value: &Value) -> Result<Mark, Error> {
    let map = object(value)?;
    let kind = MarkKind::from_name(name_of(map)?);
    let (attrs, empty_attrs) = attrs_of(map);
    Ok(Mark {
        kind,
        attrs,
        shape: Shape {
            attrs: empty_attrs,
            order: order_of(map),
            ..Shape::default()
        },
    })
}

/// Clés à écrire, dans l'ordre de la source puis dans l'ordre canonique pour les clés ajoutées.
fn keys_to_write(shape: &Shape, present: impl Fn(Key) -> bool) -> Vec<Key> {
    let mut keys: Vec<Key> = shape
        .order
        .iter()
        .flatten()
        .copied()
        .filter(|key| present(*key))
        .collect();
    for key in Key::CANONICAL {
        if present(key) && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

fn mark_to_value(mark: &Mark) -> Value {
    let present = |key| match key {
        Key::Type => true,
        Key::Attrs => !mark.attrs.is_empty() || mark.shape.attrs,
        _ => false,
    };
    let mut map = Map::new();
    for key in keys_to_write(&mark.shape, present) {
        let value = match key {
            Key::Type => Value::String(mark.kind.name().to_owned()),
            _ => Value::Object(mark.attrs.clone()),
        };
        map.insert(key.name().to_owned(), value);
    }
    Value::Object(map)
}

fn list_of<T>(
    map: &Map<String, Value>,
    key: &str,
    read: impl Fn(&Value) -> Result<T, Error>,
) -> Result<(Vec<T>, bool), Error> {
    match map.get(key) {
        Some(Value::Array(items)) => Ok((
            items.iter().map(read).collect::<Result<Vec<_>, _>>()?,
            items.is_empty(),
        )),
        _ => Ok((Vec::new(), false)),
    }
}

impl Node {
    /// Lit un nœud depuis une valeur JSON.
    ///
    /// # Errors
    ///
    /// [`Error::NotAnObject`], [`Error::MissingType`] ou [`Error::MissingText`] selon le défaut.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let map = object(value)?;
        let kind = Kind::from_name(name_of(map)?);
        let (attrs, empty_attrs) = attrs_of(map);
        let (content, empty_content) = list_of(map, "content", Self::from_json)?;
        let (marks, empty_marks) = list_of(map, "marks", mark_from)?;
        let text = map.get("text").and_then(Value::as_str).map(str::to_owned);
        if kind == Kind::Text && text.is_none() {
            return Err(Error::MissingText);
        }
        Ok(Self {
            kind,
            attrs,
            content,
            marks,
            text,
            shape: Shape {
                attrs: empty_attrs,
                content: empty_content,
                marks: empty_marks,
                order: order_of(map),
            },
        })
    }

    /// Lit un nœud depuis du texte JSON.
    ///
    /// # Errors
    ///
    /// [`Error::Json`] si le texte n'est pas du JSON, sinon les erreurs de [`Node::from_json`].
    pub fn from_json_str(source: &str) -> Result<Self, Error> {
        Self::from_json(&serde_json::from_str(source)?)
    }

    /// Valeur JSON. L'ordre des clés de la source est conservé ; les clés ajoutées ensuite suivent
    /// l'ordre canonique de `ProseMirror` (`type`, `attrs`, `content`, `marks`, `text`).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let present = |key| match key {
            Key::Type => true,
            Key::Attrs => !self.attrs.is_empty() || self.shape.attrs,
            Key::Content => !self.content.is_empty() || self.shape.content,
            Key::Marks => !self.marks.is_empty() || self.shape.marks,
            Key::Text => self.text.is_some(),
        };
        let mut map = Map::new();
        for key in keys_to_write(&self.shape, present) {
            let value = match key {
                Key::Type => Value::String(self.kind.name().to_owned()),
                Key::Attrs => Value::Object(self.attrs.clone()),
                Key::Content => Value::Array(self.content.iter().map(Self::to_json).collect()),
                Key::Marks => Value::Array(self.marks.iter().map(mark_to_value).collect()),
                Key::Text => Value::String(self.text.clone().unwrap_or_default()),
            };
            map.insert(key.name().to_owned(), value);
        }
        Value::Object(map)
    }

    /// Texte JSON compact, comme `JSON.stringify`.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        self.to_json().to_string()
    }
}
