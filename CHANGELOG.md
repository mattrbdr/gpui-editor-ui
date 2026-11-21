# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

First release of both crates.

### gpui-editor-core

- Block document model shaped like ProseMirror/TipTap, with a lossless JSON round trip (unknown
  nodes and marks, key order and empty `attrs` / `content` are kept).
- Markdown import and export: CommonMark + GFM, and a `TipTapCompat` flavor identical to
  `@tiptap/markdown`. Plain-text extraction.
- Editing engine `Editor`: selection, typing, deletion by grapheme, word and block, Markdown input
  rules, headings, lists, tasks, quotes, code blocks, marks, links, mentions, undo/redo.
- Undo snapshots share the top-level blocks that did not change; the text block index is cached.
- Property tests over random editing sequences.

### gpui-editor-ui

- `EditorView`, a GPUI component: rendering, caret, selection, mouse, keyboard, clipboard, IME,
  `/` block menu, `[[` mention menu fed by the host, links (Ctrl+K, Ctrl+click), embed cards,
  code block copy button, automatic scrolling.
- Image blocks loaded through `EditorView::set_image_resolver`.
- `EditorTheme::default()` and `EditorTheme::dark()`.
- Accessibility through AccessKit: multiline text input with its text, caret and selection; the
  menus are list boxes.
- `test-support` feature for the `*_for_tests` helpers.

[Unreleased]: https://github.com/mattrbdr/gpui-editor-ui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mattrbdr/gpui-editor-ui/releases/tag/v0.1.0
