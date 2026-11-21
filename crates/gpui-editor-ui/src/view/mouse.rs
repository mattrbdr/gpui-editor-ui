//! Mouse selection and automatic scrolling.

use std::time::Duration;

use gpui::{
    Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
    point, px,
};

use super::{EditorEvent, EditorView};

impl EditorView {
    // --- Mouse ---

    pub(crate) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        window.focus(&self.focus_handle, cx);
        let Some(pos) = self.hit_test(event.position) else {
            return;
        };
        if event.modifiers.control
            && event.click_count == 1
            && let Some(id) = self.editor.mention_near(pos)
        {
            cx.emit(EditorEvent::MentionClicked(id));
            return;
        }
        if event.modifiers.control
            && event.click_count == 1
            && let Some((_, _, href)) = self.editor.link_at(pos)
        {
            cx.emit(EditorEvent::LinkClicked(href));
            return;
        }
        self.selecting = true;
        self.start_autoscroll(window, cx);
        self.marked = None;
        self.goal_x = None;
        match event.click_count {
            1 => self.editor.move_to(pos, event.modifiers.shift),
            2 => self.editor.select_word_at(pos),
            _ => self.editor.select_block(pos.block),
        }
        self.caret_visible = true;
        cx.notify();
    }

    pub(crate) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selecting
            && event.dragging()
            && let Some(pos) = self.hit_test(event.position)
        {
            self.editor.move_to(pos, true);
            cx.notify();
        }
    }

    pub(crate) fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    /// One autoscroll step: the pointer is past the top or bottom edge of the visible area during
    /// a selection. Scrolls (at a speed proportional to the distance) and extends the selection to
    /// the edge. Returns true if the scroll offset moved.
    pub(crate) fn autoscroll_step(&mut self, mouse: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if !self.selecting {
            return false;
        }
        let viewport = self.scroll.bounds();
        let overshoot = if mouse.y < viewport.top() {
            mouse.y - viewport.top()
        } else if mouse.y > viewport.bottom() {
            mouse.y - viewport.bottom()
        } else {
            return false;
        };
        let speed = (overshoot * 0.4).clamp(px(-40.0), px(40.0));
        let offset = self.scroll.offset();
        let y = (offset.y - speed).clamp(-self.scroll.max_offset().y, px(0.0));
        let moved = y != offset.y;
        if moved {
            self.scroll.set_offset(point(offset.x, y));
        }
        let target = point(
            mouse.x.clamp(viewport.left(), viewport.right()),
            mouse.y.clamp(viewport.top(), viewport.bottom() - px(1.0)),
        );
        if let Some(pos) = self.hit_test(target) {
            self.editor.move_to(pos, true);
        }
        cx.notify();
        moved
    }

    /// Starts (only once) the loop that scrolls while the mouse is dragged outside the viewport.
    pub(super) fn start_autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.autoscrolling {
            return;
        }
        self.autoscrolling = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this.update_in(cx, |this, window, cx| {
                    if !this.selecting {
                        this.autoscrolling = false;
                        return false;
                    }
                    let mouse = window.mouse_position();
                    this.autoscroll_step(mouse, cx);
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// One autoscroll step with a simulated pointer (tests and integrations).
    #[cfg(feature = "test-support")]
    pub fn autoscroll_for_tests(&mut self, mouse: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        self.autoscroll_step(mouse, cx)
    }

    /// Bottom edge of the visible area (tests and integrations).
    #[must_use]
    #[cfg(feature = "test-support")]
    pub fn scroll_viewport_bottom_for_tests(&self) -> Pixels {
        self.scroll.bounds().bottom()
    }

    /// Starts a mouse selection (tests and integrations).
    #[cfg(feature = "test-support")]
    pub fn begin_selection_for_tests(&mut self) {
        self.selecting = true;
    }
}
