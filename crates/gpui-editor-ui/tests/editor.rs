use gpui::{
    AnyWindowHandle, EntityInputHandler, Modifiers, TestAppContext, VisualTestContext,
    WindowHandle, point, px,
};
use gpui_editor_core::{Kind, Node, Pos, Selection};
use gpui_editor_ui::{EditorEvent, EditorLabels, EditorTheme, EditorView, bind_keys};

fn doc(blocks: &[&str]) -> Node {
    let content = blocks
        .iter()
        .map(|t| {
            format!(
                r#"{{"type":"paragraph","content":[{{"type":"text","text":{}}}]}}"#,
                serde_json::to_string(t).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Node::from_json_str(&format!(r#"{{"type":"doc","content":[{content}]}}"#)).unwrap()
}

fn open(cx: &mut TestAppContext, node: Node) -> WindowHandle<EditorView> {
    cx.update(bind_keys);
    let window = cx.add_window(|_, cx| EditorView::new(node, EditorTheme::default(), cx));
    window
        .update(cx, |view, window, cx| view.focus(window, cx))
        .unwrap();
    cx.run_until_parked();
    window
}

fn any(window: WindowHandle<EditorView>) -> AnyWindowHandle {
    window.into()
}

fn texts(cx: &mut TestAppContext, window: WindowHandle<EditorView>) -> Vec<String> {
    window
        .read_with(cx, |view, _| {
            let editor = view.editor();
            (0..editor.block_count())
                .map(|i| editor.block_text(i))
                .collect()
        })
        .unwrap()
}

fn caret(cx: &mut TestAppContext, window: WindowHandle<EditorView>) -> Selection {
    window
        .read_with(cx, |view, _| view.editor().selection())
        .unwrap()
}

fn set_caret(
    cx: &mut TestAppContext,
    window: WindowHandle<EditorView>,
    block: usize,
    offset: usize,
) {
    window
        .update(cx, |view, _, cx| {
            view.set_caret_for_tests(Pos { block, offset }, cx);
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn typed_text_reaches_the_document_through_the_input_handler(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["ab"]));
    set_caret(cx, window, 0, 1);
    cx.simulate_input(any(window), "é✓");
    assert_eq!(texts(cx, window), ["aé✓b"]);
    assert_eq!(
        caret(cx, window).head,
        Pos {
            block: 0,
            offset: 3
        }
    );
}

#[gpui::test]
fn the_editor_emits_changed_when_the_document_changes(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["a"]));
    let events = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = events.clone();
    let view = window.root(cx).unwrap();
    cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &EditorEvent, _| {
            if *event == EditorEvent::Changed {
                counter.set(counter.get() + 1);
            }
        })
        .detach();
    });
    cx.simulate_input(any(window), "x");
    assert!(events.get() >= 1);
}

#[gpui::test]
fn keyboard_actions_edit_and_navigate(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["abc"]));
    set_caret(cx, window, 0, 3);
    cx.simulate_keystrokes(any(window), "backspace");
    assert_eq!(texts(cx, window), ["ab"]);
    cx.simulate_keystrokes(any(window), "enter");
    cx.simulate_input(any(window), "xy");
    assert_eq!(texts(cx, window), ["ab", "xy"]);
    cx.simulate_keystrokes(any(window), "home");
    assert_eq!(
        caret(cx, window).head,
        Pos {
            block: 1,
            offset: 0
        }
    );
    cx.simulate_keystrokes(any(window), "backspace");
    assert_eq!(texts(cx, window), ["abxy"]);
    cx.simulate_keystrokes(any(window), "ctrl-z");
    assert_eq!(texts(cx, window), ["ab", "xy"]);
    cx.simulate_keystrokes(any(window), "ctrl-shift-z");
    assert_eq!(texts(cx, window), ["abxy"]);
}

#[gpui::test]
fn shift_arrows_select_and_ctrl_b_makes_the_selection_bold(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["abcdef"]));
    set_caret(cx, window, 0, 1);
    cx.simulate_keystrokes(any(window), "shift-right shift-right shift-right");
    assert_eq!(
        caret(cx, window),
        Selection {
            anchor: Pos {
                block: 0,
                offset: 1
            },
            head: Pos {
                block: 0,
                offset: 4
            }
        }
    );
    cx.simulate_keystrokes(any(window), "ctrl-b");
    let markdown = window
        .read_with(cx, |view, _| view.doc().to_markdown())
        .unwrap();
    assert_eq!(markdown, "a**bcd**ef");
}

