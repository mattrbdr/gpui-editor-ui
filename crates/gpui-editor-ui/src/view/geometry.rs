//! Hit testing, caret and selection geometry.

use gpui::{Bounds, Pixels, Point, TextLayout, point, px, size};
use gpui_editor_core::Pos;

use super::{EditorView, bytes_to_chars, chars_to_bytes};

impl EditorView {
    // --- Geometry ---

    pub(super) fn ready(&self) -> bool {
        self.painted && self.layouts.len() == self.editor.block_count()
    }

    pub(super) fn block_bytes(&self, block: usize, chars: usize) -> usize {
        chars_to_bytes(&self.editor.block_text(block), chars)
    }

    /// Closest position to a point in the window.
    pub(crate) fn hit_test(&self, position: Point<Pixels>) -> Option<Pos> {
        if !self.ready() {
            return None;
        }
        let block = self
            .layouts
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let distance = |layout: &TextLayout| {
                    let bounds = layout.bounds();
                    if position.y < bounds.top() {
                        bounds.top() - position.y
                    } else if position.y > bounds.bottom() {
                        position.y - bounds.bottom()
                    } else {
                        px(0.0)
                    }
                };
                distance(a)
                    .partial_cmp(&distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)?;
        let bytes = match self.layouts[block].index_for_position(position) {
            Ok(bytes) | Err(bytes) => bytes,
        };
        let text = self.editor.block_text(block);
        Some(Pos {
            block,
            offset: bytes_to_chars(&text, bytes.min(text.len())),
        })
    }

    pub(super) fn caret_point(&self, pos: Pos) -> Option<Point<Pixels>> {
        if !self.ready() {
            return None;
        }
        self.layouts[pos.block].position_for_index(self.block_bytes(pos.block, pos.offset))
    }

    /// Caret rectangle (for painting and for the IME candidate window).
    pub(crate) fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        let head = self.editor.selection().head;
        let origin = self.caret_point(head)?;
        Some(Bounds::new(
            origin,
            size(px(1.5), self.layouts[head.block].line_height()),
        ))
    }

    /// Selection rectangles, one visual line at a time.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub(crate) fn selection_quads(&self) -> Vec<Bounds<Pixels>> {
        let selection = self.editor.selection();
        if selection.is_collapsed() || !self.ready() {
            return Vec::new();
        }
        let (start, end) = (selection.start(), selection.end());
        let mut quads = Vec::new();
        for block in start.block..=end.block {
            let layout = &self.layouts[block];
            let len = self.editor.block_len(block);
            let from = if block == start.block {
                start.offset
            } else {
                0
            };
            let to = if block == end.block { end.offset } else { len };
            let bounds = layout.bounds();
            let line = layout.line_height();
            let (Some(a), Some(b)) = (
                layout.position_for_index(self.block_bytes(block, from)),
                layout.position_for_index(self.block_bytes(block, to)),
            ) else {
                continue;
            };
            let newline_selected = block != end.block;
            let tail = if newline_selected { px(6.0) } else { px(0.0) };
            if (a.y - b.y).abs() < line / 2.0 {
                quads.push(Bounds::from_corners(a, point(b.x + tail, a.y + line)));
                continue;
            }
            quads.push(Bounds::from_corners(a, point(bounds.right(), a.y + line)));
            let extra = (((b.y - a.y) / line).round() as i32 - 1).max(0);
            for step in 1..=extra {
                let y = a.y + line * step as f32;
                quads.push(Bounds::from_corners(
                    point(bounds.left(), y),
                    point(bounds.right(), y + line),
                ));
            }
            quads.push(Bounds::from_corners(
                point(bounds.left(), b.y),
                point(b.x + tail, b.y + line),
            ));
        }
        quads
    }
}
