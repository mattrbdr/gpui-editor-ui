use std::fmt;

/// Échec de lecture d'un document JSON.
#[derive(Debug)]
pub enum Error {
    /// Le texte n'est pas du JSON valide.
    Json(serde_json::Error),
    /// Un nœud ou une marque n'est pas un objet JSON.
    NotAnObject,
    /// La clé `type` est absente ou n'est pas une chaîne.
    MissingType,
    /// Un nœud `text` n'a pas de clé `text` de type chaîne.
    MissingText,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "JSON invalide : {error}"),
            Self::NotAnObject => f.write_str("un nœud ou une marque doit être un objet JSON"),
            Self::MissingType => f.write_str("la clé `type` est absente ou n'est pas une chaîne"),
            Self::MissingText => f.write_str("un nœud `text` doit porter une chaîne `text`"),
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
