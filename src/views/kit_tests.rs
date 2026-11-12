//! Exercise the migrated platform, Kit Root, wheel dispatch and our custom input
//! against both selectable GPUI engines. No backend service or user data is used.
use super::input::TextField;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, Entity, Focusable, InputEvent, IntoElement, ListAlignment, ListState,
    ParentElement, Render, ScrollDelta, Styled, TestAppContext, div, list, point, px,
};

struct Transcript {
    list: ListState,
    scroll_events: usize,
}

impl Render for Transcript {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        gpui_kit::base::ScrollBounce::new(
            "transcript-bounce",
            &self.list,
            list(self.list.clone(), |_, _, _| {
                div().h(px(40.)).w_full().into_any_element()
            })
            .w(px(240.))
            .h(px(200.)),
        )
        .enabled(true)
        .on_scroll(move |_, cx| {
            let _ = owner.update(cx, |this, _| this.scroll_events += 1);
        })
    }
}

#[gpui_kit::test]
fn kit_root_applies_each_wheel_delta_once(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let list = ListState::new(40, ListAlignment::Top, px(0.)).measure_all();
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |_, cx| {
            cx.new(|_| Transcript {
                list: list.clone(),
                scroll_events: 0,
            })
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::ScrollEvent {
                position: point(px(100.), px(100.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
                touch_phase: gpui_kit::TouchPhase::Started,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(list.scroll_px_offset_for_scrollbar().y, px(-30.));
        assert_eq!(view.read(cx).scroll_events, 1);
    })
    .unwrap();
}

struct InputFixture(Entity<TextField>);
impl Render for InputFixture {
    fn render(&mut self, _: &mut gpui_kit::Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p_4().w(px(320.)).child(self.0.clone())
    }
}

#[gpui_kit::test]
fn custom_composer_keeps_unicode_input_and_focus_through_redraw(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::key_bindings::install(cx);
    });
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            let input = cx
                .new(|cx| TextField::new_auto_grow(cx.focus_handle(), "Message", String::new(), 8));
            window.focus(&input.focus_handle(cx), cx);
            cx.new(|_| InputFixture(input))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.input("hello 🦀界", cx);
        let input = view.read(cx).0.clone();
        assert_eq!(input.read(cx).text(), "hello 🦀界");
        window.press("backspace", cx);
        assert_eq!(input.read(cx).text(), "hello 🦀");
        view.update(cx, |_, cx| cx.notify());
        window.render_frame(cx);
        window.input("!", cx);
        assert_eq!(input.read(cx).text(), "hello 🦀!");
        assert!(input.focus_handle(cx).is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn composer_up_and_down_move_one_line_at_a_time(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::key_bindings::install(cx);
    });
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            let input = cx
                .new(|cx| TextField::new_auto_grow(cx.focus_handle(), "Message", String::new(), 8));
            window.focus(&input.focus_handle(cx), cx);
            cx.new(|_| InputFixture(input))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        let input = view.read(cx).0.clone();
        let set = |text: &str, at: usize, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            input.update(cx, |input, cx| {
                input.replace_all_and_select(text.into(), at..at, cx)
            });
            window.render_frame(cx);
        };
        let press = |key: &str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            window.press(key, cx);
            window.render_frame(cx);
            input.read(cx).cursor_offset()
        };

        // A fenced draft: lines 0..3, 4..26 and 27..30.
        let draft = "```\nlet total = a + b + c;\n```";
        set(draft, draft.len(), window, cx);
        assert_eq!(
            press("down", window, cx),
            30,
            "Down on the last line keeps the end"
        );
        assert!(
            (5..=26).contains(&press("up", window, cx)),
            "Up reaches line 2"
        );
        assert_eq!(
            press("down", window, cx),
            30,
            "Down returns to the last line"
        );
        set(draft, 0, window, cx);
        assert_eq!(press("down", window, cx), 4, "Down reaches line 2");
        assert_eq!(press("down", window, cx), 27, "Down reaches line 3");
        assert_eq!(press("up", window, cx), 4);
        assert_eq!(press("up", window, cx), 0);
        assert_eq!(
            press("up", window, cx),
            0,
            "Up on the first line stays at the start"
        );

        // Passing a short line, the cursor returns to the column it started at.
        let lines = "The quick brown fox jumps\nHi\nThe quick brown fox jumps";
        set(lines, 15, window, cx);
        assert_eq!(press("down", window, cx), 28);
        assert_eq!(press("down", window, cx), 29 + 15);
        assert_eq!(press("up", window, cx), 28);
        assert_eq!(press("up", window, cx), 15);
    })
    .unwrap();
}