#[gpui::test]
fn block_shortcuts_change_the_document_structure(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["Titre"]));
    set_caret(cx, window, 0, 0);
    cx.simulate_keystrokes(any(window), "ctrl-alt-2");
    let kind = window
        .read_with(cx, |view, _| view.editor().block_kind(0))
        .unwrap();
    assert_eq!(kind, Kind::Heading);
    cx.simulate_keystrokes(any(window), "ctrl-alt-2 ctrl-shift-8");
    let markdown = window
        .read_with(cx, |view, _| view.doc().to_markdown())
        .unwrap();
    assert_eq!(markdown, "- Titre");
}

#[gpui::test]
fn word_and_document_navigation_use_ctrl(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["un deux", "trois"]));
    set_caret(cx, window, 0, 0);
    cx.simulate_keystrokes(any(window), "ctrl-right");
    assert_eq!(
        caret(cx, window).head,
        Pos {
            block: 0,
            offset: 2
        }
    );
    cx.simulate_keystrokes(any(window), "ctrl-end");
    assert_eq!(
        caret(cx, window).head,
        Pos {
            block: 1,
            offset: 5
        }
    );
    cx.simulate_keystrokes(any(window), "ctrl-a");
    assert_eq!(
        window
            .read_with(cx, |v, _| v.editor().selected_text())
            .unwrap(),
        "un deux\ntrois"
    );
}

#[gpui::test]
fn copy_cut_and_paste_use_the_clipboard(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["abcdef"]));
    window
        .update(cx, |view, _, cx| {
            view.select_for_tests(
                Selection {
                    anchor: Pos {
                        block: 0,
                        offset: 1,
                    },
                    head: Pos {
                        block: 0,
                        offset: 4,
                    },
                },
                cx,
            );
        })
        .unwrap();
    cx.simulate_keystrokes(any(window), "ctrl-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|i| i.text()).as_deref(),
        Some("bcd")
    );
    cx.simulate_keystrokes(any(window), "ctrl-x");
    assert_eq!(texts(cx, window), ["aef"]);
    cx.simulate_keystrokes(any(window), "ctrl-v ctrl-v");
    assert_eq!(texts(cx, window), ["abcdbcdef"]);
}

#[gpui::test]
fn ime_composition_is_marked_then_committed_as_one_undo_step(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["a"]));
    set_caret(cx, window, 0, 1);
    let view = window.root(cx).unwrap();
    let mut vcx = VisualTestContext::from_window(any(window), cx);
    view.update_in(&mut vcx, |v, window, cx| {
        v.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
        assert_eq!(v.marked_text_range(window, cx), Some(1..2));
        v.replace_and_mark_text_in_range(None, "日", Some(1..1), window, cx);
        v.replace_text_in_range(None, "日本", window, cx);
        assert_eq!(v.marked_text_range(window, cx), None);
    });
    assert_eq!(texts(cx, window), ["a日本"]);
    cx.simulate_keystrokes(any(window), "ctrl-z");
    assert_eq!(texts(cx, window), ["a"]);
}

#[gpui::test]
fn the_input_handler_reports_the_selection_and_text_in_utf16(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["a🙂b", "cd"]));
    window
        .update(cx, |view, _, cx| {
            view.select_for_tests(
                Selection {
                    anchor: Pos {
                        block: 0,
                        offset: 1,
                    },
                    head: Pos {
                        block: 1,
                        offset: 1,
                    },
                },
                cx,
            );
        })
        .unwrap();
    let view = window.root(cx).unwrap();
    let mut vcx = VisualTestContext::from_window(any(window), cx);
    view.update_in(&mut vcx, |v, window, cx| {
        let selection = v.selected_text_range(true, window, cx).unwrap();
        assert_eq!(selection.range, 1..6);
        let mut adjusted = None;
        let text = v.text_for_range(1..6, &mut adjusted, window, cx).unwrap();
        assert_eq!(text, "🙂b\nc");
        assert_eq!(adjusted, Some(1..6));
    });
}

