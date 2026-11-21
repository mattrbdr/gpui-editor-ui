//! Editor rendering: block tree, styled text, and caret and selection painted in an overlay.

use std::ops::Range;

use gpui::{
    AnyElement, Context, ElementInputHandler, FontStyle, FontWeight, HighlightStyle, Hsla,
    MouseButton, ObjectFit, ParentElement, PathBuilder, Render, StatefulInteractiveElement,
    StrikethroughStyle, Styled, StyledImage, StyledText, TextLayout, UnderlineStyle, Window,
    canvas, deferred, div, fill, img, point, prelude::*, px,
};
use gpui_editor_core::{Kind, MarkKind, Node};

use crate::{
    keys,
    view::{EditorView, Marked, chars_to_bytes},
};

/// Appearance of a text block.
#[derive(Clone, Copy)]
struct BlockLook {
    size: gpui::Pixels,
    line_height: gpui::Pixels,
    weight: Option<FontWeight>,
    code_block: bool,
}

const HEADING_SCALE: [f32; 6] = [1.9, 1.6, 1.35, 1.2, 1.1, 1.0];

/// Text of a block and mark styles, as byte offsets.
fn text_and_styles(
    node: &Node,
    theme: &crate::EditorTheme,
    code_block: bool,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut styles = Vec::new();
    for child in &node.content {
        match &child.kind {
            Kind::Text => {
                let part = child.text.as_deref().unwrap_or_default();
                let start = text.len();
                text.push_str(part);
                if code_block || part.is_empty() {
                    continue;
                }
                let mut style = HighlightStyle::default();
                for mark in &child.marks {
                    match &mark.kind {
                        MarkKind::Bold => style.font_weight = Some(FontWeight::BOLD),
                        MarkKind::Italic => style.font_style = Some(FontStyle::Italic),
                        MarkKind::Strike => {
                            style.strikethrough = Some(StrikethroughStyle {
                                thickness: px(1.0),
                                color: None,
                            });
                        }
                        MarkKind::Underline => {
                            style.underline = Some(UnderlineStyle {
                                thickness: px(1.0),
                                color: None,
                                wavy: false,
                            });
                        }
                        MarkKind::Code => style.background_color = Some(theme.code_background),
                        MarkKind::Link => {
                            style.color = Some(theme.link);
                            style.underline = Some(UnderlineStyle {
                                thickness: px(1.0),
                                color: Some(theme.link),
                                wavy: false,
                            });
                        }
                        MarkKind::Other(name) if name == "mention" => {
                            style.color = Some(theme.link);
                            style.background_color = Some(theme.code_background);
                        }
                        MarkKind::Other(_) => {}
                    }
                }
                if style != HighlightStyle::default() {
                    styles.push((start..text.len(), style));
                }
            }
            Kind::HardBreak => text.push('\n'),
            Kind::Other(name) if name == "mention" => {
                let attrs = &child.attrs;
                let label = ["label", "id"]
                    .iter()
                    .find_map(|key| {
                        attrs
                            .get(*key)
                            .and_then(|v| v.as_str())
                            .filter(|v| !v.is_empty())
                    })
                    .unwrap_or("Link");
                let start = text.len();
                text.push_str("[[");
                text.push_str(label);
                text.push_str("]]");
                styles.push((
                    start..text.len(),
                    HighlightStyle {
                        color: Some(theme.link),
                        background_color: Some(theme.code_background),
                        ..HighlightStyle::default()
                    },
                ));
            }
            // Unknown inline node: a single replacement character, as in the model.
            Kind::Other(_) => text.push('\u{fffc}'),
            _ => {}
        }
    }
    (text, styles)
}

