//! Replays a real-world corpus without ever committing it.
//!
//! Set `GPUI_EDITOR_CORPUS` to the path of a file containing one JSON document per line;
//! without this variable, the test does nothing.

use gpui_editor_core::Node;

/// Excerpt of `text` around byte `at`, without splitting a character.
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
    let corpus = std::fs::read_to_string(&path).expect("unreadable corpus");
    let mut checked = 0;
    for (index, line) in corpus
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let node = Node::from_json_str(line)
            .unwrap_or_else(|error| panic!("unreadable document {index}: {error}"));
        let written = node.to_json_string();
        if written != line {
            let at = written
                .bytes()
                .zip(line.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            panic!(
                "document {index}: mismatch at byte {at}\n  source:  {}\n  written: {}",
                window(line, at),
                window(&written, at)
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "empty corpus");
    eprintln!("{checked} documents checked");
}