#[gpui::test]
fn clicking_places_the_caret_and_dragging_selects(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["première ligne", "seconde ligne"]));
    cx.run_until_parked();
    let (second_top, left) = window
        .read_with(cx, |view, _| view.block_origin_for_tests(1))
        .unwrap()
        .expect("the layout should exist");
    let mut vcx = VisualTestContext::from_window(any(window), cx);
    vcx.simulate_click(
        point(left + px(1.0), second_top + px(4.0)),
        Modifiers::default(),
    );
    let selection = window
        .read_with(&vcx, |view, _| view.editor().selection())
        .unwrap();
    assert_eq!(selection.head.block, 1);
    assert!(selection.is_collapsed());
}

#[gpui::test]
fn up_and_down_move_between_blocks_keeping_the_column(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["première", "seconde", "troisième"]));
    set_caret(cx, window, 0, 4);
    cx.simulate_keystrokes(any(window), "down");
    assert_eq!(caret(cx, window).head.block, 1);
    cx.simulate_keystrokes(any(window), "down");
    assert_eq!(caret(cx, window).head.block, 2);
    cx.simulate_keystrokes(any(window), "up up");
    assert_eq!(caret(cx, window).head.block, 0);
}

fn open_empty(cx: &mut TestAppContext) -> WindowHandle<EditorView> {
    open(cx, doc(&[""]))
}

fn slash_items(cx: &mut TestAppContext, window: WindowHandle<EditorView>) -> Vec<String> {
    window
        .read_with(cx, |view, _| view.slash_items_for_tests())
        .unwrap()
}

fn slash_open(cx: &mut TestAppContext, window: WindowHandle<EditorView>) -> bool {
    window.read_with(cx, |view, _| view.slash_open()).unwrap()
}

fn markdown(cx: &mut TestAppContext, window: WindowHandle<EditorView>) -> String {
    window
        .read_with(cx, |view, _| view.doc().to_markdown())
        .unwrap()
}

#[gpui::test]
fn markdown_typed_through_the_input_handler_is_converted(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    cx.simulate_input(any(window), "## Titre");
    assert_eq!(markdown(cx, window), "## Titre");
    cx.simulate_keystrokes(any(window), "enter");
    cx.simulate_input(any(window), "- un");
    assert_eq!(markdown(cx, window), "## Titre\n\n- un");
    cx.simulate_keystrokes(any(window), "enter");
    cx.simulate_input(any(window), "**gras** et `code`");
    assert_eq!(
        markdown(cx, window),
        "## Titre\n\n- un\n- **gras** et `code`"
    );
    cx.simulate_keystrokes(any(window), "ctrl-z");
    // Undo restores the text as typed just before the last conversion.
    assert!(
        texts(cx, window).last().unwrap().contains("`code`")
            || texts(cx, window).last().unwrap().contains("code")
    );
}

#[gpui::test]
fn a_fence_and_enter_open_a_code_block(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    cx.simulate_input(any(window), "```js");
    cx.simulate_keystrokes(any(window), "enter");
    cx.simulate_input(any(window), "let a = 1;");
    assert_eq!(markdown(cx, window), "```js\nlet a = 1;\n```");
}

#[gpui::test]
fn typing_a_slash_opens_a_filterable_menu_and_enter_applies_the_command(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    assert!(!slash_open(cx, window));
    cx.simulate_input(any(window), "/");
    assert!(slash_open(cx, window));
    assert_eq!(slash_items(cx, window).len(), 10);
    cx.simulate_input(any(window), "head");
    assert_eq!(
        slash_items(cx, window),
        ["Heading 1", "Heading 2", "Heading 3"]
    );
    cx.simulate_keystrokes(any(window), "down enter");
    assert!(!slash_open(cx, window));
    let kind = window
        .read_with(cx, |view, _| view.editor().block_kind(0))
        .unwrap();
    assert_eq!(kind, Kind::Heading);
    assert_eq!(texts(cx, window), [""]);
    cx.simulate_input(any(window), "Deux");
    assert_eq!(markdown(cx, window), "## Deux");
    cx.simulate_keystrokes(any(window), "ctrl-z ctrl-z");
    assert_eq!(texts(cx, window), ["/head"]);
}