#[gpui_kit::test]
fn composer_moves_row_by_row_through_wrapped_lines(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::key_bindings::install(cx);
    });
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            let input = cx
                .new(|cx| TextField::new_auto_grow(cx.focus_handle(), "Message", String::new(), 8));
            window.focus(&input.focus_handle(cx), cx);
            cx.new(|_| InputFixture(input))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        let input = view.read(cx).0.clone();
        let paragraph = "The quick brown fox jumps over the lazy dog while five boxing wizards \
            jump quickly. Pack my box with five dozen liquor jugs, then sphinx of black \
            quartz, judge my vow and keep on typing a little longer.";
        let draft = format!("```\n{paragraph}\n```");
        let last_line = draft.len() - 3;
        input.update(cx, |input, cx| {
            input.replace_all_and_select(draft.clone(), 0..0, cx)
        });
        window.render_frame(cx);
        let caret = |cx: &mut gpui_kit::App| input.read(cx).caret_position().unwrap();
        let press = |key: &str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            window.press(key, cx);
            window.render_frame(cx);
            (input.read(cx).cursor_offset(), caret(cx).0)
        };

        // Down visits every row once, each at its start, until the closing fence.
        let (top, line_height) = caret(cx);
        let mut rows = vec![(0, top)];
        while rows.last().unwrap().0 != last_line {
            assert!(rows.len() < 20, "Down stalled at {:?}", rows.last());
            let (offset, position) = press("down", window, cx);
            let (_, above) = *rows.last().unwrap();
            assert_eq!(position.y, above.y + line_height, "one row lower: {rows:?}");
            assert_eq!(position.x, top.x, "at the row's start: {rows:?}");
            rows.push((offset, position));
        }
        assert!(
            rows.len() >= 5,
            "the paragraph wraps onto several rows: {rows:?}"
        );

        // Up retraces them.
        for &(offset, position) in rows.iter().rev().skip(1) {
            assert_eq!(press("up", window, cx), (offset, position));
        }

        // A click at the start of a wrapped row puts the caret there, not at the
        // end of the row above, which is the same offset.
        let (wrap, row_start) = rows[2];
        let click = |local: gpui_kit::Point<gpui_kit::Pixels>,
                     window: &mut gpui_kit::Window,
                     cx: &mut gpui_kit::App| {
            let position = input.read(cx).window_point(local).unwrap();
            for event in [
                gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
                gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
            ] {
                window.dispatch_event(event, cx);
            }
            window.render_frame(cx);
            (input.read(cx).cursor_offset(), caret(cx).0)
        };
        let mid_row = gpui_kit::point(
            row_start.x + gpui_kit::px(1.),
            row_start.y + line_height / 2.,
        );
        assert_eq!(click(mid_row, window, cx), (wrap, row_start));
        // Past the end of the row above, the caret stays on that row. The
        // fixture's text is 288px wide; its rows wrap before the edge.
        let past_end = gpui_kit::point(gpui_kit::px(287.), row_start.y - line_height / 2.);
        let (offset, position) = click(past_end, window, cx);
        assert_eq!(offset, wrap);
        assert_eq!(position.y, row_start.y - line_height);
    })
    .unwrap();
}

