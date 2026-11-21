//! Editor test window: `cargo run -p gpui-editor-ui --example spike [file.md]`.

use gpui::{App, Bounds, ImageSource, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_editor_core::Node;
use gpui_editor_ui::{EditorTheme, EditorView, bind_keys};

const SAMPLE: &str = "# Block editor\n\nType here: accented characters (é, è, ç), dead keys, **bold** (Ctrl+B), *italic* (Ctrl+I), `code` (Ctrl+E) and [links](https://example.com).\n\n## Lists\n\n- First item\n- Second item\n  - Nested (Tab / Shift+Tab)\n\n1. One\n2. Two\n\n- [ ] To do (Ctrl+Enter checks it)\n- [x] Done\n\n> A quote spanning\n> several lines.\n\n```js\nconst a = 1;\nconsole.log(a);\n```\n\n---\n\nEnd of document. Shortcuts: Ctrl+Alt+1/2/3 headings, Ctrl+Shift+8/7/9 lists, Ctrl+Z / Ctrl+Shift+Z undo and redo.";

fn main() {
    let file = std::env::args().nth(1).map(std::path::PathBuf::from);
    let source = file.as_ref().map_or_else(
        || SAMPLE.to_owned(),
        |path| std::fs::read_to_string(path).expect("unreadable file"),
    );
    // Relative image paths are read next to the Markdown file.
    let base = file
        .as_ref()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_default();
    gpui_platform::application().run(move |cx: &mut App| {
        bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(820.0), px(680.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    let doc = Node::from_markdown(&source);
                    cx.new(|cx| {
                        let mut theme = match std::env::var("SPIKE_THEME").as_deref() {
                            Ok("dark") => EditorTheme::dark(),
                            _ => EditorTheme::default(),
                        };
                        if let Ok(font) = std::env::var("SPIKE_FONT") {
                            theme.font_family = font.into();
                        }
                        let mut view = EditorView::new(doc, theme, cx);
                        let base = base.clone();
                        view.set_image_resolver(
                            move |src| {
                                let path = std::path::Path::new(src.trim_start_matches("file://"));
                                if src.contains("://") && !src.starts_with("file://") {
                                    return None;
                                }
                                Some(ImageSource::from(base.join(path)))
                            },
                            cx,
                        );
                        view
                    })
                },
            )
            .expect("failed to open window");
        window
            .update(cx, |view, window, cx| {
                view.focus(window, cx);
                cx.activate(true);
            })
            .ok();
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
