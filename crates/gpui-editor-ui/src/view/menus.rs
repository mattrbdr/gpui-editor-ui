//! The `/` menu and the `[[` mention menu.

use gpui::{Context, Window};
use gpui_editor_core::{BlockCommand, Kind, Pos};

use crate::keys;

use super::{EditorEvent, EditorView, EmbedCard, MentionItem, MentionMenu, SLASH_COMMANDS, Slash};

impl EditorView {
    // --- `/` menu ---

    /// Text typed after the `/` (menu filter).
    pub(super) fn slash_query(&self) -> Option<String> {
        let slash = self.slash?;
        let head = self.editor.selection().head;
        let text = self.editor.block_text(slash.block);
        let chars: Vec<char> = text.chars().collect();
        (head.block == slash.block
            && head.offset > slash.start
            && chars.get(slash.start) == Some(&'/'))
        .then(|| {
            chars[slash.start + 1..head.offset.min(chars.len())]
                .iter()
                .collect()
        })
    }

    /// Commands matching the filter.
    pub(crate) fn slash_items(&self) -> Vec<(&'static str, BlockCommand)> {
        let query = self.slash_query().unwrap_or_default().to_lowercase();
        SLASH_COMMANDS
            .iter()
            .filter(|(label, keywords, _)| {
                query.is_empty()
                    || label.to_lowercase().contains(&query)
                    || keywords.contains(query.as_str())
            })
            .map(|(label, _, command)| (*label, *command))
            .collect()
    }

    /// Labels of the offered commands (tests and integrations).
    #[must_use]
    #[cfg(feature = "test-support")]
    pub fn slash_items_for_tests(&self) -> Vec<String> {
        self.slash_items()
            .into_iter()
            .map(|(label, _)| label.to_owned())
            .collect()
    }

    /// Whether the `/` menu is open.
    #[must_use]
    pub fn slash_open(&self) -> bool {
        self.slash.is_some()
    }

    /// Closes the menu once it no longer applies (caret moved away, space typed, `/` deleted).
    pub(super) fn sync_slash(&mut self) {
        let Some(mut slash) = self.slash else { return };
        match self.slash_query() {
            Some(query) if !query.contains(char::is_whitespace) => {
                slash.selected = slash
                    .selected
                    .min(self.slash_items().len().saturating_sub(1));
                self.slash = Some(slash);
            }
            _ => self.slash = None,
        }
    }

    /// Opens the menu when a `/` was just typed at the start of a word (outside code blocks).
    pub(super) fn maybe_open_slash(&mut self) {
        let selection = self.editor.selection();
        let head = selection.head;
        if !selection.is_collapsed()
            || head.offset == 0
            || self.editor.block_kind(head.block) == Kind::CodeBlock
        {
            return;
        }
        let chars: Vec<char> = self.editor.block_text(head.block).chars().collect();
        let start = head.offset - 1;
        let after_space = start == 0 || chars.get(start - 1).is_some_and(|c| c.is_whitespace());
        if chars.get(start) == Some(&'/') && after_space {
            self.slash = Some(Slash {
                block: head.block,
                start,
                selected: 0,
            });
        }
    }

