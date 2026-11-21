# gpui-editor-ui

A native rich-text block editor component for [GPUI](https://www.gpui.rs/), built on
[`gpui-editor-core`](https://crates.io/crates/gpui-editor-core).

`EditorView` renders a ProseMirror/TipTap-compatible document and handles the caret, selection,
mouse, keyboard, clipboard and IME; Markdown shortcuts while typing; a `/` block menu and a `[[`
mention menu fed by the host application; links; image blocks loaded through a host resolver;
light and dark themes; and accessibility through AccessKit.

```rust,ignore
gpui_editor_ui::bind_keys(cx);
let view = cx.new(|cx| EditorView::new(Node::from_markdown("# Title"), EditorTheme::default(), cx));
cx.subscribe(&view, |_, _, event: &EditorEvent, _| {
    if let EditorEvent::Changed = event { /* save view.doc().to_json() */ }
})
.detach();
```

It targets `gpui-pre =0.3.4`, the GPUI build published on crates.io. On Linux it needs GPUI's
system libraries (Vulkan, X11/Wayland, xkbcommon, fontconfig).

Screenshots and the full documentation are in the
[repository](https://github.com/mattrbdr/gpui-editor-ui).

License: MIT.