#[gpui::test]
fn the_slash_menu_shows_and_filters_the_labels_of_the_application(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    window
        .update(cx, |view, _, cx| {
            view.set_labels(
                EditorLabels {
                    heading_1: "Titre 1".into(),
                    heading_2: "Titre 2".into(),
                    heading_3: "Titre 3".into(),
                    ..EditorLabels::default()
                },
                cx,
            );
        })
        .unwrap();
    cx.simulate_input(any(window), "/titre");
    assert_eq!(slash_items(cx, window), ["Titre 1", "Titre 2", "Titre 3"]);
}

#[gpui::test]
fn the_slash_menu_closes_on_escape_backspace_or_a_space_and_never_opens_mid_word(
    cx: &mut TestAppContext,
) {
    let window = open_empty(cx);
    cx.simulate_input(any(window), "/");
    cx.simulate_keystrokes(any(window), "escape");
    assert!(!slash_open(cx, window));
    assert_eq!(texts(cx, window), ["/"]);
    cx.simulate_keystrokes(any(window), "backspace");
    cx.simulate_input(any(window), "/");
    cx.simulate_keystrokes(any(window), "backspace");
    assert!(!slash_open(cx, window));
    cx.simulate_input(any(window), "/ab ");
    assert!(!slash_open(cx, window), "a space closes the menu");
    let window = open(cx, doc(&["http:"]));
    set_caret(cx, window, 0, 5);
    cx.simulate_input(any(window), "/");
    assert!(!slash_open(cx, window), "no menu in the middle of a word");
}

#[gpui::test]
fn with_no_match_enter_just_splits_the_block(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    cx.simulate_input(any(window), "/zzz");
    assert_eq!(slash_items(cx, window), Vec::<String>::new());
    cx.simulate_keystrokes(any(window), "enter");
    assert!(!slash_open(cx, window));
    assert_eq!(texts(cx, window), ["/zzz", ""]);
}

#[gpui::test]
fn the_copy_button_puts_the_code_block_text_on_the_clipboard(cx: &mut TestAppContext) {
    let node = Node::from_json_str(
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"avant"}]},{"type":"codeBlock","attrs":{"language":null},"content":[{"type":"text","text":"ls -la\ncd /tmp"}]}]}"#,
    )
    .unwrap();
    let window = open(cx, node);
    assert_eq!(
        window.read_with(cx, |view, _| view.copied_block()).unwrap(),
        None
    );
    window
        .update(cx, |view, _, cx| view.copy_code_block(1, cx))
        .unwrap();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("ls -la\ncd /tmp")
    );
    assert_eq!(
        window.read_with(cx, |view, _| view.copied_block()).unwrap(),
        Some(1)
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        window.read_with(cx, |view, _| view.copied_block()).unwrap(),
        None,
        "the \"Copied\" state clears by itself"
    );
}

fn mention_items() -> Vec<gpui_editor_ui::MentionItem> {
    ["Alpha", "Bêta"]
        .iter()
        .enumerate()
        .map(|(i, title)| gpui_editor_ui::MentionItem {
            id: format!("document:{i}"),
            title: (*title).to_owned(),
            detail: "Procédures".to_owned(),
        })
        .collect()
}

#[gpui::test]
fn double_brackets_open_a_menu_that_reports_queries_and_inserts_a_mention(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    let queries = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let sink = queries.clone();
    window
        .update(cx, |_, _, cx| {
            let entity = cx.entity();
            cx.subscribe(&entity, move |_, _, event: &EditorEvent, _| {
                if let EditorEvent::MentionQuery(query) = event {
                    sink.borrow_mut().push(query.clone());
                }
            })
            .detach();
        })
        .unwrap();
    cx.simulate_input(any(window), "voir [");
    assert!(
        !window.read_with(cx, |v, _| v.mention_open()).unwrap(),
        "a single bracket opens nothing"
    );
    cx.simulate_input(any(window), "[");
    assert!(window.read_with(cx, |v, _| v.mention_open()).unwrap());
    assert_eq!(
        *queries.borrow(),
        [""],
        "the empty query is reported on opening"
    );
    cx.simulate_input(any(window), "al");
    assert_eq!(*queries.borrow(), ["", "a", "al"]);
    window
        .update(cx, |view, _, cx| {
            view.set_mention_items(mention_items(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes(any(window), "down up down enter");
    assert!(!window.read_with(cx, |v, _| v.mention_open()).unwrap());
    assert_eq!(texts(cx, window), ["voir [[Bêta]]"]);
    let json = window
        .read_with(cx, |v, _| v.doc().to_json_string())
        .unwrap();
    assert_eq!(
        json,
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"voir "},{"type":"mention","attrs":{"id":"document:1","label":"Bêta","mentionSuggestionChar":"[["}}]}]}"#
    );
    cx.simulate_keystrokes(any(window), "ctrl-z");
    assert_eq!(texts(cx, window), ["voir [[al"]);
}

