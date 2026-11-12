//! Éditeur de texte riche à blocs pour GPUI.
//!
//! [`EditorView`] affiche un document de `gpui_editor_core`, gère le curseur, la sélection, la
//! souris, le clavier, le presse-papiers et la saisie IME (touches mortes, compositions). Le
//! contenu est lu par l'application via [`EditorView::doc`] et les événements [`EditorEvent`].
//!
//! ```ignore
//! gpui_editor_ui::bind_keys(cx);
//! let view = cx.new(|cx| EditorView::new(doc, EditorTheme::default(), cx));
//! ```

mod keys;
mod render;
mod view;

pub use keys::bind_keys;
pub use view::{EditorEvent, EditorTheme, EditorView, EmbedCard, MentionItem};
