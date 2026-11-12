//! Compare l'export Markdown compatible TipTap à des exports réels, sans jamais les versionner.
//!
//! `GPUI_EDITOR_MARKDOWN_CORPUS` : fichier avec un objet JSON par ligne, `{"doc": …, "markdown": "…"}`.
//! Sans cette variable, le test ne fait rien.

use gpui_editor_core::{Flavor, MarkdownOptions, Node};
use serde_json::Value;

#[test]
fn compat_export_matches_real_exports() {
    let Some(path) = std::env::var_os("GPUI_EDITOR_MARKDOWN_CORPUS") else {
        return;
    };
    let corpus = std::fs::read_to_string(&path).expect("corpus illisible");
    let options = MarkdownOptions {
        flavor: Flavor::TipTapCompat,
    };
    let mut checked = 0;
    let mut failures = Vec::new();
    for (index, line) in corpus.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let entry: Value = serde_json::from_str(line).expect("ligne JSON invalide");
        let node = Node::from_json(&entry["doc"]).expect("document illisible");
        let actual = node.to_markdown_with(&options);
        let expected = entry["markdown"].as_str().unwrap_or_default();
        if actual.trim() != expected.trim() {
            let at = actual
                .bytes()
                .zip(expected.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            failures.push(format!(
                "document {index} : écart à l'octet {at} (attendu {} octets, obtenu {})",
                expected.len(),
                actual.len()
            ));
        }
        checked += 1;
    }
    eprintln!("{checked} exports comparés, {} écart(s)", failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