#[gpui::test]
fn the_mention_menu_closes_on_escape_a_closing_bracket_or_when_the_caret_leaves(
    cx: &mut TestAppContext,
) {
    let window = open_empty(cx);
    cx.simulate_input(any(window), "[[ab");
    assert!(window.read_with(cx, |v, _| v.mention_open()).unwrap());
    cx.simulate_keystrokes(any(window), "escape");
    assert!(!window.read_with(cx, |v, _| v.mention_open()).unwrap());
    cx.simulate_input(any(window), " [[x]");
    assert!(
        !window.read_with(cx, |v, _| v.mention_open()).unwrap(),
        "a closing bracket closes the menu"
    );
    let reopened = open_empty(cx);
    cx.simulate_input(any(reopened), "[[x");
    cx.simulate_keystrokes(any(reopened), "left left left");
    assert!(
        !reopened.read_with(cx, |v, _| v.mention_open()).unwrap(),
        "the caret left the query"
    );
}

#[gpui::test]
fn ctrl_click_on_a_mention_reports_its_target_but_a_plain_click_only_moves_the_caret(
    cx: &mut TestAppContext,
) {
    let source = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"document:42","label":"Cible","mentionSuggestionChar":"[["}},{"type":"text","text":" suite"}]}]}"#;
    let window = open(cx, Node::from_json_str(source).unwrap());
    let clicks = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let sink = clicks.clone();
    window
        .update(cx, |_, _, cx| {
            let entity = cx.entity();
            cx.subscribe(&entity, move |_, _, event: &EditorEvent, _| {
                if let EditorEvent::MentionClicked(id) = event {
                    sink.borrow_mut().push(id.clone());
                }
            })
            .detach();
        })
        .unwrap();
    cx.run_until_parked();
    let (top, left) = window
        .read_with(cx, |view, _| view.block_origin_for_tests(0))
        .unwrap()
        .expect("layout");
    let origin = point(left + px(24.0), top + px(6.0));
    let mut vcx = VisualTestContext::from_window(any(window), cx);
    vcx.simulate_click(
        origin,
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(*clicks.borrow(), ["document:42"]);
    vcx.simulate_click(origin, Modifiers::default());
    assert_eq!(
        clicks.borrow().len(),
        1,
        "a plain click does not follow the link"
    );
}

#[gpui::test]
fn moving_the_caret_far_down_scrolls_it_into_view(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..200).map(|i| format!("ligne {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let window = open(cx, doc(&refs));
    let offset = |cx: &mut TestAppContext| {
        window
            .read_with(cx, |view, _| view.scroll_offset_y())
            .unwrap()
    };
    assert_eq!(offset(cx), px(0.0), "nothing has scrolled at the start");
    cx.simulate_keystrokes(any(window), "ctrl-end");
    cx.run_until_parked();
    assert!(offset(cx) < px(0.0), "ctrl-end should scroll to the caret");
    let scrolled = offset(cx);
    cx.simulate_keystrokes(any(window), "ctrl-home");
    cx.run_until_parked();
    assert!(offset(cx) > scrolled, "ctrl-home should scroll back up");
}

#[gpui::test]
fn the_caret_blinks_only_while_focused_and_resets_on_typing(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["abc"]));
    let visible = |cx: &mut TestAppContext| {
        window
            .read_with(cx, |view, _| view.caret_visible_for_tests())
            .unwrap()
    };
    assert!(visible(cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(540));
    cx.run_until_parked();
    assert!(!visible(cx), "the caret turns off after a half-period");
    cx.simulate_input(any(window), "x");
    assert!(visible(cx), "typing turns it back on immediately");
}