/// Cmd and Option shortcuts for moving and deleting, in a wrapped paragraph.
#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn composer_line_keys_act_on_the_visible_row(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::key_bindings::install(cx);
    });
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            let input = cx
                .new(|cx| TextField::new_auto_grow(cx.focus_handle(), "Message", String::new(), 8));
            window.focus(&input.focus_handle(cx), cx);
            cx.new(|_| InputFixture(input))
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        let input = view.read(cx).0.clone();
        let paragraph = "The quick brown fox jumps over the lazy dog while five boxing wizards \
            jump quickly. Pack my box with five dozen liquor jugs, then sphinx of black \
            quartz, judge my vow and keep on typing a little longer.";
        let draft = format!("```\n{paragraph}\n```");
        let paragraph_end = draft.len() - 4;
        let set = |text: &str, at: usize, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            input.update(cx, |input, cx| {
                input.replace_all_and_select(text.into(), at..at, cx)
            });
            window.render_frame(cx);
        };
        let press = |key: &str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            window.press(key, cx);
            window.render_frame(cx);
            let input = input.read(cx);
            (input.selected_range(), input.caret_position().unwrap().0)
        };
        let text = |cx: &mut gpui_kit::App| input.read(cx).text().to_string();

        // Find the paragraph's second row: Down from its start lands on it.
        set(&draft, 4, window, cx);
        let (second_row, row_start) = press("down", window, cx);
        let row = second_row.start;
        let (third_row, _) = press("down", window, cx);
        let row_end = third_row.start;
        assert!(4 < row && row < row_end && row_end < paragraph_end);
        let inside = row + 3;

        // Home and End: the row's start and end, drawn on that row.
        set(&draft, inside, window, cx);
        assert_eq!(press("cmd-left", window, cx), (row..row, row_start));
        let (range, end_caret) = press("cmd-right", window, cx);
        assert_eq!(range, row_end..row_end, "End after Home stays on the row");
        assert_eq!(end_caret.y, row_start.y);
        assert!(end_caret.x > row_start.x);
        set(&draft, inside, window, cx);
        assert_eq!(press("cmd-shift-left", window, cx).0, row..inside);
        set(&draft, inside, window, cx);
        assert_eq!(press("cmd-shift-right", window, cx).0, inside..row_end);

        // Deleting to the row's edges stays on the row.
        set(&draft, inside, window, cx);
        press("cmd-backspace", window, cx);
        assert_eq!(text(cx), format!("{}{}", &draft[..row], &draft[inside..]));
        set(&draft, inside, window, cx);
        press("cmd-delete", window, cx);
        assert_eq!(
            text(cx),
            format!("{}{}", &draft[..inside], &draft[row_end..])
        );

        // Ctrl+A, Ctrl+E and Ctrl+K keep acting on the whole paragraph.
        set(&draft, inside, window, cx);
        assert_eq!(press("ctrl-a", window, cx).0, 4..4);
        assert_eq!(press("ctrl-e", window, cx).0, paragraph_end..paragraph_end);
        set(&draft, inside, window, cx);
        press("ctrl-k", window, cx);
        assert_eq!(
            text(cx),
            format!("{}{}", &draft[..inside], &draft[paragraph_end..])
        );

        // Cmd+Up and Cmd+Down: the start and end of the message.
        set(&draft, inside, window, cx);
        assert_eq!(press("cmd-up", window, cx).0, 0..0);
        assert_eq!(press("cmd-down", window, cx).0, draft.len()..draft.len());
        set(&draft, inside, window, cx);
        assert_eq!(press("cmd-shift-down", window, cx).0, inside..draft.len());
        set(&draft, inside, window, cx);
        assert_eq!(press("cmd-shift-up", window, cx).0, 0..inside);

        // Option+Delete deletes the next word.
        set("alpha beta gamma", 6, window, cx);
        press("alt-delete", window, cx);
        assert_eq!(text(cx), "alpha  gamma");
    })
    .unwrap();
}

#[cfg(target_os = "macos")]
mod sidebar_bounce {
    use super::*;
    use gpui_kit::test::TestSupportExt;
    use gpui_kit::{
        InteractiveElement, ScrollHandle, StatefulInteractiveElement, UniformListScrollHandle,
        base::ScrollbarHandle,
    };

