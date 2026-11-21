//! GPUI editor state: selection, mouse, clipboard, IME.

mod actions;
mod geometry;
mod ime;
mod menus;
mod mouse;

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, Hsla, ImageSource, Pixels,
    ScrollHandle, SharedString, Task, TextLayout, Window, hsla, point, px,
};
use gpui_editor_core::{BlockCommand, Editor, Node};
#[cfg(feature = "test-support")]
use gpui_editor_core::{Pos, Selection};

/// Editor colors, fonts and sizes.
#[derive(Debug, Clone)]
pub struct EditorTheme {
    /// Font family of the text and menus; it needs bold and italic faces to render those marks.
    pub font_family: SharedString,
    /// Font family of code blocks.
    pub mono_family: SharedString,
    /// Base font size (headings are scaled from it).
    pub font_size: Pixels,
    /// Base line height (headings are scaled from it).
    pub line_height: Pixels,
    /// Background of the editor and of its menus.
    pub background: Hsla,
    /// Main text color.
    pub text: Hsla,
    /// Secondary text: list markers, checked tasks, `[name]` markers, menu details.
    pub muted: Hsla,
    /// Caret color, also the fill of checked task boxes.
    pub caret: Hsla,
    /// Selection highlight and the active row of the menus.
    pub selection: Hsla,
    /// Link and mention color.
    pub link: Hsla,
    /// Background of inline code, code blocks, mentions, embed cards and hovered menu rows.
    pub code_background: Hsla,
    /// Borders (menus, cards, quote bar, task boxes) and horizontal rules.
    pub border: Hsla,
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            font_family: ".SystemUIFont".into(),
            mono_family: "monospace".into(),
            font_size: px(15.0),
            line_height: px(24.0),
            background: hsla(0.0, 0.0, 1.0, 1.0),
            text: hsla(0.0, 0.0, 0.1, 1.0),
            muted: hsla(0.0, 0.0, 0.45, 1.0),
            caret: hsla(0.6, 0.9, 0.45, 1.0),
            selection: hsla(0.6, 0.9, 0.6, 0.3),
            link: hsla(0.6, 0.8, 0.4, 1.0),
            code_background: hsla(0.0, 0.0, 0.0, 0.07),
            border: hsla(0.0, 0.0, 0.0, 0.2),
        }
    }
}

impl EditorTheme {
    /// Dark variant of the default theme: light text on a dark gray background, with contrasts of
    /// at least 4.5:1 for the text, links and muted text.
    ///
    /// ```
    /// use gpui_editor_ui::EditorTheme;
    ///
    /// let mut theme = EditorTheme::dark();
    /// theme.font_family = "Inter".into();
    /// assert_eq!(theme.font_size, EditorTheme::default().font_size);
    /// assert!(theme.background.l < theme.text.l);
    /// ```
    #[must_use]
    pub fn dark() -> Self {
        Self {
            background: hsla(0.0, 0.0, 0.11, 1.0),
            text: hsla(0.0, 0.0, 0.9, 1.0),
            muted: hsla(0.0, 0.0, 0.62, 1.0),
            caret: hsla(0.6, 0.9, 0.68, 1.0),
            selection: hsla(0.6, 0.9, 0.6, 0.35),
            link: hsla(0.58, 0.9, 0.72, 1.0),
            code_background: hsla(0.0, 0.0, 1.0, 0.08),
            border: hsla(0.0, 0.0, 1.0, 0.22),
            ..Self::default()
        }
    }
}

/// Resolves the `src` attribute of an image block into something GPUI can load.
pub type ImageResolver = Rc<dyn Fn(&str) -> Option<ImageSource>>;

/// Default image resolution: `file://` URLs and absolute paths; anything else stays a marker.
fn default_image_source(src: &str) -> Option<ImageSource> {
    let path = src.strip_prefix("file://").unwrap_or(src);
    let path = std::path::Path::new(path);
    path.is_absolute().then(|| ImageSource::from(path))
}