#[gpui::test]
fn page_down_and_page_up_move_the_caret_by_a_screenful(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..200).map(|i| format!("ligne {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let window = open(cx, doc(&refs));
    set_caret(cx, window, 0, 0);
    cx.simulate_keystrokes(any(window), "pagedown");
    let down = caret(cx, window).head.block;
    assert!(down > 3, "pagedown should skip several lines, got {down}");
    cx.simulate_keystrokes(any(window), "pageup");
    assert!(
        caret(cx, window).head.block < down,
        "pageup should move back up"
    );
}

#[gpui::test]
fn dragging_below_the_viewport_scrolls_and_extends_the_selection(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..200).map(|i| format!("ligne {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let window = open(cx, doc(&refs));
    set_caret(cx, window, 0, 0);
    let moved = window
        .update(cx, |view, _, cx| {
            view.begin_selection_for_tests();
            let bottom = view.scroll_viewport_bottom_for_tests();
            view.autoscroll_for_tests(point(px(40.0), bottom + px(80.0)), cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(moved, "a pointer below the visible area scrolls");
    assert!(window.read_with(cx, |v, _| v.scroll_offset_y()).unwrap() < px(0.0));
    assert!(
        caret(cx, window).head.block > 0,
        "the selection extends downward"
    );
    assert_eq!(caret(cx, window).anchor.block, 0);
}

#[gpui::test]
fn ctrl_k_asks_for_a_link_and_apply_link_sets_it(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["voir la page"]));
    let requests = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Option<String>>::new()));
    let sink = requests.clone();
    let view = window.root(cx).unwrap();
    cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &EditorEvent, _| {
            if let EditorEvent::LinkRequested(current) = event {
                sink.borrow_mut().push(current.clone());
            }
        })
        .detach();
    });
    set_caret(cx, window, 0, 9); // inside "page"
    cx.simulate_keystrokes(any(window), "ctrl-k");
    assert_eq!(
        requests.borrow().as_slice(),
        [None],
        "no link yet: empty request"
    );
    assert_eq!(
        caret(cx, window).start().offset..caret(cx, window).end().offset,
        8..12,
        "the word is selected"
    );
    window
        .update(cx, |view, _, cx| {
            view.apply_link(Some("https://exemple.fr"), cx);
        })
        .unwrap();
    cx.simulate_keystrokes(any(window), "ctrl-k");
    assert_eq!(
        requests.borrow().last().cloned().flatten().as_deref(),
        Some("https://exemple.fr")
    );
}

