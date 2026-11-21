//! Keyboard actions: navigation, editing, clipboard.

use gpui::{ClipboardItem, Context, TextLayout, Window, point};
use gpui_editor_core::{Editor, Kind, MarkKind, Pos, Selection};

use crate::keys;

use super::{EditorEvent, EditorView};

impl EditorView {
    // --- Actions: navigation ---

    pub(super) fn nav(&mut self, cx: &mut Context<Self>, run: impl FnOnce(&mut Editor)) {
        run(&mut self.editor);
        self.goal_x = None;
        self.caret_moved();
        self.sync_slash();
        self.sync_mention(cx);
        cx.notify();
    }

    pub(crate) fn on_left(&mut self, _: &keys::Left, _: &mut Window, cx: &mut Context<Self>) {
        self.nav(cx, |e| e.move_left(false));
    }

    pub(crate) fn on_right(&mut self, _: &keys::Right, _: &mut Window, cx: &mut Context<Self>) {
        self.nav(cx, |e| e.move_right(false));
    }

    pub(crate) fn on_select_left(
        &mut self,
        _: &keys::SelectLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_left(true));
    }

    pub(crate) fn on_select_right(
        &mut self,
        _: &keys::SelectRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_right(true));
    }

    pub(crate) fn on_word_left(
        &mut self,
        _: &keys::WordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_word_left(false));
    }

    pub(crate) fn on_word_right(
        &mut self,
        _: &keys::WordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_word_right(false));
    }

    pub(crate) fn on_select_word_left(
        &mut self,
        _: &keys::SelectWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_word_left(true));
    }

    pub(crate) fn on_select_word_right(
        &mut self,
        _: &keys::SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_word_right(true));
    }

    pub(crate) fn on_home(&mut self, _: &keys::Home, _: &mut Window, cx: &mut Context<Self>) {
        self.nav(cx, |e| e.move_to_block_start(false));
    }

    pub(crate) fn on_end(&mut self, _: &keys::End, _: &mut Window, cx: &mut Context<Self>) {
        self.nav(cx, |e| e.move_to_block_end(false));
    }

    pub(crate) fn on_select_home(
        &mut self,
        _: &keys::SelectHome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_to_block_start(true));
    }

    pub(crate) fn on_select_end(
        &mut self,
        _: &keys::SelectEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_to_block_end(true));
    }

    pub(crate) fn on_doc_start(
        &mut self,
        _: &keys::DocStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_to_doc_start(false));
    }

    pub(crate) fn on_doc_end(&mut self, _: &keys::DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.nav(cx, |e| e.move_to_doc_end(false));
    }

    pub(crate) fn on_select_doc_start(
        &mut self,
        _: &keys::SelectDocStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_to_doc_start(true));
    }

    pub(crate) fn on_select_doc_end(
        &mut self,
        _: &keys::SelectDocEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, |e| e.move_to_doc_end(true));
    }

    pub(crate) fn on_select_all(
        &mut self,
        _: &keys::SelectAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.nav(cx, Editor::select_all);
    }

    /// Moves one visual line up or down, keeping the starting column.
    pub(super) fn vertical(&mut self, direction: f32, extend: bool, cx: &mut Context<Self>) {
        let head = self.editor.selection().head;
        let Some(origin) = self.caret_point(head) else {
            return;
        };
        let Some(layout) = self.layouts[head.block].clone() else {
            return;
        };
        let line = layout.line_height();
        let x = *self.goal_x.get_or_insert(origin.x);
        let target = point(x, origin.y + line / 2.0 + line * direction);
        let at_edge =
            (direction < 0.0 && head.block == 0 && origin.y - layout.bounds().top() < line / 2.0)
                || (direction > 0.0
                    && head.block + 1 == self.layouts.len()
                    && layout.bounds().bottom() - origin.y < line * 1.5);
        let goal = self.goal_x;
        if at_edge {
            if direction < 0.0 {
                self.editor.move_to_doc_start(extend);
            } else {
                self.editor.move_to_doc_end(extend);
            }
        } else if let Some(pos) = self.hit_test(target) {
            self.editor.move_to(pos, extend);
        }
        self.goal_x = goal;
        self.caret_moved();
        cx.notify();
    }

    /// Moves the caret by one "page" (visible height minus one line), keeping the column.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(super) fn page(&mut self, direction: f32, extend: bool, cx: &mut Context<Self>) {
        let head = self.editor.selection().head;
        let Some(line) = self
            .layouts
            .get(head.block)
            .and_then(Option::as_ref)
            .map(TextLayout::line_height)
        else {
            return;
        };
        let lines = (self.scroll.bounds().size.height / line).floor().max(2.0) - 1.0;
        for _ in 0..(lines as usize) {
            self.vertical(direction, extend, cx);
        }
    }

    pub(crate) fn on_page_up(&mut self, _: &keys::PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.page(-1.0, false, cx);
    }

    pub(crate) fn on_page_down(
        &mut self,
        _: &keys::PageDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page(1.0, false, cx);
    }

    pub(crate) fn on_select_page_up(
        &mut self,
        _: &keys::SelectPageUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page(-1.0, true, cx);
    }

    pub(crate) fn on_select_page_down(
        &mut self,
        _: &keys::SelectPageDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page(1.0, true, cx);
    }

    pub(crate) fn on_up(&mut self, _: &keys::Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.slash_step(-1, cx) || self.mention_step(-1, cx) {
            return;
        }
        self.vertical(-1.0, false, cx);
    }

    pub(crate) fn on_down(&mut self, _: &keys::Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.slash_step(1, cx) || self.mention_step(1, cx) {
            return;
        }
        self.vertical(1.0, false, cx);
    }

    pub(crate) fn on_select_up(
        &mut self,
        _: &keys::SelectUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vertical(-1.0, true, cx);
    }

    pub(crate) fn on_select_down(
        &mut self,
        _: &keys::SelectDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vertical(1.0, true, cx);
    }

    // --- Actions: editing ---

    pub(super) fn edit(&mut self, cx: &mut Context<Self>, run: impl FnOnce(&mut Editor)) {
        run(&mut self.editor);
        self.marked = None;
        self.changed(cx);
    }

    pub(crate) fn on_backspace(
        &mut self,
        _: &keys::Backspace,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, Editor::delete_backward);
    }

    pub(crate) fn on_delete(&mut self, _: &keys::Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, Editor::delete_forward);
    }

    pub(crate) fn on_enter(&mut self, _: &keys::Enter, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(slash) = self.slash
            && !self.slash_items().is_empty()
        {
            self.run_slash(slash.selected, cx);
            return;
        }
        if let Some(menu) = self.mention
            && !self.mention_items.is_empty()
        {
            self.run_mention(menu.selected, cx);
            return;
        }
        self.slash = None;
        self.mention = None;
        self.edit(cx, Editor::split_block);
    }

    pub(crate) fn on_hard_break(
        &mut self,
        _: &keys::HardBreak,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, Editor::insert_hard_break);
    }

    pub(crate) fn on_indent(&mut self, _: &keys::Indent, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, Editor::indent);
    }

    pub(crate) fn on_outdent(&mut self, _: &keys::Outdent, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, Editor::outdent);
    }

    pub(crate) fn on_undo(&mut self, _: &keys::Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, Editor::undo);
    }

    pub(crate) fn on_redo(&mut self, _: &keys::Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, Editor::redo);
    }

    pub(crate) fn on_bold(&mut self, _: &keys::Bold, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |e| e.toggle_mark(MarkKind::Bold));
    }

    pub(crate) fn on_italic(&mut self, _: &keys::Italic, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |e| e.toggle_mark(MarkKind::Italic));
    }

    pub(crate) fn on_underline(
        &mut self,
        _: &keys::Underline,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_mark(MarkKind::Underline));
    }

    pub(crate) fn on_strike(&mut self, _: &keys::Strike, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |e| e.toggle_mark(MarkKind::Strike));
    }

    pub(crate) fn on_link(&mut self, _: &keys::Link, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.editor.select_link_at_caret();
        if current.is_none() && self.editor.selection().is_collapsed() {
            self.editor.select_word_at(self.editor.selection().head);
        }
        cx.emit(EditorEvent::LinkRequested(current));
        cx.notify();
    }

    /// Sets (`Some`) or removes (`None`) the link on the current selection; response to
    /// [`EditorEvent::LinkRequested`].
    pub fn apply_link(&mut self, href: Option<&str>, cx: &mut Context<Self>) {
        self.edit(cx, |e| e.set_link(href));
    }

    pub(crate) fn on_code(&mut self, _: &keys::Code, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |e| e.toggle_mark(MarkKind::Code));
    }

    pub(crate) fn on_heading1(
        &mut self,
        _: &keys::Heading1,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_heading(1));
    }

    pub(crate) fn on_heading2(
        &mut self,
        _: &keys::Heading2,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_heading(2));
    }

    pub(crate) fn on_heading3(
        &mut self,
        _: &keys::Heading3,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_heading(3));
    }

    pub(crate) fn on_code_block(
        &mut self,
        _: &keys::CodeBlock,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, Editor::toggle_code_block);
    }

    pub(crate) fn on_bullet_list(
        &mut self,
        _: &keys::BulletList,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_list(Kind::BulletList));
    }

    pub(crate) fn on_ordered_list(
        &mut self,
        _: &keys::OrderedList,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_list(Kind::OrderedList));
    }

    pub(crate) fn on_task_list(
        &mut self,
        _: &keys::TaskList,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |e| e.toggle_list(Kind::TaskList));
    }

    pub(crate) fn on_toggle_task(
        &mut self,
        _: &keys::ToggleTask,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, Editor::toggle_task_checked);
    }

    pub(crate) fn toggle_task_at(&mut self, block: usize, cx: &mut Context<Self>) {
        self.edit(cx, |e| {
            e.set_selection(Selection::caret(Pos { block, offset: 0 }));
            e.toggle_task_checked();
        });
    }

    // --- Clipboard ---

    pub(crate) fn on_copy(&mut self, _: &keys::Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.selection().is_collapsed() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.editor.selected_text()));
        }
    }

    pub(crate) fn on_cut(&mut self, _: &keys::Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.selection().is_collapsed() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.editor.selected_text()));
            self.edit(cx, Editor::delete_backward);
        }
    }

    pub(crate) fn on_paste(&mut self, _: &keys::Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = text.replace("\r\n", "\n");
            self.edit(cx, |e| e.insert_text(&text));
        }
    }
}