/// Event emitted by the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditorEvent {
    /// The document content changed.
    Changed,
    /// The text typed after `[[` changed: the application responds with
    /// [`EditorView::set_mention_items`].
    MentionQuery(String),
    /// Ctrl+click on a mention: the identifier of its target (for example `document:<uuid>`).
    MentionClicked(String),
    /// Ctrl+K: the application should ask for a link URL. Contains the current URL if the caret
    /// is inside a link (which is then selected in full), otherwise `None` (the word under the
    /// caret is selected if there is no selection). Respond with [`EditorView::apply_link`].
    LinkRequested(Option<String>),
    /// Ctrl+click on a link: its URL.
    LinkClicked(String),
}

/// Text being composed (IME), within a single block; offsets in characters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Marked {
    pub block: usize,
    pub start: usize,
    pub end: usize,
}

/// Open `/` menu: target block, offset of the `/` and selected row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slash {
    pub block: usize,
    pub start: usize,
    pub selected: usize,
}

/// Preview of an unknown block (for example a database view), provided by the application:
/// title, subtitle and a small table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmbedCard {
    /// Title, shown first.
    pub title: String,
    /// Line shown under the title, in the muted color.
    pub subtitle: String,
    /// Column headers of the table; no header row when empty.
    pub columns: Vec<String>,
    /// Table rows, one cell per column.
    pub rows: Vec<Vec<String>>,
}

/// Open `[[` menu: target block, offset of the `[[` and selected row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MentionMenu {
    pub block: usize,
    pub start: usize,
    pub selected: usize,
}

/// Target offered by the `[[` menu (provided by the application).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionItem {
    /// Identifier stored in the mention (for example `document:<uuid>`).
    pub id: String,
    /// Title shown in the text.
    pub title: String,
    /// Detail shown below the title (path, preview).
    pub detail: String,
}

/// `/` menu commands: label, search keywords, effect.
pub(crate) const SLASH_COMMANDS: [(&str, &str, BlockCommand); 10] = [
    ("Text", "text paragraph", BlockCommand::Paragraph),
    ("Heading 1", "heading title h1 #", BlockCommand::Heading(1)),
    ("Heading 2", "heading title h2 ##", BlockCommand::Heading(2)),
    (
        "Heading 3",
        "heading title h3 ###",
        BlockCommand::Heading(3),
    ),
    (
        "Bulleted list",
        "bulleted list bullet ul -",
        BlockCommand::BulletList,
    ),
    (
        "Numbered list",
        "numbered list ordered ol 1.",
        BlockCommand::OrderedList,
    ),
    (
        "Task list",
        "task list todo checkbox [ ]",
        BlockCommand::TaskList,
    ),
    ("Quote", "quote blockquote >", BlockCommand::Quote),
    ("Code block", "code block ```", BlockCommand::CodeBlock),
    (
        "Divider",
        "divider rule separator hr ---",
        BlockCommand::Rule,
    ),
];

/// Editing component: a document, its selection and its history.
#[allow(clippy::struct_excessive_bools)]
pub struct EditorView {
    pub(crate) editor: Editor,
    pub(crate) theme: EditorTheme,
    pub(crate) focus_handle: FocusHandle,
    /// Text layout of each text block, `None` for blocks outside the laid-out area.
    pub(crate) layouts: Vec<Option<TextLayout>>,
    /// Heights of the top-level blocks as last laid out (windowed rendering).
    pub(crate) heights: Rc<std::cell::RefCell<crate::visible::Heights>>,
    pub(crate) painted: bool,
    pub(crate) selecting: bool,
    pub(crate) marked: Option<Marked>,
    pub(crate) goal_x: Option<Pixels>,
    pub(crate) slash: Option<Slash>,
    pub(crate) mention: Option<MentionMenu>,
    pub(crate) mention_items: Vec<MentionItem>,
    mention_query: String,
    /// Previews of unknown blocks, keyed by the value of their `viewId` attribute.
    pub(crate) embeds: std::collections::HashMap<String, EmbedCard>,
    /// Resolution of image sources, see [`EditorView::set_image_resolver`].
    pub(crate) image_resolver: Option<ImageResolver>,
    /// Name of the editor for assistive technology.
    pub(crate) accessible_label: SharedString,
    /// Ids of the accessibility text runs of the last frame, one per text block.
    pub(crate) a11y_runs: Rc<std::cell::RefCell<Vec<gpui::accesskit::NodeId>>>,
    /// Code block whose "Copy" button was just used.
    pub(crate) copied: Option<usize>,
    /// Vertical scrolling of the document.
    pub(crate) scroll: ScrollHandle,
    /// The next paint must scroll the caret back into view.
    pub(crate) reveal_caret: bool,
    /// Visible phase of the caret blink.
    pub(crate) caret_visible: bool,
    /// The editor had focus at the last paint (blinking stops without focus).
    pub(crate) focused: bool,
    /// The selection autoscroll loop is already running.
    autoscrolling: bool,
    _blink: Task<()>,
}

