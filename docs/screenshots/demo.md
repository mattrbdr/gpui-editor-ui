# Release notes

The editor keeps **bold**, *italic*, `inline code`, ~~strike~~ and [links](https://www.gpui.rs/) while you type.

## What changed

- A native block editor for GPUI
- Lossless ProseMirror/TipTap JSON
  - Unknown nodes are kept as they are
- Markdown import and export

1. Write
2. Review

- [x] Ship the core
- [ ] Publish on crates.io

> Typing `# `, `- ` or `> ` at the start of a block converts it.

```rust
let doc = Node::from_markdown("# Title");
```

---

Type `/` for the block menu.
