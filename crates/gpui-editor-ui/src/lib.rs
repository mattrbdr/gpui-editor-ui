//! Block-based rich text editor for GPUI.
//!
//! [`EditorView`] displays a `gpui_editor_core` document and handles the caret, selection,
//! mouse, keyboard, clipboard and IME input (dead keys, compositions). The application reads
//! the content through [`EditorView::doc`] and [`EditorEvent`] events.
//!
//! ```no_run
//! use gpui::AppContext as _;
//! use gpui_editor_core::Node;
//! use gpui_editor_ui::{EditorEvent, EditorTheme, EditorView};
//!
//! # fn demo(cx: &mut gpui::App) {
//! gpui_editor_ui::bind_keys(cx);
//! let doc = Node::from_markdown("# Title");
//! let view = cx.new(|cx| EditorView::new(doc, EditorTheme::default(), cx));
//! cx.subscribe(&view, |view, event: &EditorEvent, cx| {
//!     if *event == EditorEvent::Changed {
//!         let _json = view.read(cx).doc().to_json_string();
//!     }
//! })
//! .detach();
//! # }
//! ```

#![warn(missing_docs)]

mod a11y;
mod keys;
mod render;
mod view;
mod visible;

pub use keys::bind_keys;
pub use view::{
    EditorEvent, EditorLabels, EditorTheme, EditorView, EmbedCard, ImageResolver, MentionItem,
};
