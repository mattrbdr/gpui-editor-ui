//! Compare le cœur à la sortie du vrai `@tiptap/markdown` (fixtures générées par `tools/gen-fixtures`).

use gpui_editor_core::Node;
use serde_json::Value;

fn cases() -> Vec<Value> {
    let source = include_str!("fixtures/tiptap_markdown.json");
    serde_json::from_str::<Value>(source)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn every_tiptap_document_round_trips_byte_for_byte() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let node = Node::from_json(&case["doc"]).unwrap_or_else(|e| panic!("{name} : {e}"));
        assert_eq!(
            node.to_json_string(),
            case["doc"].to_string(),
            "cas « {name} »"
        );
    }
}

#[test]
fn plain_text_matches_the_reference_implementation() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let node = Node::from_json(&case["doc"]).unwrap();
        assert_eq!(
            node.plain_text(),
            case["plain_text"].as_str().unwrap(),
            "cas « {name} »"
        );
    }
}

#[test]
fn the_fixture_set_covers_the_expected_cases() {
    let names: Vec<String> = cases()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.len() >= 20, "{names:?}");
    for expected in [
        "task_list",
        "code_block",
        "hard_break",
        "nested_marks",
        "image",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "cas manquant : {expected}"
        );
    }
}
