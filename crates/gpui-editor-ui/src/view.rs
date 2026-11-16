//! État de l'éditeur GPUI : sélection, souris, presse-papiers, IME.

use std::ops::Range;

use std::time::Duration;

use gpui::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle,
    SharedString, Task, TextLayout, UTF16Selection, Window, hsla, point, px, size,
};
use gpui_editor_core::{BlockCommand, Editor, Kind, MarkKind, Node, Pos, Selection};

use crate::keys;

/// Couleurs, polices et tailles de l'éditeur.
#[derive(Debug, Clone)]
pub struct EditorTheme {
    pub font_family: SharedString,
    pub mono_family: SharedString,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub background: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub caret: Hsla,
    pub selection: Hsla,
    pub link: Hsla,
    pub code_background: Hsla,
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

/// Événement émis par l'éditeur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    /// Le contenu du document a changé.
    Changed,
    /// Le texte tapé après `[[` a changé : l'application répond par
    /// [`EditorView::set_mention_items`].
    MentionQuery(String),
    /// Ctrl+clic sur une mention : identifiant de sa cible (par exemple `document:<uuid>`).
    MentionClicked(String),
}

/// Texte en cours de composition (IME), dans un seul bloc ; décalages en caractères.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Marked {
    pub block: usize,
    pub start: usize,
    pub end: usize,
}

/// Menu `/` ouvert : bloc concerné, décalage du `/` et ligne choisie.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slash {
    pub block: usize,
    pub start: usize,
    pub selected: usize,
}

/// Aperçu d'un bloc inconnu (par exemple une vue de base de données), fourni par l'application :
/// titre, sous-titre et petit tableau.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmbedCard {
    pub title: String,
    pub subtitle: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Menu `[[` ouvert : bloc concerné, décalage des `[[` et ligne choisie.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MentionMenu {
    pub block: usize,
    pub start: usize,
    pub selected: usize,
}

/// Cible proposée par le menu `[[` (fournie par l'application).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionItem {
    /// Identifiant stocké dans la mention (par exemple `document:<uuid>`).
    pub id: String,
    /// Titre affiché dans le texte.
    pub title: String,
    /// Détail affiché sous le titre (chemin, aperçu).
    pub detail: String,
}

/// Commandes du menu `/` : libellé, mots-clés de recherche, effet.
pub(crate) const SLASH_COMMANDS: [(&str, &str, BlockCommand); 10] = [
    ("Texte", "texte paragraphe text", BlockCommand::Paragraph),
    ("Titre 1", "titre heading h1 #", BlockCommand::Heading(1)),
    ("Titre 2", "titre heading h2 ##", BlockCommand::Heading(2)),
    ("Titre 3", "titre heading h3 ###", BlockCommand::Heading(3)),
    (
        "Liste à puces",
        "liste puces bullet ul -",
        BlockCommand::BulletList,
    ),
    (
        "Liste numérotée",
        "liste numerotee ordered ol 1.",
        BlockCommand::OrderedList,
    ),
    (
        "Liste de tâches",
        "liste taches todo checkbox [ ]",
        BlockCommand::TaskList,
    ),
    ("Citation", "citation quote >", BlockCommand::Quote),
    ("Bloc de code", "code bloc ```", BlockCommand::CodeBlock),
    ("Séparateur", "separateur ligne hr ---", BlockCommand::Rule),
];

/// Composant d'édition : un document, sa sélection et son historique.
#[allow(clippy::struct_excessive_bools)]
pub struct EditorView {
    pub(crate) editor: Editor,
    pub(crate) theme: EditorTheme,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layouts: Vec<TextLayout>,
    pub(crate) painted: bool,
    pub(crate) selecting: bool,
    pub(crate) marked: Option<Marked>,
    pub(crate) goal_x: Option<Pixels>,
    pub(crate) slash: Option<Slash>,
    pub(crate) mention: Option<MentionMenu>,
    pub(crate) mention_items: Vec<MentionItem>,
    mention_query: String,
    /// Aperçus des blocs inconnus, par valeur de leur attribut `viewId`.
    pub(crate) embeds: std::collections::HashMap<String, EmbedCard>,
    /// Bloc de code dont le bouton « Copier » vient d'être utilisé.
    pub(crate) copied: Option<usize>,
    /// Défilement vertical du document.
    pub(crate) scroll: ScrollHandle,
    /// Le prochain dessin doit ramener le curseur dans la zone visible.
    pub(crate) reveal_caret: bool,
    /// Phase visible du clignotement du curseur.
    pub(crate) caret_visible: bool,
    /// L'éditeur avait le focus au dernier dessin (le clignotement s'arrête sans focus).
    pub(crate) focused: bool,
    _blink: Task<()>,
}