    /// Applies menu command `index`: removes `/filter` then converts the block.
    pub(crate) fn run_slash(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(slash), Some((_, command))) =
            (self.slash, self.slash_items().get(index).copied())
        else {
            return;
        };
        let head = self.editor.selection().head;
        self.slash = None;
        self.editor.replace_range_with_block(
            Pos {
                block: slash.block,
                offset: slash.start,
            },
            head,
            command,
        );
        self.marked = None;
        self.changed(cx);
    }

    pub(crate) fn on_cancel(&mut self, _: &keys::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        let closed_slash = self.slash.take().is_some();
        let closed_mention = self.mention.take().is_some();
        if closed_slash || closed_mention {
            self.mention_items.clear();
            cx.notify();
        }
    }

    /// Moves the selected menu row; `false` if the menu is not open.
    pub(super) fn slash_step(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let count = self.slash_items().len();
        let Some(slash) = self.slash.as_mut() else {
            return false;
        };
        if count == 0 {
            return false;
        }
        slash.selected = slash.selected.saturating_add_signed(delta).min(count - 1);
        cx.notify();
        true
    }

    // --- `[[` menu ---

    /// Text typed after `[[`, as long as the menu still applies.
    pub(super) fn mention_query_now(&self) -> Option<String> {
        let menu = self.mention?;
        let head = self.editor.selection().head;
        let chars: Vec<char> = self.editor.block_text(menu.block).chars().collect();
        let opened = head.block == menu.block
            && head.offset >= menu.start + 2
            && chars.get(menu.start) == Some(&'[')
            && chars.get(menu.start + 1) == Some(&'[');
        let query: String = chars
            .get(menu.start + 2..head.offset.min(chars.len()))?
            .iter()
            .collect();
        (opened && !query.contains([']', '\n']) && query.chars().count() <= 80).then_some(query)
    }

    /// Opens the menu when `[[` was just typed (outside code blocks).
    pub(super) fn maybe_open_mention(&mut self) {
        let selection = self.editor.selection();
        let head = selection.head;
        if !selection.is_collapsed()
            || head.offset < 2
            || self.editor.block_kind(head.block) == Kind::CodeBlock
        {
            return;
        }
        let chars: Vec<char> = self.editor.block_text(head.block).chars().collect();
        if chars.get(head.offset - 2) == Some(&'[') && chars.get(head.offset - 1) == Some(&'[') {
            self.mention = Some(MentionMenu {
                block: head.block,
                start: head.offset - 2,
                selected: 0,
            });
            self.mention_items.clear();
            self.mention_query = "\u{0}".into();
        }
    }

    /// Closes the menu when out of context; otherwise reports the new query to the application.
    pub(super) fn sync_mention(&mut self, cx: &mut Context<Self>) {
        if self.mention.is_none() {
            return;
        }
        if let Some(query) = self.mention_query_now() {
            if query != self.mention_query {
                self.mention_query.clone_from(&query);
                if let Some(menu) = self.mention.as_mut() {
                    menu.selected = 0;
                }
                cx.emit(EditorEvent::MentionQuery(query));
            }
        } else {
            self.mention = None;
            self.mention_items.clear();
        }
    }

    /// Targets to offer in the `[[` menu (response to [`EditorEvent::MentionQuery`]).
    pub fn set_mention_items(&mut self, items: Vec<MentionItem>, cx: &mut Context<Self>) {
        if self.mention.is_some() {
            self.mention_items = items;
            if let Some(menu) = self.mention.as_mut() {
                menu.selected = menu
                    .selected
                    .min(self.mention_items.len().saturating_sub(1));
            }
            cx.notify();
        }
    }

    /// Previews to show in place of unknown blocks that carry a `viewId` attribute.
    pub fn set_embeds(
        &mut self,
        embeds: std::collections::HashMap<String, EmbedCard>,
        cx: &mut Context<Self>,
    ) {
        self.embeds = embeds;
        cx.notify();
    }

    /// Whether the `[[` menu is open.
    #[must_use]
    pub fn mention_open(&self) -> bool {
        self.mention.is_some()
    }

    /// Inserts menu target `index` in place of `[[query`.
    pub(crate) fn run_mention(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(menu), Some(item)) = (self.mention, self.mention_items.get(index).cloned())
        else {
            return;
        };
        let head = self.editor.selection().head;
        self.mention = None;
        self.mention_items.clear();
        self.editor.insert_mention(
            Pos {
                block: menu.block,
                offset: menu.start,
            },
            head,
            &item.id,
            &item.title,
        );
        self.marked = None;
        self.changed(cx);
    }

    pub(super) fn mention_step(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let count = self.mention_items.len();
        let Some(menu) = self.mention.as_mut() else {
            return false;
        };
        if count == 0 {
            return false;
        }
        menu.selected = menu.selected.saturating_add_signed(delta).min(count - 1);
        cx.notify();
        true
    }
}
