use super::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    InputEvent, ListAlignment, ListState, ScrollDelta, TestAppContext, TouchPhase, list, point,
};

struct DialogFixture {
    transcript: ListState,
    form_scroll: ScrollHandle,
    focus: FocusHandle,
    height: gpui_kit::Pixels,
}
impl Render for DialogFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .w(px(700.))
            .h(self.height)
            .child(gpui_kit::base::ScrollBounce::new(
                "transcript",
                &self.transcript,
                list(self.transcript.clone(), |_, _, _| {
                    div().h(px(40.)).into_any_element()
                })
                .w(px(700.))
                .h(self.height),
            ))
            .child(modal_frame(
                &self.focus,
                &self.form_scroll,
                "Review bot access",
                div()
                    .h(px(1400.))
                    .flex_shrink_0()
                    .child("Last channel")
                    .into_any_element(),
                div()
                    .h(px(52.))
                    .flex_shrink_0()
                    .child("Add bot")
                    .into_any_element(),
                self.height,
            ))
    }
}
fn wheel(window: &mut Window, cx: &mut App, position: gpui_kit::Point<gpui_kit::Pixels>) {
    window.dispatch_event(
        gpui_kit::ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(-400.))),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}
fn check_modal_scroll(cx: &mut TestAppContext, height: f32) {
    cx.update(gpui_kit::init);
    let transcript = ListState::new(100, ListAlignment::Top, px(0.)).measure_all();
    let form_scroll = ScrollHandle::new();
    let (window, _view) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |_, cx| {
            cx.new(|cx| DialogFixture {
                transcript: transcript.clone(),
                form_scroll: form_scroll.clone(),
                focus: cx.focus_handle(),
                height: px(height),
            })
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bounds = form_scroll.bounds();
        assert!(
            form_scroll.max_offset().y > px(0.),
            "the form must have a bounded scrolling viewport"
        );
        assert!(
            bounds.bottom() + px(52.) <= px(height - 16.),
            "the footer must fit inside the window"
        );
        let conversation_offset = transcript.scroll_px_offset_for_scrollbar();
        let pointer = bounds.center();
        wheel(window, cx, pointer);
        assert!(
            form_scroll.offset().y < px(0.),
            "wheel input must scroll the access form"
        );
        assert_eq!(
            transcript.scroll_px_offset_for_scrollbar(),
            conversation_offset,
            "modal wheel input must not reach messages"
        );
        for _ in 0..8 {
            wheel(window, cx, pointer);
        }
        assert_eq!(
            form_scroll.offset().y,
            -form_scroll.max_offset().y,
            "every field at the bottom must be reachable"
        );
        assert_eq!(
            form_scroll.bounds(),
            bounds,
            "scrolling must leave the viewport and footer in place"
        );
        assert_eq!(
            transcript.scroll_px_offset_for_scrollbar(),
            conversation_offset,
            "scrolling at the form's boundary must stay in the modal"
        );
        let bottom = form_scroll.offset();
        for pointer in [
            point(bounds.center().x, bounds.top() - px(10.)),
            point(bounds.center().x, bounds.bottom() + px(10.)),
            point(px(5.), px(200.)),
        ] {
            wheel(window, cx, pointer);
            assert_eq!(
                transcript.scroll_px_offset_for_scrollbar(),
                conversation_offset,
                "header, footer and backdrop must shield messages"
            );
            assert_eq!(form_scroll.offset(), bottom);
        }
    })
    .unwrap();
}
#[gpui_kit::test]
fn access_form_scrolls_to_bottom_without_moving_conversation(cx: &mut TestAppContext) {
    check_modal_scroll(cx, 760.);
}
#[gpui_kit::test]
fn short_window_keeps_bot_form_scrollable_and_footer_visible(cx: &mut TestAppContext) {
    check_modal_scroll(cx, 400.);
}
