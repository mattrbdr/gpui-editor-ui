//! Block-based document model, with no dependency on a GUI toolkit.
//!
//! The model mirrors the ProseMirror/TipTap shape (`type`, `attrs`, `content`, `marks`, `text`)
//! so that an existing JSON document round-trips losslessly: unknown nodes and marks are kept
//! as is ([`Kind::Other`], [`MarkKind::Other`]).
//!
//! Known limitations: only the `type`, `attrs`, `content`, `marks` and `text` keys are read; any
//! other key is ignored when parsing. An empty `attrs` or `content` written explicitly in the
//! source is preserved as is.

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
