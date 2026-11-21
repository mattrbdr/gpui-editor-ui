# gpui-editor-core

A headless rich-text block document model, with no dependency on a GUI toolkit:

- blocks and marks shaped like ProseMirror/TipTap (`type`, `attrs`, `content`, `marks`, `text`);
- a lossless JSON round trip: unknown nodes and marks, key order and empty `attrs` / `content`
  are kept, so a document read and written back is byte-identical;
- Markdown import and export (CommonMark + GFM, or a flavor identical to `@tiptap/markdown`);
- plain-text extraction;
- an editing engine (`Editor`): selection, typing, deletion by grapheme, word and block, Markdown
  input rules, headings, lists, tasks, quotes, code blocks, marks, links, mentions, undo/redo.

```rust
use gpui_editor_core::{Editor, Node, Pos, Selection};

let mut editor = Editor::new(Node::from_markdown("# Title\n\nHello"));
editor.set_selection(Selection::caret(Pos { block: 1, offset: 5 }));
// Markdown input rules apply as characters are typed.
for typed in " **world**".chars() {
    editor.insert_text(&typed.to_string());
}
assert_eq!(editor.doc().to_markdown(), "# Title\n\nHello **world**");
```

The GPUI component built on it is [`gpui-editor-ui`](https://crates.io/crates/gpui-editor-ui).
See the [repository](https://github.com/mattrbdr/gpui-editor-ui) for the full documentation.

License: MIT.