/// Half-period of the caret blink.
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
/// Margin (px) kept around the caret when scrolling it back into view.
const REVEAL_MARGIN: f32 = 24.0;

impl EventEmitter<EditorEvent> for EditorView {}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// --- Offset conversions -----------------------------------------------------------------------

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

pub(crate) fn chars_to_bytes(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
}

fn bytes_to_chars(text: &str, bytes: usize) -> usize {
    text.char_indices()
        .take_while(|(index, _)| *index < bytes)
        .count()
}

fn chars_to_utf16(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(char::len_utf16).sum()
}

fn utf16_to_chars(text: &str, utf16: usize) -> usize {
    let mut seen = 0;
    for (index, c) in text.chars().enumerate() {
        if seen >= utf16 {
            return index;
        }
        seen += c.len_utf16();
    }
    text.chars().count()
}

impl EditorView {
    /// Opens a document.
    pub fn new(doc: Node, theme: EditorTheme, cx: &mut Context<Self>) -> Self {
        Self {
            editor: Editor::new(doc),
            theme,
            focus_handle: cx.focus_handle(),
            layouts: Vec::new(),
            heights: Rc::default(),
            painted: false,
            selecting: false,
            marked: None,
            goal_x: None,
            slash: None,
            mention: None,
            mention_items: Vec::new(),
            mention_query: String::new(),
            embeds: std::collections::HashMap::new(),
            image_resolver: None,
            accessible_label: "Document editor".into(),
            a11y_runs: Rc::default(),
            copied: None,
            scroll: ScrollHandle::new(),
            reveal_caret: false,
            caret_visible: true,
            focused: false,
            autoscrolling: false,
            _blink: cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(BLINK_INTERVAL).await;
                    let alive = this.update(cx, |view, cx| {
                        if view.focused {
                            view.caret_visible = !view.caret_visible;
                            cx.notify();
                        }
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            }),
        }
    }

    /// The caret just moved: show it without waiting for the blink and scroll it into view.
    fn caret_moved(&mut self) {
        self.caret_visible = true;
        self.reveal_caret = true;
    }

    /// Scrolls the caret back into the visible area; returns true if the scroll offset changed.
    pub(crate) fn reveal_caret_now(&mut self) -> bool {
        if !std::mem::take(&mut self.reveal_caret) {
            return false;
        }
        let Some(caret) = self.caret_bounds() else {
            return false;
        };
        let viewport = self.scroll.bounds();
        let margin = px(REVEAL_MARGIN).min(viewport.size.height / 4.0);
        let offset = self.scroll.offset();
        let mut y = offset.y;
        if caret.top() < viewport.top() + margin {
            y += viewport.top() + margin - caret.top();
        } else if caret.bottom() > viewport.bottom() - margin {
            y -= caret.bottom() - (viewport.bottom() - margin);
        }
        let y = y.clamp(-self.scroll.max_offset().y, px(0.0));
        if y == offset.y {
            return false;
        }
        self.scroll.set_offset(point(offset.x, y));
        true
    }

    /// Visible phase of the blink (tests and integrations).
    #[must_use]
    #[cfg(feature = "test-support")]
    pub fn caret_visible_for_tests(&self) -> bool {
        self.caret_visible
    }