    #[derive(Clone)]
    enum Handle {
        Grouped(ScrollHandle),
        Recent(UniformListScrollHandle),
    }
    impl Handle {
        fn scroll(&self) -> &dyn ScrollbarHandle {
            match self {
                Self::Grouped(handle) => handle,
                Self::Recent(handle) => handle,
            }
        }
    }
    struct SidebarFixture {
        handle: Handle,
        reduced_motion: bool,
    }
    impl Render for SidebarFixture {
        fn render(&mut self, _: &mut gpui_kit::Window, _: &mut Context<Self>) -> impl IntoElement {
            let viewport = match &self.handle {
                Handle::Grouped(handle) => super::super::sidebar::scroll_viewport(
                    "test-grouped-bounce",
                    handle,
                    div()
                        .id("grouped")
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_y_scroll()
                        .track_scroll(handle)
                        .child(div().h(px(1600.)).flex_shrink_0()),
                    self.reduced_motion,
                ),
                Handle::Recent(handle) => super::super::sidebar::scroll_viewport(
                    "test-recent-bounce",
                    handle,
                    gpui_kit::uniform_list("recent", 40, |range, _, _| {
                        range
                            .map(|_| div().h(px(40.)).w_full().into_any_element())
                            .collect()
                    })
                    .track_scroll(handle)
                    .flex_1()
                    .min_h(px(0.)),
                    self.reduced_motion,
                ),
            };
            div()
                .w(px(240.))
                .h(px(300.))
                .flex()
                .flex_col()
                .child(div().id("header").test_support().h(px(40.)).flex_shrink_0())
                .child(viewport)
                .child(div().id("footer").test_support().h(px(40.)).flex_shrink_0())
        }
    }

    fn check(cx: &mut TestAppContext, reduced_motion: bool) {
        for recent in [false, true] {
            for bottom in [false, true] {
                let handle = if recent {
                    Handle::Recent(UniformListScrollHandle::new())
                } else {
                    Handle::Grouped(ScrollHandle::new())
                };
                let mut app = cx.new_app();
                app.update(gpui_kit::init);
                let (window, _) = app.update(|cx| {
                    gpui_kit::open_window(Default::default(), cx, |_, cx| {
                        cx.new(|_| SidebarFixture {
                            handle: handle.clone(),
                            reduced_motion,
                        })
                    })
                    .unwrap()
                });
                app.update_window(window, |_, window, cx| {
                    window.render_frame(cx);
                    let max = handle.scroll().content_size().height
                        - handle.scroll().viewport_bounds().size.height;
                    assert!(max > px(0.));
                    let offset = if bottom { -max } else { px(0.) };
                    handle.scroll().set_offset(point(px(0.), offset));
                    window.refresh();
                    window.render_frame(cx);
                    let header = window.find("header").bounds();
                    let footer = window.find("footer").bounds();
                    let origin = handle.scroll().viewport_bounds().origin;
                    let direction = if bottom { -1. } else { 1. };
                    for (phase, dy) in [
                        (gpui_kit::TouchPhase::Started, 0.),
                        (gpui_kit::TouchPhase::Moved, 100. * direction),
                    ] {
                        window.dispatch_event(
                            gpui_kit::ScrollEvent {
                                position: point(px(100.), px(100.)),
                                delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                                touch_phase: phase,
                                ..Default::default()
                            }
                            .to_platform_input(),
                            cx,
                        );
                        window.render_frame(cx);
                    }
                    let pulled = handle.scroll().viewport_bounds().origin;
                    if reduced_motion {
                        assert_eq!(pulled, origin);
                    } else {
                        assert!(
                            (pulled.y - origin.y) * direction > px(0.),
                            "sidebar should stretch at either edge"
                        );
                    }
                    assert_eq!(
                        handle.scroll().offset().y,
                        offset,
                        "logical scrolling stays clamped"
                    );
                    assert_eq!(window.find("header").bounds(), header);
                    assert_eq!(window.find("footer").bounds(), footer);
                })
                .unwrap();
            }
        }
    }

