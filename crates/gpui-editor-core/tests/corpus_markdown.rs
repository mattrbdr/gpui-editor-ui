//! Compares the TipTap-compatible Markdown export with real exports, without ever committing them.
//!
//! `GPUI_EDITOR_MARKDOWN_CORPUS`: a file with one JSON object per line,
//! `{"doc": …, "markdown": "…"}`.
//! Without this variable, the test does nothing.

use gpui_editor_core::{Flavor, MarkdownOptions, Node};
use serde_json::Value;

#[test]
fn compat_export_matches_real_exports() {
    let Some(path) = std::env::var_os("GPUI_EDITOR_MARKDOWN_CORPUS") else {
        return;
    };
    let corpus = std::fs::read_to_string(&path).expect("unreadable corpus");
    let options = MarkdownOptions {
        flavor: Flavor::TipTapCompat,
    };
    let mut checked = 0;
    let mut failures = Vec::new();
    for (index, line) in corpus.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let entry: Value = serde_json::from_str(line).expect("invalid JSON line");
        let node = Node::from_json(&entry["doc"]).expect("unreadable document");
        let actual = node.to_markdown_with(&options);
        let expected = entry["markdown"].as_str().unwrap_or_default();
        if actual.trim() != expected.trim() {
            let at = actual
                .bytes()
                .zip(expected.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            failures.push(format!(
                "document {index}: mismatch at byte {at} (expected {} bytes, got {})",
                expected.len(),
                actual.len()
            ));
        }
        checked += 1;
    }
    eprintln!(
        "{checked} exports compared, {} mismatch(es)",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
