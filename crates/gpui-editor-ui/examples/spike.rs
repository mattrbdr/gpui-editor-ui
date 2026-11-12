//! Fenêtre d'essai de l'éditeur : `cargo run -p gpui-editor-ui --example spike [fichier.md]`.

use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_editor_core::Node;
use gpui_editor_ui::{EditorTheme, EditorView, bind_keys};

const SAMPLE: &str = "# Éditeur à blocs\n\nTapez ici : accents (é, è, ç), touches mortes, **gras** (Ctrl+B), *italique* (Ctrl+I), `code` (Ctrl+E) et [liens](https://example.com).\n\n## Listes\n\n- Premier élément\n- Deuxième élément\n  - Imbriqué (Tab / Maj+Tab)\n\n1. Un\n2. Deux\n\n- [ ] À faire (Ctrl+Entrée coche)\n- [x] Fait\n\n> Une citation sur\n> plusieurs lignes.\n\n```js\nconst a = 1;\nconsole.log(a);\n```\n\n---\n\nFin du document. Raccourcis : Ctrl+Alt+1/2/3 titres, Ctrl+Maj+8/7/9 listes, Ctrl+Z / Ctrl+Maj+Z annuler et rétablir.";

fn main() {
    let source = std::env::args().nth(1).map_or_else(
        || SAMPLE.to_owned(),
        |path| std::fs::read_to_string(path).expect("fichier illisible"),
    );
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
                        let mut theme = EditorTheme::default();
                        if let Ok(font) = std::env::var("SPIKE_FONT") {
                            theme.font_family = font.into();
                        }
                        EditorView::new(doc, theme, cx)
                    })
                },
            )
            .expect("fenêtre");
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