/// Adds an underline on `range` (text being composed) by splitting the styles.
fn with_underline(
    styles: &[(Range<usize>, HighlightStyle)],
    range: &Range<usize>,
    len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut cuts = vec![0, len, range.start.min(len), range.end.min(len)];
    for (r, _) in styles {
        cuts.extend([r.start, r.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Vec::new();
    for pair in cuts.windows(2) {
        let piece = pair[0]..pair[1];
        let mut style = styles
            .iter()
            .find(|(r, _)| r.start <= piece.start && piece.end <= r.end)
            .map(|(_, s)| *s)
            .unwrap_or_default();
        if range.start <= piece.start && piece.end <= range.end {
            style.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: None,
                wavy: false,
            });
        }
        if style != HighlightStyle::default() && !piece.is_empty() {
            out.push((piece, style));
        }
    }
    out
}

impl EditorView {
    fn marked_bytes(&self, block: usize, text: &str) -> Option<Range<usize>> {
        let Marked {
            block: marked_block,
            start,
            end,
        } = self.marked?;
        (marked_block == block).then(|| chars_to_bytes(text, start)..chars_to_bytes(text, end))
    }

    fn text_block(
        &self,
        node: &Node,
        counter: &mut usize,
        layouts: &mut Vec<TextLayout>,
        look: BlockLook,
        cx: &Context<Self>,
    ) -> AnyElement {
        let BlockLook {
            size,
            line_height,
            weight,
            code_block,
        } = look;
        let index = *counter;
        *counter += 1;
        let (text, mut styles) = text_and_styles(node, &self.theme, code_block);
        if let Some(range) = self.marked_bytes(index, &text) {
            styles = with_underline(&styles, &range, text.len());
        }
        // An empty block keeps one line: index 0 stays valid for the caret.
        let shown = if text.is_empty() {
            " ".to_owned()
        } else {
            text
        };
        let shaped = StyledText::new(shown).with_highlights(styles);
        layouts.push(shaped.layout().clone());
        let mut element = div()
            .w_full()
            .min_h(line_height)
            .text_size(size)
            .line_height(line_height)
            .text_color(self.theme.text)
            .child(shaped);
        if let Some(weight) = weight {
            element = element.font_weight(weight);
        }
        if self.slash.is_some_and(|slash| slash.block == index) {
            return div()
                .relative()
                .w_full()
                .child(element)
                .child(deferred(self.slash_menu(line_height, cx)).with_priority(1))
                .into_any_element();
        }
        if self.mention.is_some_and(|mention| mention.block == index) {
            return div()
                .relative()
                .w_full()
                .child(element)
                .child(deferred(self.mention_menu(line_height, cx)).with_priority(1))
                .into_any_element();
        }
        element.into_any_element()
    }

    /// `/` menu below the current block.
    fn slash_menu(&self, line_height: gpui::Pixels, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let selected = self.slash.map_or(0, |slash| slash.selected);
        let items = self.slash_items();
        let rows: Vec<AnyElement> = if items.is_empty() {
            vec![
                div()
                    .px_2()
                    .py_1()
                    .text_color(theme.muted)
                    .child("No results")
                    .into_any_element(),
            ]
        } else {
            items
                .iter()
                .enumerate()
                .map(|(index, (label, _))| {
                    div()
                        .id(("gpui-editor-slash", index))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_color(theme.text)
                        .cursor_pointer()
                        .when(index == selected, |row| row.bg(theme.selection))
                        .hover(|row| row.bg(theme.code_background))
                        .child(*label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.run_slash(index, cx);
                            }),
                        )
                        .into_any_element()
                })
                .collect()
        };
        div()
            .absolute()
            .top(line_height + px(4.0))
            .left(px(0.0))
            .w(px(260.0))
            .p_1()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .text_size(theme.font_size)
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }

    /// Preview of an unknown block: title, subtitle and compact table.
    fn embed_card(&self, card: &crate::EmbedCard) -> AnyElement {
        let theme = &self.theme;
        let header = div()
            .flex()
            .gap_3()
            .pb_1()
            .border_b_1()
            .border_color(theme.border)
            .text_size(px(12.0))
            .text_color(theme.muted)
            .children(card.columns.iter().enumerate().map(|(index, column)| {
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(index == 0, |c| c.flex_grow(2.0))
                    .child(column.clone())
            }));
        let rows = card.rows.iter().map(|row| {
            div()
                .flex()
                .gap_3()
                .py_0p5()
                .children(row.iter().enumerate().map(|(index, cell)| {
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .when(index == 0, |c| c.flex_grow(2.0))
                        .child(cell.clone())
                }))
        });
        div()
            .w_full()
            .p_3()
            .my_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.code_background)
            .text_size(theme.font_size)
            .text_color(theme.text)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(card.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(card.subtitle.clone()),
            )
            .when(!card.columns.is_empty(), |c| c.child(header))
            .children(rows)
            .into_any_element()
    }

    /// `[[` menu: targets offered by the application, with their path in muted text.
    fn mention_menu(&self, line_height: gpui::Pixels, cx: &Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let selected = self.mention.map_or(0, |mention| mention.selected);
        let rows: Vec<AnyElement> = if self.mention_items.is_empty() {
            vec![
                div()
                    .px_2()
                    .py_1()
                    .text_color(theme.muted)
                    .child("No results")
                    .into_any_element(),
            ]
        } else {
            self.mention_items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    div()
                        .id(("gpui-editor-mention", index))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_color(theme.text)
                        .cursor_pointer()
                        .when(index == selected, |row| row.bg(theme.selection))
                        .hover(|row| row.bg(theme.code_background))
                        .child(div().child(item.title.clone()))
                        .when(!item.detail.is_empty(), |row| {
                            row.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.muted)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(item.detail.clone()),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.run_mention(index, cx);
                            }),
                        )
                        .into_any_element()
                })
                .collect()
        };
        div()
            .absolute()
            .top(line_height + px(4.0))
            .left(px(0.0))
            .w(px(340.0))
            .max_h(px(260.0))
            .overflow_hidden()
            .p_1()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .text_size(theme.font_size)
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }

    fn build_children(
        &self,
        nodes: &[Node],
        counter: &mut usize,
        layouts: &mut Vec<TextLayout>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        nodes
            .iter()
            .filter_map(|node| self.build_node(node, counter, layouts, cx))
            .collect()
    }

    /// Code block: background, monospace font and "Copy" button.
    fn build_code_block(
        &self,
        node: &Node,
        counter: &mut usize,
        layouts: &mut Vec<TextLayout>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let index = *counter;
        let copied = self.copied == Some(index);
        div()
            .relative()
            .w_full()
            .p_3()
            .rounded_md()
            .bg(theme.code_background)
            .font_family(theme.mono_family.clone())
            .child(self.text_block(
                node,
                counter,
                layouts,
                BlockLook {
                    size: theme.font_size,
                    line_height: theme.line_height,
                    weight: None,
                    code_block: true,
                },
                cx,
            ))
            .child(
                div()
                    .id(("gpui-editor-copy", index))
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .px_2()
                    .py(px(2.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .text_size(px(12.0))
                    .font_family(theme.font_family.clone())
                    .text_color(if copied { theme.link } else { theme.muted })
                    .cursor_pointer()
                    .hover(|button| button.text_color(theme.text))
                    .child(if copied { "Copied ✓" } else { "Copy" })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.copy_code_block(index, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    fn build_node(
        &self,
        node: &Node,
        counter: &mut usize,
        layouts: &mut Vec<TextLayout>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = &self.theme;
        Some(match &node.kind {
            Kind::Paragraph => self.text_block(
                node,
                counter,
                layouts,
                BlockLook {
                    size: theme.font_size,
                    line_height: theme.line_height,
                    weight: None,
                    code_block: false,
                },
                cx,
            ),
            Kind::Heading => {
                let level = node
                    .attrs
                    .get("level")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, 6);
                let scale = HEADING_SCALE[usize::try_from(level - 1).unwrap_or(0)];
                self.text_block(
                    node,
                    counter,
                    layouts,
                    BlockLook {
                        size: theme.font_size * scale,
                        line_height: theme.line_height * scale.max(1.0),
                        weight: Some(FontWeight::BOLD),
                        code_block: false,
                    },
                    cx,
                )
            }
            Kind::CodeBlock => self.build_code_block(node, counter, layouts, cx),
            Kind::Blockquote => div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .pl_4()
                .border_l_2()
                .border_color(theme.border)
                .children(self.build_children(&node.content, counter, layouts, cx))
                .into_any_element(),
            Kind::BulletList | Kind::OrderedList | Kind::TaskList => {
                self.build_list(node, counter, layouts, cx)
            }
            Kind::HorizontalRule => div()
                .w_full()
                .my_2()
                .h(px(1.0))
                .bg(theme.border)
                .into_any_element(),
            Kind::ListItem | Kind::TaskItem | Kind::Doc => div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .children(self.build_children(&node.content, counter, layouts, cx))
                .into_any_element(),
            Kind::Text | Kind::HardBreak => return None,
            Kind::Other(_)
                if node
                    .attrs
                    .get("viewId")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| self.embeds.contains_key(id)) =>
            {
                let key = node
                    .attrs
                    .get("viewId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                self.embed_card(&self.embeds[key])
            }
            Kind::Other(name) => self.unknown_block(name, node),
        })
    }

    /// A node the editor does not know: an image (loaded through the image resolver) or a
    /// `[name]` marker. The node is kept in the document either way.
    fn unknown_block(&self, name: &str, node: &Node) -> AnyElement {
        let theme = &self.theme;
        let label = match node.attrs.get("alt").and_then(serde_json::Value::as_str) {
            Some(alt) if name == "image" => format!("[image: {alt}]"),
            _ => format!("[{name}]"),
        };
        let marker = {
            let muted = theme.muted;
            move || {
                div()
                    .w_full()
                    .text_color(muted)
                    .child(label.clone())
                    .into_any_element()
            }
        };
        let source = (name == "image")
            .then(|| node.attrs.get("src").and_then(serde_json::Value::as_str))
            .flatten()
            .and_then(|src| self.resolve_image(src));
        match source {
            Some(source) => div()
                .w_full()
                .child(
                    img(source)
                        .max_w_full()
                        .object_fit(ObjectFit::Contain)
                        .rounded(px(4.0))
                        .with_fallback(marker),
                )
                .into_any_element(),
            None => marker(),
        }
    }

    fn build_list(
        &self,
        list: &Node,
        counter: &mut usize,
        layouts: &mut Vec<TextLayout>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let start = list
            .attrs
            .get("start")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let mut rows = Vec::new();
        for (index, item) in list.content.iter().enumerate() {
            let first_block = *counter;
            let checked = item.kind == Kind::TaskItem
                && item
                    .attrs
                    .get("checked")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
            let marker: AnyElement = match (&list.kind, &item.kind) {
                (Kind::OrderedList, _) => {
                    format!("{}.", start + i64::try_from(index).unwrap_or(0)).into_any_element()
                }
                (_, Kind::TaskItem) => div()
                    .h(theme.line_height)
                    .flex()
                    .items_center()
                    .child(checkbox(
                        checked,
                        theme.caret,
                        theme.border,
                        theme.background,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.toggle_task_at(first_block, cx);
                            window.focus(&this.focus_handle, cx);
                        }),
                    )
                    .into_any_element(),
                _ => "•".into_any_element(),
            };
            let content = self.build_children(&item.content, counter, layouts, cx);
            rows.push(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(22.0))
                            .flex()
                            .justify_end()
                            .text_color(theme.muted)
                            .line_height(theme.line_height)
                            .child(marker),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .when(checked, |c| c.text_color(theme.muted))
                            .children(content),
                    ),
            );
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children(rows)
            .into_any_element()
    }
}

/// Drawn checkbox (rounded square, stroked check mark): independent of system fonts.
fn checkbox(checked: bool, accent: Hsla, border: Hsla, mark: Hsla) -> AnyElement {
    const SIDE: f32 = 16.0;
    let base = div()
        .id("gpui-editor-checkbox")
        .size(px(SIDE))
        .flex_none()
        .rounded(px(4.0))
        .cursor_pointer()
        .border_1();
    if checked {
        base.border_color(accent)
            .bg(accent)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| {
                        let at = |x: f32, y: f32| {
                            point(
                                bounds.origin.x + bounds.size.width * x,
                                bounds.origin.y + bounds.size.height * y,
                            )
                        };
                        let mut path = PathBuilder::stroke(px(2.0));
                        path.move_to(at(0.24, 0.53));
                        path.line_to(at(0.43, 0.71));
                        path.line_to(at(0.77, 0.31));
                        if let Ok(path) = path.build() {
                            window.paint_path(path, mark);
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    } else {
        base.border_color(border)
            .hover(move |b| b.border_color(accent))
            .into_any_element()
    }
}

/// Absolute overlay: registers the input handler, paints the selection and the caret.
fn overlay(cx: &Context<EditorView>, selection: Hsla, caret_color: Hsla) -> AnyElement {
    let entity = cx.entity();
    canvas(
        |bounds, _, _| bounds,
        move |bounds, _, window, cx| {
            let focus = entity.read(cx).focus_handle.clone();
            window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
            let (quads, caret, visible) = {
                let view = entity.read(cx);
                (
                    view.selection_quads(),
                    view.caret_bounds(),
                    view.caret_visible,
                )
            };
            for quad in quads {
                window.paint_quad(fill(quad, selection));
            }
            let focused = focus.is_focused(window);
            if focused
                && visible
                && let Some(caret) = caret
            {
                window.paint_quad(fill(caret, caret_color));
            }
            entity.update(cx, |view, cx| {
                view.painted = true;
                view.focused = focused;
                // Scrolling only takes effect on the next paint: request one.
                if view.reveal_caret_now() {
                    cx.notify();
                }
            });
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut layouts = Vec::new();
        let mut counter = 0;
        let blocks =
            self.build_children(&self.editor.doc().content, &mut counter, &mut layouts, cx);
        self.layouts = layouts;
        let overlay = overlay(cx, self.theme.selection, self.theme.caret);

        div()
            .key_context(keys::CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(self.theme.background)
            .font_family(self.theme.font_family.clone())
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_word_left))
            .on_action(cx.listener(Self::on_word_right))
            .on_action(cx.listener(Self::on_select_word_left))
            .on_action(cx.listener(Self::on_select_word_right))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_select_page_up))
            .on_action(cx.listener(Self::on_select_page_down))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_select_home))
            .on_action(cx.listener(Self::on_select_end))
            .on_action(cx.listener(Self::on_doc_start))
            .on_action(cx.listener(Self::on_doc_end))
            .on_action(cx.listener(Self::on_select_doc_start))
            .on_action(cx.listener(Self::on_select_doc_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_enter))
            .on_action(cx.listener(Self::on_hard_break))
            .on_action(cx.listener(Self::on_indent))
            .on_action(cx.listener(Self::on_outdent))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_bold))
            .on_action(cx.listener(Self::on_italic))
            .on_action(cx.listener(Self::on_underline))
            .on_action(cx.listener(Self::on_strike))
            .on_action(cx.listener(Self::on_code))
            .on_action(cx.listener(Self::on_heading1))
            .on_action(cx.listener(Self::on_heading2))
            .on_action(cx.listener(Self::on_heading3))
            .on_action(cx.listener(Self::on_code_block))
            .on_action(cx.listener(Self::on_bullet_list))
            .on_action(cx.listener(Self::on_ordered_list))
            .on_action(cx.listener(Self::on_task_list))
            .on_action(cx.listener(Self::on_toggle_task))
            .on_action(cx.listener(Self::on_link))
            .on_action(cx.listener(Self::on_cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                div()
                    .id("gpui-editor-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(blocks)
                            .child(overlay),
                    ),
            )
    }
}