#[gpui::test]
fn image_blocks_go_through_the_image_resolver(cx: &mut TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    let node = Node::from_json_str(
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"},{"type":"image","attrs":{"src":"inline.png","alt":"inline"}}]},{"type":"image","attrs":{"src":"block.png","alt":"A block"}},{"type":"image","attrs":{"src":"none.png","alt":"none"}}]}"#,
    )
    .unwrap();
    let before = node.to_json_string();
    let window = open(cx, node);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let seen = asked.clone();
    window
        .update(cx, |view, _, cx| {
            view.set_image_resolver(
                move |src| {
                    seen.borrow_mut().push(src.to_owned());
                    (src == "block.png").then(|| gpui::ImageSource::from("/tmp/block.png"))
                },
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
    let asked = asked.borrow();
    assert!(asked.iter().any(|src| src == "block.png"), "{asked:?}");
    assert!(asked.iter().any(|src| src == "none.png"), "{asked:?}");
    assert!(
        !asked.iter().any(|src| src == "inline.png"),
        "inline images stay markers: {asked:?}"
    );
    let after = window
        .read_with(cx, |view, _| view.doc().to_json_string())
        .unwrap();
    assert_eq!(
        after, before,
        "rendering an image never changes the document"
    );
}

#[test]
fn the_dark_theme_is_dark_and_keeps_the_default_fonts() {
    let (light, dark) = (EditorTheme::default(), EditorTheme::dark());
    assert!(dark.background.l < 0.2 && light.background.l > 0.8);
    assert!(dark.text.l > 0.8);
    assert_eq!(dark.font_family, light.font_family);
    assert_eq!(dark.font_size, light.font_size);
}

#[gpui::test]
fn long_documents_only_lay_out_the_blocks_near_the_viewport(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..2_000)
        .map(|i| format!("Paragraph {i} with some text to lay out."))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let window = open(cx, doc(&refs));
    let read = |cx: &mut TestAppContext| {
        window
            .read_with(cx, |view, _| {
                (
                    view.laid_out_blocks_for_tests(),
                    view.scroll_max_for_tests(),
                    view.scroll_viewport_bottom_for_tests(),
                )
            })
            .unwrap()
    };
    // The first frame lays everything out once to measure it.
    let (_, full_height, _) = read(cx);
    window.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.run_until_parked();
    let (laid_out, height, bottom) = read(cx);
    assert!(laid_out > 0 && laid_out < 400, "{laid_out} blocks laid out");
    assert_eq!(height, full_height, "the boxes keep the document's height");

    // Ctrl+End reaches a block that was not laid out and shows the caret.
    cx.simulate_keystrokes(any(window), "ctrl-end");
    cx.run_until_parked();
    // The scroll requested while painting takes effect on the next frame.
    window.update(cx, |_, _, cx| cx.notify()).unwrap();
    cx.run_until_parked();
    let (top, _) = window
        .read_with(cx, |view, _| view.block_origin_for_tests(1_999))
        .unwrap()
        .expect("the caret's block is laid out");
    assert!(
        top >= px(0.0) && top < bottom,
        "caret block at {top:?}, viewport ends at {bottom:?}"
    );
    cx.simulate_input(any(window), "!");
    assert!(texts(cx, window)[1_999].ends_with("out.!"));

    // Back to the start, and down by pages: the caret stays in laid-out blocks.
    cx.simulate_keystrokes(any(window), "ctrl-home");
    cx.run_until_parked();
    for _ in 0..3 {
        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        cx.run_until_parked();
        cx.simulate_keystrokes(any(window), "pagedown");
        cx.run_until_parked();
    }
    let head = caret(cx, window).head;
    assert!(head.block > 20, "three pages down: block {}", head.block);
    let origin = window
        .read_with(cx, |view, _| view.block_origin_for_tests(head.block))
        .unwrap();
    assert!(origin.is_some(), "the caret's block is laid out");
    let (_, height, _) = read(cx);
    assert_eq!(height, full_height, "editing kept the measured heights");
}

fn search(cx: &mut TestAppContext, window: WindowHandle<EditorView>, query: Option<&str>) {
    window
        .update(cx, |view, _, cx| {
            view.set_search(query.map(str::to_owned), cx);
        })
        .unwrap();
    cx.run_until_parked();
}

fn status(
    cx: &mut TestAppContext,
    window: WindowHandle<EditorView>,
) -> Option<(Option<usize>, usize)> {
    window
        .read_with(cx, |view, _| view.search_status())
        .unwrap()
}

fn highlights(cx: &mut TestAppContext, window: WindowHandle<EditorView>) -> usize {
    window
        .read_with(cx, |view, _| view.search_highlights_for_tests())
        .unwrap()
}

#[gpui::test]
fn searching_selects_the_next_match_and_steps_around_the_document(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["un chat", "deux chats", "Chat"]));
    set_caret(cx, window, 1, 0);
    search(cx, window, Some("chat"));
    assert_eq!(status(cx, window), Some((Some(1), 3)));
    assert_eq!(highlights(cx, window), 3);
    assert_eq!(
        caret(cx, window).start(),
        Pos {
            block: 1,
            offset: 5
        }
    );
    for expected in [2, 0, 1] {
        window
            .update(cx, |view, _, cx| view.search_step(true, cx))
            .unwrap();
        assert_eq!(status(cx, window), Some((Some(expected), 3)));
    }
    window
        .update(cx, |view, _, cx| view.search_step(false, cx))
        .unwrap();
    assert_eq!(status(cx, window), Some((Some(0), 3)));
    search(cx, window, None);
    assert_eq!(status(cx, window), None);
    assert_eq!(highlights(cx, window), 0);
}

