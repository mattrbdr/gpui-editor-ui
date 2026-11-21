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
- Property tests over random editing sequences and over arbitrary Markdown and JSON input;
  they found and fixed several Markdown round trips that were not stable (heading closing
  sequences, empty nested quotes, code span spaces, image alt text, code block languages,
  nested lists after a paragraph) and loose task items imported as plain bullets.
- Criterion benchmarks (`cargo bench -p gpui-editor-core`) and complexity guards run in CI.
- A keystroke while typing no longer rebuilds the text block index (O(block) instead of
  O(document)).
- `Kind`, `MarkKind`, `Error`, `Flavor` and `BlockCommand` are `#[non_exhaustive]`, so that
  later versions can add variants; match them with a `_` arm.

### gpui-editor-ui

- `EditorView`, a GPUI component: rendering, caret, selection, mouse, keyboard, clipboard, IME,
  `/` block menu, `[[` mention menu fed by the host, links (Ctrl+K, Ctrl+click), embed cards,
  code block copy button, automatic scrolling.
- Image blocks loaded through `EditorView::set_image_resolver`.
- `EditorTheme::default()` and `EditorTheme::dark()`.
- Accessibility through AccessKit: multiline text input with its text, caret and selection; the
  menus are list boxes.
- `test-support` feature for the `*_for_tests` helpers.
- `EditorEvent` is `#[non_exhaustive]`. Structs that applications build (`EditorTheme`,
  `MentionItem`, `EmbedCard`) keep public fields; adding a field to them will be a breaking change.
- Interaction timing test with a complexity guard (`cargo test -p gpui-editor-ui --release --test
  perf`).

[Unreleased]: https://github.com/mattrbdr/gpui-editor-ui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mattrbdr/gpui-editor-ui/releases/tag/v0.1.0
