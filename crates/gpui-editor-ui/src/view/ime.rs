//! Input method support: UTF-16 offsets over the whole document.

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, px, size};
use gpui_editor_core::{Pos, Selection};

use super::{EditorView, Marked, chars_to_utf16, utf16_len, utf16_to_chars};

impl EditorView {
    // --- Décalages UTF-16 globaux (texte du document, blocs séparés par un saut de ligne) ---

    pub(super) fn global_utf16(&self, pos: Pos) -> usize {
        let before: usize = (0..pos.block)
            .map(|b| utf16_len(&self.editor.block_text(b)) + 1)
            .sum();
        before + chars_to_utf16(&self.editor.block_text(pos.block), pos.offset)
    }

    pub(super) fn pos_from_utf16(&self, mut offset: usize) -> Pos {
        let count = self.editor.block_count();
        for block in 0..count {
            let text = self.editor.block_text(block);
            let len = utf16_len(&text);
            if offset <= len {
                return Pos {
                    block,
                    offset: utf16_to_chars(&text, offset),
                };
            }
            offset -= len + 1;
        }
        let last = count - 1;
        Pos {
            block: last,
            offset: self.editor.block_len(last),
        }
    }

    pub(super) fn full_text(&self) -> String {
        (0..self.editor.block_count())
            .map(|b| self.editor.block_text(b))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.full_text();
        let start = utf16_to_chars(&text, range.start);
        let end = utf16_to_chars(&text, range.end).max(start);
        adjusted.replace(chars_to_utf16(&text, start)..chars_to_utf16(&text, end));
        Some(text.chars().skip(start).take(end - start).collect())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.editor.selection();
        let (start, end) = (
            self.global_utf16(selection.start()),
            self.global_utf16(selection.end()),
        );
        Some(UTF16Selection {
            range: start..end,
            reversed: selection.anchor > selection.head,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.map(|m| {
            self.global_utf16(Pos {
                block: m.block,
                offset: m.start,
            })..self.global_utf16(Pos {
                block: m.block,
                offset: m.end,
            })
        })
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let given = range.map(|r| (self.pos_from_utf16(r.start), self.pos_from_utf16(r.end)));
        if let Some(marked) = self.marked.take() {
            let (start, end) = given.unwrap_or((
                Pos {
                    block: marked.block,
                    offset: marked.start,
                },
                Pos {
                    block: marked.block,
                    offset: marked.end,
                },
            ));
            self.editor.replace_range(start, end, text, false);
        } else if let Some((start, end)) = given {
            self.editor.replace_range(start, end, text, true);
        } else {
            self.editor.insert_text(text);
            if text == "/" {
                self.maybe_open_slash();
            }
            if text == "[" {
                self.maybe_open_mention();
            }
        }
        self.changed(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (start, end) = match (range, self.marked) {
            (Some(r), _) => (self.pos_from_utf16(r.start), self.pos_from_utf16(r.end)),
            (None, Some(m)) => (
                Pos {
                    block: m.block,
                    offset: m.start,
                },
                Pos {
                    block: m.block,
                    offset: m.end,
                },
            ),
            (None, None) => (
                self.editor.selection().start(),
                self.editor.selection().end(),
            ),
        };
        if self.marked.is_none() {
            self.editor.begin_batch();
        }
        self.editor.replace_range(start, end, new_text, false);
        let chars = new_text.chars().count();
        self.marked = (chars > 0 && !new_text.contains('\n')).then_some(Marked {
            block: start.block,
            start: start.offset,
            end: start.offset + chars,
        });
        if let (Some(selected), Some(_)) = (new_selected, self.marked) {
            let from = utf16_to_chars(new_text, selected.start);
            let to = utf16_to_chars(new_text, selected.end);
            self.editor.set_selection(Selection {
                anchor: Pos {
                    block: start.block,
                    offset: start.offset + from,
                },
                head: Pos {
                    block: start.block,
                    offset: start.offset + to,
                },
            });
        }
        self.changed(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let pos = self.pos_from_utf16(range.start);
        let origin = self.caret_point(pos)?;
        let height = self.layouts.get(pos.block)?.line_height();
        let _ = element_bounds;
        Some(Bounds::new(origin, size(px(1.0), height)))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let pos = self.hit_test(position)?;
        Some(self.global_utf16(pos))
    }
}
