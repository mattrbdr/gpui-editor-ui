//! Find and replace: highlighted matches, navigation between them.

use gpui::{Bounds, Context, Pixels};
use gpui_editor_core::{Match, Selection};

use super::EditorView;

/// Text being searched and its matches in the current document.
#[derive(Debug, Default)]
pub(crate) struct Search {
    query: String,
    matches: Vec<Match>,
}

fn selection_of(found: Match) -> Selection {
    Selection {
        anchor: found.start,
        head: found.end,
    }
}

impl EditorView {
    /// Highlights every occurrence of `query` and selects the first one from the caret;
    /// `None` ends the search and keeps the selection. Case and accents are ignored.
    pub fn set_search(&mut self, query: Option<String>, cx: &mut Context<Self>) {
        self.search = query.map(|query| Search {
            matches: self.editor.find(&query),
            query,
        });
        let from = self.editor.selection().start();
        let first = self.search.as_ref().and_then(|search| {
            let next = search.matches.iter().find(|m| m.start >= from);
            next.or_else(|| search.matches.first()).copied()
        });
        match first {
            Some(found) => self.select_match(found, cx),
            None => cx.notify(),
        }
    }

    /// Index of the selected match, if the selection is one, and number of matches; `None`
    /// without a search.
    #[must_use]
    pub fn search_status(&self) -> Option<(Option<usize>, usize)> {
        let search = self.search.as_ref()?;
        let selection = self.editor.selection();
        let current = search
            .matches
            .iter()
            .position(|m| m.start == selection.start() && m.end == selection.end());
        Some((current, search.matches.len()))
    }

    /// Selects the match after (`forward`) or before the selection, wrapping around.
    pub fn search_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(search) = self.search.as_ref() else {
            return;
        };
        let selection = self.editor.selection();
        let matches = &search.matches;
        let found = if forward {
            let next = matches.iter().find(|m| m.start >= selection.end());
            next.or_else(|| matches.first())
        } else {
            let previous = matches.iter().rev().find(|m| m.end <= selection.start());
            previous.or_else(|| matches.last())
        };
        if let Some(found) = found.copied() {
            self.select_match(found, cx);
        }
    }

    /// Replaces the selected match with `text` and selects the next one. Without a selected
    /// match, or if it cannot be replaced, only moves to the next one and returns false.
    pub fn replace_current(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let selected = self.search_status().and_then(|(current, _)| current);
        let found = selected.and_then(|index| Some(self.search.as_ref()?.matches[index]));
        let Some(found) = found.filter(|found| found.replaceable) else {
            self.search_step(true, cx);
            return false;
        };
        self.edit(cx, |editor| {
            editor.replace_matches(&[found], text);
        });
        self.search_step(true, cx);
        true
    }

    /// Replaces every replaceable match with `text`, as one undo step; returns their number.
    pub fn replace_all(&mut self, text: &str, cx: &mut Context<Self>) -> usize {
        let Some(search) = self.search.as_ref() else {
            return 0;
        };
        let matches = search.matches.clone();
        if !matches.iter().any(|m| m.replaceable) {
            return 0;
        }
        let mut count = 0;
        self.edit(cx, |editor| count = editor.replace_matches(&matches, text));
        count
    }

    /// Searches again after the document changed.
    pub(super) fn refresh_search(&mut self) {
        if let Some(search) = self.search.as_mut() {
            search.matches = self.editor.find(&search.query);
        }
    }

    fn select_match(&mut self, found: Match, cx: &mut Context<Self>) {
        self.nav(cx, |editor| editor.set_selection(selection_of(found)));
    }

    /// Rectangles of the visible matches, and whether each belongs to the selected one.
    pub(crate) fn search_quads(&self) -> Vec<(Bounds<Pixels>, bool)> {
        let Some(search) = self.search.as_ref() else {
            return Vec::new();
        };
        let selection = self.editor.selection();
        search
            .matches
            .iter()
            .flat_map(|found| {
                let current = selection_of(*found) == selection;
                self.range_quads(found.start, found.end)
                    .into_iter()
                    .map(move |quad| (quad, current))
            })
            .collect()
    }

    /// Number of highlight rectangles (tests and integrations).
    #[must_use]
    #[cfg(feature = "test-support")]
    pub fn search_highlights_for_tests(&self) -> usize {
        self.search_quads().len()
    }
}
