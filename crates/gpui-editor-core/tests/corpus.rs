//! Rejoue un corpus réel sans jamais le versionner.
//!
//! Définir `GPUI_EDITOR_CORPUS` avec le chemin d'un fichier contenant un document JSON par ligne ;
//! sans cette variable, le test ne fait rien.

use gpui_editor_core::Node;

/// Extrait de `text` autour de l'octet `at`, sans couper un caractère.
fn window(text: &str, at: usize) -> &str {
    let mut start = at.saturating_sub(30);
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (at + 50).min(text.len());
    while !text.is_char_boundary(end) {
        end += 1;
    }
    &text[start..end]
}

#[test]
fn every_document_of_an_external_corpus_round_trips_byte_for_byte() {
    let Some(path) = std::env::var_os("GPUI_EDITOR_CORPUS") else {
        return;
    };
    let corpus = std::fs::read_to_string(&path).expect("corpus illisible");
    let mut checked = 0;
    for (index, line) in corpus
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let node = Node::from_json_str(line)
            .unwrap_or_else(|error| panic!("document {index} illisible : {error}"));
        let written = node.to_json_string();
        if written != line {
            let at = written
                .bytes()
                .zip(line.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            panic!(
                "document {index} : écart à l'octet {at}\n  source : {}\n  écrit  : {}",
                window(line, at),
                window(&written, at)
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "corpus vide");
    eprintln!("{checked} documents vérifiés");
}
