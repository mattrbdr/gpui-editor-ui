//! Block-based rich text editor for GPUI.
//!
//! [`EditorView`] displays a `gpui_editor_core` document and handles the caret, selection,
//! mouse, keyboard, clipboard and IME input (dead keys, compositions). The application reads
//! the content through [`EditorView::doc`] and [`EditorEvent`] events.
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