#[gpui::test]
fn a_match_far_down_is_scrolled_into_view_and_highlights_follow_edits(cx: &mut TestAppContext) {
    let mut blocks = vec!["ligne"; 200];
    blocks.push("aiguille");
    let window = open(cx, doc(&blocks));
    search(cx, window, Some("aiguille"));
    let offset = window
        .read_with(cx, |view, _| view.scroll_offset_y())
        .unwrap();
    assert!(offset < px(0.0), "{offset:?}");
    assert_eq!(highlights(cx, window), 1);
    cx.simulate_input(any(window), "x");
    cx.run_until_parked();
    assert_eq!(status(cx, window), Some((None, 0)));
    cx.simulate_keystrokes(any(window), "ctrl-z");
    cx.run_until_parked();
    assert_eq!(status(cx, window).map(|(_, total)| total), Some(1));
    assert_eq!(highlights(cx, window), 1);
}

#[gpui::test]
fn replacing_moves_to_the_next_match_and_replace_all_is_one_undo_step(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["a a", "a"]));
    search(cx, window, Some("a"));
    let replaced = window
        .update(cx, |view, _, cx| view.replace_current("aa", cx))
        .unwrap();
    assert!(replaced);
    assert_eq!(texts(cx, window), ["aa a", "a"]);
    assert_eq!(
        caret(cx, window).start(),
        Pos {
            block: 0,
            offset: 3
        }
    );
    let count = window
        .update(cx, |view, _, cx| view.replace_all("b", cx))
        .unwrap();
    assert_eq!(count, 4);
    assert_eq!(texts(cx, window), ["bb b", "b"]);
    cx.run_until_parked();
    cx.simulate_keystrokes(any(window), "ctrl-z");
    assert_eq!(texts(cx, window), ["aa a", "a"]);
}

fn clipboard(cx: &mut TestAppContext) -> String {
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default()
}

#[gpui::test]
fn formatted_text_is_copied_as_markdown_and_pasted_back(cx: &mut TestAppContext) {
    let window = open(cx, Node::from_markdown("**gras**"));
    cx.simulate_keystrokes(any(window), "ctrl-a ctrl-c");
    assert_eq!(clipboard(cx), "**gras**");
    cx.simulate_keystrokes(any(window), "ctrl-end ctrl-v");
    assert_eq!(markdown(cx, window), "**grasgras**");
    cx.simulate_keystrokes(any(window), "ctrl-shift-v");
    assert_eq!(texts(cx, window), ["grasgras**gras**"]);
}

#[gpui::test]
fn markdown_from_elsewhere_is_interpreted_only_when_unambiguous(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("- un\r\n- deux".into()));
    cx.simulate_keystrokes(any(window), "ctrl-v");
    assert_eq!(markdown(cx, window), "- un\n- deux");
    let window = open_empty(cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("log 1\nlog 2".into()));
    cx.simulate_keystrokes(any(window), "ctrl-v");
    assert_eq!(texts(cx, window), ["log 1", "log 2"]);
}

#[gpui::test]
fn an_action_without_effect_does_not_report_a_change(cx: &mut TestAppContext) {
    let window = open(cx, doc(&["a"]));
    let changes = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = changes.clone();
    let view = window.root(cx).unwrap();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&view, move |_, event, _| {
            if *event == EditorEvent::Changed {
                seen.set(seen.get() + 1);
            }
        })
    });
    cx.simulate_keystrokes(any(window), "ctrl-z backspace tab");
    cx.run_until_parked();
    assert_eq!(changes.get(), 0);
    cx.simulate_keystrokes(any(window), "delete");
    cx.run_until_parked();
    assert_eq!(changes.get(), 1);
}

#[gpui::test]
fn the_placeholder_shows_only_while_the_document_is_empty(cx: &mut TestAppContext) {
    let window = open_empty(cx);
    window
        .update(cx, |view, _, cx| view.set_placeholder("Écrivez", cx))
        .unwrap();
    let shown = |cx: &mut TestAppContext| {
        window
            .read_with(cx, |view, _| view.placeholder_shown())
            .unwrap()
    };
    assert!(shown(cx));
    cx.simulate_input(any(window), "a");
    assert!(!shown(cx));
    assert_eq!(texts(cx, window), ["a"]);
    cx.simulate_keystrokes(any(window), "backspace");
    assert!(shown(cx));
}