/// Demi-période du clignotement du curseur.
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
/// Marge (px) gardée autour du curseur lorsqu'on le ramène dans la zone visible.
const REVEAL_MARGIN: f32 = 24.0;

impl EventEmitter<EditorEvent> for EditorView {}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// --- Conversions de décalages -----------------------------------------------------------------

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
    /// Ouvre un document.
    pub fn new(doc: Node, theme: EditorTheme, cx: &mut Context<Self>) -> Self {
        Self {
            editor: Editor::new(doc),
            theme,
            focus_handle: cx.focus_handle(),
            layouts: Vec::new(),
            painted: false,
            selecting: false,
            marked: None,
            goal_x: None,
            slash: None,
            mention: None,
            mention_items: Vec::new(),
            mention_query: String::new(),
            embeds: std::collections::HashMap::new(),
            copied: None,
            scroll: ScrollHandle::new(),
            reveal_caret: false,
            caret_visible: true,
            focused: false,
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

    /// Le curseur vient de bouger : le montrer sans attendre le clignotement et le ramener dans la vue.
    fn caret_moved(&mut self) {
        self.caret_visible = true;
        self.reveal_caret = true;
    }

    /// Ramène le curseur dans la zone visible du défilement ; vrai si le défilement a changé.
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

    /// Phase visible du clignotement (tests et intégrations).
    #[must_use]
    pub fn caret_visible_for_tests(&self) -> bool {
        self.caret_visible
    }

    /// Décalage vertical du défilement (tests et intégrations).
    #[must_use]
    pub fn scroll_offset_y(&self) -> Pixels {
        self.scroll.offset().y
    }

    /// Document courant.
    #[must_use]
    pub fn doc(&self) -> &Node {
        self.editor.doc()
    }

    /// Moteur d'édition (lecture : sélection, état des marques, historique…).
    #[must_use]
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Remplace le document (l'historique est effacé).
    pub fn set_doc(&mut self, doc: Node, cx: &mut Context<Self>) {
        self.editor = Editor::new(doc);
        self.marked = None;
        self.painted = false;
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: EditorTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Donne le focus clavier à l'éditeur.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    pub(crate) fn changed(&mut self, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.caret_moved();
        self.sync_slash();
        self.sync_mention(cx);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Place le curseur sans passer par la souris ni le clavier (tests et intégrations).
    pub fn set_caret_for_tests(&mut self, pos: Pos, cx: &mut Context<Self>) {
        self.editor.set_selection(Selection::caret(pos));
        cx.notify();
    }

    /// Fixe la sélection (tests et intégrations).
    pub fn select_for_tests(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.editor.set_selection(selection);
        cx.notify();
    }

    /// Haut et bord gauche du bloc de texte `block`, une fois l'éditeur dessiné.
    #[must_use]
    pub fn block_origin_for_tests(&self, block: usize) -> Option<(Pixels, Pixels)> {
        if !self.ready() {
            return None;
        }
        let bounds = self.layouts.get(block)?.bounds();
        Some((bounds.top(), bounds.left()))
    }

    /// Copie le texte du bloc de code `block` dans le presse-papiers (bouton « Copier »).
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

    /// Bloc dont le bouton « Copier » est dans l'état « Copié » (tests et intégrations).
    #[must_use]
    pub fn copied_block(&self) -> Option<usize> {
        self.copied
    }

    // --- Menu `/` ---

    /// Texte tapé après le `/` (filtre du menu).
    fn slash_query(&self) -> Option<String> {
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

    /// Commandes correspondant au filtre.
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

    /// Libellés des commandes proposées (tests et intégrations).
    #[must_use]
    pub fn slash_items_for_tests(&self) -> Vec<String> {
        self.slash_items()
            .into_iter()
            .map(|(label, _)| label.to_owned())
            .collect()
    }

    /// Le menu `/` est-il ouvert ?
    #[must_use]
    pub fn slash_open(&self) -> bool {
        self.slash.is_some()
    }

    /// Ferme le menu s'il n'a plus de raison d'être (curseur ailleurs, espace tapé, `/` effacé).
    fn sync_slash(&mut self) {
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

    /// Ouvre le menu quand un `/` vient d'être tapé en début de mot (hors bloc de code).
    fn maybe_open_slash(&mut self) {
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

    /// Applique la commande `index` du menu : retire `/filtre` puis convertit le bloc.
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

    /// Déplace la ligne choisie du menu ; `false` si le menu n'est pas ouvert.
    fn slash_step(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
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

    // --- Menu `[[` ---

    /// Texte tapé après `[[` tant que le menu reste pertinent.
    fn mention_query_now(&self) -> Option<String> {
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

    /// Ouvre le menu quand `[[` vient d'être tapé (hors bloc de code).
    fn maybe_open_mention(&mut self) {
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

    /// Ferme le menu hors contexte ; sinon signale la nouvelle requête à l'application.
    fn sync_mention(&mut self, cx: &mut Context<Self>) {
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

    /// Cibles à proposer dans le menu `[[` (réponse à [`EditorEvent::MentionQuery`]).
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

    /// Aperçus à afficher à la place des blocs inconnus qui portent un attribut `viewId`.
    pub fn set_embeds(
        &mut self,
        embeds: std::collections::HashMap<String, EmbedCard>,
        cx: &mut Context<Self>,
    ) {
        self.embeds = embeds;
        cx.notify();
    }

    /// Le menu `[[` est-il ouvert ?
    #[must_use]
    pub fn mention_open(&self) -> bool {
        self.mention.is_some()
    }

    /// Insère la cible `index` du menu à la place de `[[requête`.
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

    fn mention_step(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
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

    // --- Géométrie ---

    fn ready(&self) -> bool {
        self.painted && self.layouts.len() == self.editor.block_count()
    }

    fn block_bytes(&self, block: usize, chars: usize) -> usize {
        chars_to_bytes(&self.editor.block_text(block), chars)
    }

    /// Position la plus proche d'un point de la fenêtre.
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

    fn caret_point(&self, pos: Pos) -> Option<Point<Pixels>> {
        if !self.ready() {
            return None;
        }
        self.layouts[pos.block].position_for_index(self.block_bytes(pos.block, pos.offset))
    }

    /// Rectangle du curseur (pour le dessin et la fenêtre de candidats de l'IME).
    pub(crate) fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        let head = self.editor.selection().head;
        let origin = self.caret_point(head)?;
        Some(Bounds::new(
            origin,
            size(px(1.5), self.layouts[head.block].line_height()),
        ))
    }

    /// Rectangles de la sélection, ligne visuelle par ligne visuelle.
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

    // --- Décalages UTF-16 globaux (texte du document, blocs séparés par un saut de ligne) ---

    fn global_utf16(&self, pos: Pos) -> usize {
        let before: usize = (0..pos.block)
            .map(|b| utf16_len(&self.editor.block_text(b)) + 1)
            .sum();
        before + chars_to_utf16(&self.editor.block_text(pos.block), pos.offset)
    }

    fn pos_from_utf16(&self, mut offset: usize) -> Pos {
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

    fn full_text(&self) -> String {
        (0..self.editor.block_count())
            .map(|b| self.editor.block_text(b))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // --- Actions : navigation ---

    fn nav(&mut self, cx: &mut Context<Self>, run: impl FnOnce(&mut Editor)) {
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

    /// Déplacement vertical d'une ligne visuelle, en gardant la colonne de départ.
    fn vertical(&mut self, direction: f32, extend: bool, cx: &mut Context<Self>) {
        let head = self.editor.selection().head;
        let Some(origin) = self.caret_point(head) else {
            return;
        };
        let line = self.layouts[head.block].line_height();
        let x = *self.goal_x.get_or_insert(origin.x);
        let target = point(x, origin.y + line / 2.0 + line * direction);
        let at_edge = (direction < 0.0
            && head.block == 0
            && origin.y - self.layouts[0].bounds().top() < line / 2.0)
            || (direction > 0.0 && head.block + 1 == self.layouts.len() && {
                let bounds = self.layouts[head.block].bounds();
                bounds.bottom() - origin.y < line * 1.5
            });
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

    // --- Actions : édition ---

    fn edit(&mut self, cx: &mut Context<Self>, run: impl FnOnce(&mut Editor)) {
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

    // --- Presse-papiers ---

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

    // --- Souris ---

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
        self.selecting = true;
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
}

// --- IME -------------------------------------------------------------------------------------

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
