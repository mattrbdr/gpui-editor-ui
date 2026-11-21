use std::fmt;

/// Failure to parse a JSON document.
#[derive(Debug)]
pub enum Error {
    /// The text is not valid JSON.
    Json(serde_json::Error),
    /// A node or mark is not a JSON object.
    NotAnObject,
    /// The `type` key is missing or is not a string.
    MissingType,
    /// A `text` node has no string `text` key.
    MissingText,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid JSON: {error}"),
            Self::NotAnObject => f.write_str("a node or mark must be a JSON object"),
            Self::MissingType => f.write_str("the `type` key is missing or is not a string"),
            Self::MissingText => f.write_str("a `text` node must have a string `text`"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