    /// Vertical scroll offset (tests and integrations).
    #[must_use]
    pub fn scroll_offset_y(&self) -> Pixels {
        self.scroll.offset().y
    }

    /// Current document.
    #[must_use]
    pub fn doc(&self) -> &Node {
        self.editor.doc()
    }

    /// Editing engine (read-only: selection, mark state, history...).
    #[must_use]
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Replaces the document (the history is cleared).
    pub fn set_doc(&mut self, doc: Node, cx: &mut Context<Self>) {
        self.editor = Editor::new(doc);
        self.marked = None;
        self.heights.borrow_mut().clear();
        self.painted = false;
        cx.notify();
    }

    /// Replaces the theme and redraws.
    pub fn set_theme(&mut self, theme: EditorTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.heights.borrow_mut().clear();
        cx.notify();
    }

    /// Sets how the `src` of an image block is loaded: return `None` to show the `[image: alt]`
    /// marker instead. Without a resolver, `file://` URLs and absolute paths are loaded. The
    /// document itself is never changed. Images inside a paragraph stay markers.
    pub fn set_image_resolver(
        &mut self,
        resolve: impl Fn(&str) -> Option<ImageSource> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.image_resolver = Some(Rc::new(resolve));
        self.heights.borrow_mut().clear();
        cx.notify();
    }

    /// Image source for the `src` of an image block, see [`EditorView::set_image_resolver`].
    pub(crate) fn resolve_image(&self, src: &str) -> Option<ImageSource> {
        match &self.image_resolver {
            Some(resolve) => resolve(src),
            None => default_image_source(src),
        }
    }

    /// Sets the name assistive technology announces for the editor (default: "Document editor").
    pub fn set_accessible_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.accessible_label = label.into();
        cx.notify();
    }

    /// `SetTextSelection` from assistive technology: moves the selection over the text runs.
    pub(crate) fn on_a11y_set_selection(
        &mut self,
        data: Option<&gpui::accesskit::ActionData>,
        cx: &mut Context<Self>,
    ) {
        let Some(gpui::accesskit::ActionData::SetTextSelection(requested)) = data else {
            return;
        };
        let selection = crate::a11y::selection_from(requested, &self.a11y_runs.borrow());
        if let Some(selection) = selection {
            self.nav(cx, |editor| editor.set_selection(selection));
        }
    }

    /// Gives keyboard focus to the editor.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    pub(crate) fn changed(&mut self, cx: &mut Context<Self>) {
        // The text layouts predate the change: no `hit_test` on them until the next paint.
        self.painted = false;
        self.goal_x = None;
        self.caret_moved();
        self.sync_slash();
        self.sync_mention(cx);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Places the caret without going through the mouse or keyboard (tests and integrations).
    #[cfg(feature = "test-support")]
    pub fn set_caret_for_tests(&mut self, pos: Pos, cx: &mut Context<Self>) {
        self.editor.set_selection(Selection::caret(pos));
        cx.notify();
    }

    /// Sets the selection (tests and integrations).
    #[cfg(feature = "test-support")]
    pub fn select_for_tests(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.editor.set_selection(selection);
        cx.notify();
    }

    /// Top and left edge of the text block `block`, once the editor has been painted.
    #[must_use]
    #[cfg(feature = "test-support")]
    pub fn block_origin_for_tests(&self, block: usize) -> Option<(Pixels, Pixels)> {
        if !self.ready() {
            return None;
        }
        let bounds = self.layouts.get(block)?.as_ref()?.bounds();
        Some((bounds.top(), bounds.left()))
    }

    /// Copies the text of the code block `block` to the clipboard ("Copy" button).
    pub fn copy_code_block(&mut self, block: usize, cx: &mut Context<Self>) {
        if block >= self.editor.block_count() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(self.editor.block_text(block)));
        self.copied = Some(block);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1600))
                .await;
            this.update(cx, |this, cx| {
                if this.copied == Some(block) {
                    this.copied = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Block whose "Copy" button is in the "Copied" state (tests and integrations).
    #[must_use]
    pub fn copied_block(&self) -> Option<usize> {
        self.copied
    }
}