    #[gpui_kit::test]
    fn both_sidebar_modes_bounce_without_moving_chrome(cx: &mut TestAppContext) {
        check(cx, false);
    }

    #[gpui_kit::test]
    fn sidebar_reduce_motion_disables_stretch(cx: &mut TestAppContext) {
        check(cx, true);
    }
}

mod slow_scroll {
    use super::*;
    use crate::views::timeline::layout_anchor;
    use gpui_kit::InteractiveElement;
    use std::{cell::Cell, rc::Rc};

    struct PinnedTranscript {
        list: ListState,
        bottom_distance: Rc<Cell<gpui_kit::Pixels>>,
    }
    impl Render for PinnedTranscript {
        fn render(&mut self, _: &mut gpui_kit::Window, _: &mut Context<Self>) -> impl IntoElement {
            let list_state = self.list.clone();
            let distance = self.bottom_distance.clone();
            let sampled = self.bottom_distance.clone();
            let scroll_state = self.list.clone();
            div()
                .relative()
                .w(px(400.))
                .h(px(200.))
                .child(
                    gpui_kit::canvas(
                        move |_, _, _| {
                            if layout_anchor::should_follow_latest(
                                list_state.viewport_bounds().size.height,
                                distance.get(),
                                false,
                            ) {
                                list_state.scroll_to_end();
                            }
                        },
                        |_, _, _, _| (),
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(
                    gpui_kit::base::ScrollBounce::new(
                        "pinned-transcript",
                        &self.list,
                        list(self.list.clone(), |_, _, _| {
                            div().h(px(40.)).into_any_element()
                        })
                        .w(px(400.))
                        .h(px(200.)),
                    )
                    .on_scroll(move |_, _| {
                        sampled.set(layout_anchor::bottom_distance(&scroll_state))
                    }),
                )
                .child(
                    gpui_kit::deferred(
                        div()
                            .absolute()
                            .left(px(40.))
                            .top(px(40.))
                            .w(px(260.))
                            .h(px(32.))
                            .block_mouse_except_scroll()
                            .on_mouse_move(|_, _, cx| cx.stop_propagation())
                            .child("😀 👍 ❤️"),
                    )
                    .priority(10),
                )
        }
    }

    #[gpui_kit::test]
    fn slow_upward_scrolling_escapes_bottom_pinning(cx: &mut TestAppContext) {
        for delta in [
            ScrollDelta::Pixels(point(px(0.), px(0.25))),
            ScrollDelta::Pixels(point(px(0.), px(1.))),
            ScrollDelta::Lines(point(0., 1.)),
        ] {
            let mut app = cx.new_app();
            app.update(gpui_kit::init);
            let state = ListState::new(40, ListAlignment::Top, px(0.)).measure_all();
            state.scroll_to_end();
            let (window, _) = app.update(|cx| {
                gpui_kit::open_window(Default::default(), cx, |_, cx| {
                    cx.new(|_| PinnedTranscript {
                        list: state.clone(),
                        bottom_distance: Rc::new(Cell::new(px(0.))),
                    })
                })
                .unwrap()
            });
            app.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(layout_anchor::bottom_distance(&state), px(0.));
                let step = delta.pixel_delta(window.line_height()).y;
                for packet in 0..=16 {
                    window.dispatch_event(gpui_kit::ScrollEvent {
                        position: point(px(100.), px(100.)),
                        delta: if packet == 0 { ScrollDelta::Pixels(point(px(0.), px(0.))) } else { delta },
                        touch_phase: if packet == 0 { gpui_kit::TouchPhase::Started } else { gpui_kit::TouchPhase::Moved },
                        ..Default::default()
                    }.to_platform_input(), cx);
                    window.render_frame(cx);
                    assert!((layout_anchor::bottom_distance(&state) - step * packet as f32).abs() < px(0.01),
                        "small deltas must accumulate across layouts: {delta:?}, packet {packet}, actual {:?}", layout_anchor::bottom_distance(&state));
                }
            }).unwrap();
        }
    }
}

mod window_exit {
    use super::*;
    use gpui_kit::InteractiveElement;

