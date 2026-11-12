//! Modèle de document à blocs, sans dépendance à une interface graphique.
//!
//! Le modèle reprend la forme de ProseMirror/TipTap (`type`, `attrs`, `content`, `marks`, `text`)
//! pour qu'un document JSON existant fasse l'aller-retour sans perte : les nœuds et marques
//! inconnus sont conservés tels quels ([`Kind::Other`], [`MarkKind::Other`]).
//!
//! Limites connues : seules les clés `type`, `attrs`, `content`, `marks` et `text` sont lues ; une
//! autre clé est ignorée à la lecture. Un `attrs` ou un `content` vide écrit explicitement dans la
//! source est restitué tel quel.

mod edit;
mod error;
mod markdown;
mod markdown_import;
mod model;
mod text;
mod tiptap;

pub use edit::{BlockCommand, Editor, Pos, Selection};
pub use error::Error;
pub use markdown::{Flavor, MarkdownOptions};
pub use model::{Kind, Mark, MarkKind, Node};
