//! État de l'éditeur GPUI : sélection, souris, presse-papiers, IME.

mod actions;
mod geometry;
mod ime;
mod menus;
mod mouse;

use std::time::Duration;

use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, Hsla, Pixels, ScrollHandle,
    SharedString, Task, TextLayout, Window, hsla, point, px,
};
use gpui_editor_core::{BlockCommand, Editor, Node};
#[cfg(feature = "test-support")]
use gpui_editor_core::{Pos, Selection};

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
    /// Ctrl+K : l'application doit demander l'adresse d'un lien. Contient l'adresse actuelle si le
    /// curseur est dans un lien (qui est alors sélectionné en entier), sinon `None` (le mot sous le curseur
    /// est sélectionné s'il n'y a pas de sélection). Répondre par [`EditorView::apply_link`].
    LinkRequested(Option<String>),
    /// Ctrl+clic sur un lien : son adresse.
    LinkClicked(String),
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
    /// La boucle de défilement automatique de la sélection tourne déjà.
    autoscrolling: bool,
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
    #[cfg(feature = "test-support")]
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
        // Les dispositions de texte datent d'avant la modification : plus de `hit_test` dessus avant le prochain dessin.
        self.painted = false;
        self.goal_x = None;
        self.caret_moved();
        self.sync_slash();
        self.sync_mention(cx);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Place le curseur sans passer par la souris ni le clavier (tests et intégrations).
    #[cfg(feature = "test-support")]
    pub fn set_caret_for_tests(&mut self, pos: Pos, cx: &mut Context<Self>) {
        self.editor.set_selection(Selection::caret(pos));
        cx.notify();
    }

    /// Fixe la sélection (tests et intégrations).
    #[cfg(feature = "test-support")]
    pub fn select_for_tests(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.editor.set_selection(selection);
        cx.notify();
    }

    /// Haut et bord gauche du bloc de texte `block`, une fois l'éditeur dessiné.
    #[must_use]
    #[cfg(feature = "test-support")]
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
}