    struct HoverWindow {
        hovered: bool,
        exits: usize,
    }
    impl Render for HoverWindow {
        fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut Context<Self>) -> impl IntoElement {
            use gpui_kit::prelude::FluentBuilder;
            div()
                .relative()
                .w(px(400.))
                .h(px(200.))
                .child(super::super::app_window::window_exit_observer(cx.listener(
                    |this, _, _, cx| {
                        this.hovered = false;
                        this.exits += 1;
                        cx.notify();
                    },
                )))
                .when(self.hovered, |root| {
                    root.child(
                        gpui_kit::deferred(
                            div()
                                .absolute()
                                .inset_0()
                                .block_mouse_except_scroll()
                                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                                .on_mouse_exit(|_, _, cx| cx.stop_propagation())
                                .child("😀 👍 ❤️"),
                        )
                        .priority(10),
                    )
                })
        }
    }

    #[gpui_kit::test]
    fn window_exit_clears_hover_immediately_even_over_a_floating_strip(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(Default::default(), cx, |_, cx| {
                cx.new(|_| HoverWindow {
                    hovered: true,
                    exits: 0,
                })
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let pointer = point(px(100.), px(100.));
            window.simulate_mouse_move(pointer, cx);
            assert!(view.read(cx).hovered);
            window.dispatch_event(
                gpui_kit::MouseExitEvent {
                    position: pointer,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            // No timer advance or another pointer move is needed.
            assert!(!view.read(cx).hovered);
            assert_eq!(view.read(cx).exits, 1);
            window.render_frame(cx);
            assert!(!view.read(cx).hovered);
        })
        .unwrap();
    }
}

/// The composer's Up and Down reach the window first: an open autocomplete
/// list takes them, and only then does the editor move its caret. Real
/// keystrokes through the app's key bindings, not direct action calls.
mod composer_keys {
    use super::*;
    use crate::app::commands::ConfirmPrimary;
    use crate::views::input::{Down, Up};
    use gpui_kit::InteractiveElement;

    struct ComposerHost {
        editor: Entity<TextField>,
        list_open: bool,
        list_keys: Vec<&'static str>,
        confirmed: usize,
    }

    impl Render for ComposerHost {
        fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .key_context("Workspace")
                .w(px(400.))
                .h(px(200.))
                .capture_action(cx.listener(|this, _: &Up, _, cx| {
                    if this.list_open {
                        this.list_keys.push("up");
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &Down, _, cx| {
                    if this.list_open {
                        this.list_keys.push("down");
                        cx.stop_propagation();
                    }
                }))
                .on_action(cx.listener(|this, _: &ConfirmPrimary, _, _| this.confirmed += 1))
                .child(self.editor.clone())
        }
    }

    #[gpui_kit::test]
    fn an_open_list_takes_up_and_down_before_the_editor(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::key_bindings::install(cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| {
            let editor = cx.new(|cx| {
                TextField::new_auto_grow(cx.focus_handle(), "", "one\ntwo".to_string(), 8)
            });
            ComposerHost {
                editor,
                list_open: true,
                list_keys: Vec::new(),
                confirmed: 0,
            }
        });
        cx.update(|window, cx| {
            host.read(cx).editor.focus_handle(cx).focus(window, cx);
            window.draw(cx).clear(cx);
        });
        let end = "one\ntwo".len();

        cx.simulate_keystrokes("up down down");
        cx.update(|_, cx| {
            assert_eq!(host.read(cx).list_keys, ["up", "down", "down"]);
            assert_eq!(host.read(cx).editor.read(cx).cursor_offset(), end);
        });

        cx.update(|_, cx| host.update(cx, |host, _| host.list_open = false));
        cx.simulate_keystrokes("up");
        cx.update(|_, cx| {
            assert_eq!(host.read(cx).list_keys.len(), 3);
            assert!(host.read(cx).editor.read(cx).cursor_offset() <= "one".len());
        });

        // Return is not the editor's: it reaches the window, which accepts a
        // completion or sends.
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(host.read(cx).confirmed, 1));
    }
}
