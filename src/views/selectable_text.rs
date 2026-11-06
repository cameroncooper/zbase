mod mixed_layout;
use mixed_layout::MixedLayout;

use crate::{
    app::commands,
    views::{
        link,
        app_window::AppWindow,
        input::{
            Copy, Down, End, Home, Left, Right, SelectAll, SelectDown, SelectLeft, SelectRight,
            SelectUp, Up,
        },
        selection,
    },
};
use gpui::{
    App, AvailableSpace, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, Entity,
    FocusHandle, Focusable, FontStyle, FontWeight, GlobalElementId, Hsla, InteractiveElement,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, Resource, StrikethroughStyle, Style, Subscription, TextRun, UnderlineStyle, Window,
    WrappedLine, div, fill, point, prelude::*, px, relative, rgb, rgba, size,
};
use std::{collections::HashMap, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRange {
    pub byte_range: Range<usize>,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledRange {
    pub byte_range: Range<usize>,
    pub color: Option<u32>,
    pub background_color: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
    pub strikethrough: bool,
    /// Annotation such as "(edited)": drawn small in the body font and left out
    /// of copied text.
    pub meta: bool,
}

impl StyledRange {
    pub fn inline_code(byte_range: Range<usize>) -> Self {
        Self {
            byte_range,
            color: Some(crate::views::inline_code_foreground()),
            background_color: Some(crate::views::inline_code_background()),
            bold: false,
            italic: false,
            monospace: true,
            strikethrough: false,
            meta: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineAttachment {
    pub byte_range: Range<usize>,
    pub source: String,
    pub fallback_text: String,
    pub size_px: u16,
}

pub struct SelectableText {
    focus_handle: FocusHandle,
    blur_subscription: Option<Subscription>,
    debug_label: Option<String>,
    content: String,
    link_ranges: Vec<LinkRange>,
    styled_ranges: Vec<StyledRange>,
    inline_attachments: Vec<InlineAttachment>,
    fill_width: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    last_layout: Option<TextLayoutSnapshot>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    hovering_link: bool,
    colors: SelectableTextColors,
}

/// Capture colors while the parent theme is active. GPUI lays out and paints
/// this child later, outside that temporary theme scope.
#[derive(Clone, Copy, PartialEq, Eq)]
struct SelectableTextColors {
    link: u32,
    selection: u32,
}

impl SelectableTextColors {
    fn current() -> Self {
        Self {
            link: link(),
            selection: selection(),
        }
    }
}

impl SelectableText {
    pub fn new(focus_handle: FocusHandle, content: impl Into<String>) -> Self {
        Self {
            focus_handle,
            blur_subscription: None,
            debug_label: None,
            content: content.into(),
            link_ranges: Vec::new(),
            styled_ranges: Vec::new(),
            inline_attachments: Vec::new(),
            fill_width: true,
            selected_range: 0..0,
            selection_reversed: false,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            hovering_link: false,
            colors: SelectableTextColors::current(),
        }
    }

    pub fn set_content(
        &mut self,
        text: impl Into<String>,
        link_ranges: Vec<LinkRange>,
        styled_ranges: Vec<StyledRange>,
        cx: &mut Context<Self>,
    ) {
        self.set_content_with_options_and_attachments(
            text,
            link_ranges,
            styled_ranges,
            Vec::new(),
            true,
            cx,
        );
    }

    pub fn set_content_with_options(
        &mut self,
        text: impl Into<String>,
        link_ranges: Vec<LinkRange>,
        styled_ranges: Vec<StyledRange>,
        fill_width: bool,
        cx: &mut Context<Self>,
    ) {
        self.set_content_with_options_and_attachments(
            text,
            link_ranges,
            styled_ranges,
            Vec::new(),
            fill_width,
            cx,
        );
    }

    pub fn set_content_with_options_and_attachments(
        &mut self,
        text: impl Into<String>,
        link_ranges: Vec<LinkRange>,
        styled_ranges: Vec<StyledRange>,
        inline_attachments: Vec<InlineAttachment>,
        fill_width: bool,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        let colors = SelectableTextColors::current();
        if self.colors != colors {
            self.colors = colors;
            self.clear_layout_cache();
            cx.notify();
        }
        if self.content == text
            && self.link_ranges == link_ranges
            && self.styled_ranges == styled_ranges
            && self.inline_attachments == inline_attachments
            && self.fill_width == fill_width
        {
            return;
        }

        self.content = text;
        self.link_ranges = link_ranges;
        self.styled_ranges = styled_ranges;
        self.inline_attachments = inline_attachments;
        self.fill_width = fill_width;
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.last_layout = None;
        self.last_bounds = None;
        cx.notify();
    }

    fn clear_layout_cache(&mut self) {
        self.last_layout = None;
        self.last_bounds = None;
    }

    /// Ranges to draw at the small inline size when this paragraph needs the
    /// word-based layout: for inline code or "(edited)", or so closing punctuation
    /// never wraps onto a line of its own (GPUI's wrapper breaks before "!" or ")").
    /// `None` keeps GPUI's paragraph layout.
    fn mixed_layout_ranges(&self) -> Option<Vec<Range<usize>>> {
        if !MixedLayout::supports(&self.content) {
            return None;
        }
        let small = self
            .styled_ranges
            .iter()
            .filter(|range| range.monospace || range.meta)
            .map(|range| range.byte_range.clone())
            .collect::<Vec<_>>();
        (!small.is_empty() || mixed_layout::has_glued_closing_punctuation(&self.content))
            .then_some(small)
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    pub fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn move_vertical(&mut self, direction: f32, cx: &mut Context<Self>) {
        if let Some(next) = self.vertical_target_offset(direction) {
            self.move_to(next, cx);
        }
    }

    fn select_vertical(&mut self, direction: f32, cx: &mut Context<Self>) {
        if let Some(next) = self.vertical_target_offset(direction) {
            self.select_to(next, cx);
        }
    }

    fn vertical_target_offset(&self, direction: f32) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        let cursor = self.cursor_offset();
        let position = layout.position_for_index(cursor)?;
        Some(layout.closest_index_for_position(point(
            position.x,
            (position.y + layout.line_height * direction).max(px(0.)),
        )))
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(-1.0, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(1.0, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_vertical(-1.0, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_vertical(1.0, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text_for_copy()));
        }
    }

    fn selected_text_for_copy(&self) -> String {
        text_for_copy(
            &self.content,
            self.selected_range.clone(),
            &self.inline_attachments,
            &self.styled_ranges,
        )
    }

    pub fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(layout)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        let local_position = point(position.x - bounds.origin.x, position.y - bounds.origin.y);
        layout.closest_index_for_position(local_position)
    }

    fn link_at_index(&self, index: usize) -> Option<&LinkRange> {
        self.link_ranges
            .iter()
            .find(|link| index >= link.byte_range.start && index < link.byte_range.end)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        window.focus(&self.focus_handle(cx));
        let offset = self.index_for_mouse_position(event.position);

        if !event.modifiers.shift {
            if event.click_count >= 3 {
                self.select_line_at(offset, cx);
                return;
            }
            if event.click_count == 2 {
                self.select_word_at(offset, cx);
                return;
            }
        }

        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
        if !self.link_ranges.is_empty() {
            let in_bounds = self
                .last_bounds
                .map_or(false, |b| b.contains(&event.position));
            let over_link = if in_bounds {
                let index = self.index_for_mouse_position(event.position);
                self.link_at_index(index).is_some()
            } else {
                false
            };
            if over_link != self.hovering_link {
                self.hovering_link = over_link;
                if over_link {
                    window.dispatch_action(Box::new(commands::DismissHoverToolbar), cx);
                }
                cx.notify();
            }
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let up_offset = self.index_for_mouse_position(event.position);
            // For double/triple click selection, mouse-down may have already established a full
            // word/line range. Avoid shrinking it on mouse-up when releasing within that range.
            if self.selected_range.is_empty() {
                self.select_to(up_offset, cx);
            } else if !(self.selected_range.start <= up_offset
                && up_offset <= self.selected_range.end)
            {
                self.select_to(up_offset, cx);
            }
            self.is_selecting = false;
            if self.selected_range.is_empty() {
                let index = self.index_for_mouse_position(event.position);
                if let Some(link) = self.link_at_index(index) {
                    let action = commands::OpenUrl {
                        url: link.url.clone(),
                    };
                    window.dispatch_action(Box::new(action), cx);
                }
            }
        }
    }

    pub fn is_selecting(&self) -> bool {
        self.is_selecting
    }

    pub fn has_selection(&self) -> bool {
        !self.selected_range.is_empty()
    }

    pub fn is_hovering_link(&self) -> bool {
        self.hovering_link
    }

    pub fn is_position_over_link(&self, position: Point<Pixels>) -> bool {
        if self.link_ranges.is_empty() {
            return false;
        }
        let in_bounds = self.last_bounds.map_or(false, |b| b.contains(&position));
        if !in_bounds {
            return false;
        }
        let index = self.index_for_mouse_position(position);
        self.link_at_index(index).is_some()
    }

    fn select_word_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(range) = word_range_at_offset(&self.content, offset) else {
            self.move_to(offset, cx);
            return;
        };
        self.selected_range = range;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_line_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let range = line_range_at_offset(&self.content, offset);
        self.selected_range = range;
        self.selection_reversed = false;
        cx.notify();
    }

    fn build_text_runs(&self, base_color: Hsla, font: gpui::Font) -> Vec<TextRun> {
        build_text_runs_for_content(
            &self.content,
            &self.link_ranges,
            &self.styled_ranges,
            base_color,
            font,
            self.colors.link,
        )
    }
}

fn build_text_runs_for_content(
    content: &str,
    link_ranges: &[LinkRange],
    styled_ranges: &[StyledRange],
    base_color: Hsla,
    font: gpui::Font,
    link_foreground: u32,
) -> Vec<TextRun> {
    let text_len = content.len();
    if text_len == 0 {
        return vec![TextRun {
            len: 0,
            font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    if link_ranges.is_empty() && styled_ranges.is_empty() {
        return vec![TextRun {
            len: text_len,
            font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let link_color: Hsla = rgb(link_foreground).into();
    let range_covers_span = |range: &Range<usize>, span_start: usize, span_end: usize| {
        let start = range.start.min(text_len);
        let end = range.end.min(text_len);
        start < end && span_start >= start && span_end <= end
    };
    let mut boundaries = vec![0usize, text_len];
    for link in link_ranges {
        let start = link.byte_range.start.min(text_len);
        let end = link.byte_range.end.min(text_len);
        if start < end {
            boundaries.push(start);
            boundaries.push(end);
        }
    }
    for styled in styled_ranges {
        let start = styled.byte_range.start.min(text_len);
        let end = styled.byte_range.end.min(text_len);
        if start < end {
            boundaries.push(start);
            boundaries.push(end);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    for window in boundaries.windows(2) {
        let span_start = window[0];
        let span_end = window[1];
        if span_end <= span_start {
            continue;
        }
        let active_link_url = link_ranges
            .iter()
            .find(|link| range_covers_span(&link.byte_range, span_start, span_end))
            .map(|link| link.url.as_str());
        let link_active = active_link_url.is_some();
        let internal_link_active = active_link_url.is_some_and(|url| url.starts_with("zbase-"));

        let mut run_font = font.clone();
        let mut run_color = base_color;
        let mut run_background = None;
        let mut run_italic = false;
        let mut run_strikethrough = false;
        for styled in styled_ranges
            .iter()
            .filter(|styled| range_covers_span(&styled.byte_range, span_start, span_end))
        {
            if let Some(color) = styled.color {
                run_color = rgb(color).into();
            }
            if let Some(background) = styled.background_color {
                run_background = Some(rgb(background).into());
            }
            if styled.monospace {
                run_font.family = crate::views::mono_font_family().into();
            }
            if styled.bold {
                run_font.weight = FontWeight::BOLD;
            }
            if styled.italic {
                run_italic = true;
            }
            if styled.strikethrough {
                run_strikethrough = true;
            }
        }
        if run_italic {
            run_font.style = FontStyle::Italic;
        }
        let underline = if link_active && !internal_link_active {
            run_color = link_color;
            Some(UnderlineStyle {
                color: Some(run_color),
                ..Default::default()
            })
        } else {
            None
        };
        let strikethrough = if run_strikethrough {
            Some(StrikethroughStyle {
                color: Some(run_color),
                ..Default::default()
            })
        } else {
            None
        };

        runs.push(TextRun {
            len: span_end - span_start,
            font: run_font,
            color: run_color,
            background_color: run_background,
            underline,
            strikethrough,
        });
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: text_len,
            font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}

struct SelectableTextElement {
    view: Entity<SelectableText>,
}

struct PrepaintState {
    layout: TextLayoutSnapshot,
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    attachments: Vec<InlineAttachmentPaint>,
}

struct InlineAttachmentPaint {
    source: String,
    size_px: u16,
    bounds: Bounds<Pixels>,
}

#[cfg(debug_assertions)]
fn selectable_text_debug_enabled() -> bool {
    std::env::var("ZBASE_TEXT_DEBUG")
        .map(|v| !v.trim().is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

#[cfg(not(debug_assertions))]
fn selectable_text_debug_enabled() -> bool {
    false
}

#[cfg(debug_assertions)]
fn selectable_text_debug_filter_matches(label: &str) -> bool {
    let Ok(filter) = std::env::var("ZBASE_TEXT_DEBUG_FILTER") else {
        return true;
    };
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }
    label.contains(filter)
}

#[cfg(not(debug_assertions))]
fn selectable_text_debug_filter_matches(_: &str) -> bool {
    true
}

fn debug_preview_text(value: &str, limit: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().take(limit) {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    if value.chars().count() > limit {
        out.push('…');
    }
    out
}

fn debug_wrap_boundaries(line: &WrappedLine) -> Vec<(usize, f32)> {
    line.wrap_boundaries()
        .iter()
        .filter_map(|boundary| {
            let run = line.runs().get(boundary.run_ix)?;
            let glyph = run.glyphs.get(boundary.glyph_ix)?;
            Some((glyph.index, glyph.position.x.into()))
        })
        .collect()
}

fn log_selectable_text_layout_debug(
    label: &str,
    bounds: Bounds<Pixels>,
    content: &str,
    attachments: &[InlineAttachment],
    layout: &TextLayoutSnapshot,
) {
    if !selectable_text_debug_enabled() {
        return;
    }
    if !selectable_text_debug_filter_matches(label) {
        return;
    }
    use tracing::warn;

    let preview = debug_preview_text(content, 160);
    warn!(
        "SelectableText debug label={label} bounds_w={:?} bounds_h={:?} content_len={} preview=\"{}\"",
        bounds.size.width,
        bounds.size.height,
        content.len(),
        preview
    );
    if !attachments.is_empty() {
        for (ix, a) in attachments.iter().enumerate() {
            warn!(
                "  attachment[{ix}] byte_range={}..{} size_px={} source=\"{}\" fallback=\"{}\"",
                a.byte_range.start, a.byte_range.end, a.size_px, a.source, a.fallback_text
            );
        }
    }

    let mut line_start_ix = 0usize;
    for (line_ix, line) in layout.lines.iter().enumerate() {
        let boundaries = debug_wrap_boundaries(line);
        warn!(
            "  line[{line_ix}] len={} wrap_boundaries={:?}",
            line.len(),
            boundaries
        );

        let segments = visual_segments(line, line_start_ix, layout.line_height);
        for (seg_ix, seg) in segments.iter().enumerate() {
            warn!(
                "    seg[{seg_ix}] global={}..{} top={:?} width={:?}",
                seg.global_start, seg.global_end, seg.top, seg.width
            );
        }

        line_start_ix = layout.next_line_start(line_start_ix, line.len());
    }
}

impl IntoElement for SelectableTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let view = self.view.read(cx);
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let content = view.content.clone();
        let runs = view.build_text_runs(text_style.color, text_style.font());
        let fill_width = view.fill_width;
        let cached_mixed = view
            .last_layout
            .as_ref()
            .and_then(|layout| layout.mixed.clone());
        let mixed_ranges = view.mixed_layout_ranges();

        let mut style = Style::default();
        if fill_width {
            style.size.width = relative(1.).into();
        }

        (
            window.request_measured_layout(style, move |known, available, window, _cx| {
                let wrap_width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });

                if let Some(code_ranges) = &mixed_ranges {
                    let width = wrap_width.unwrap_or(Pixels::MAX);
                    if let Some(cached) = &cached_mixed
                        && cached.matches(font_size, line_height, width, &runs)
                    {
                        return cached.measured();
                    }
                    return MixedLayout::shape(
                        &content,
                        &runs,
                        code_ranges,
                        font_size,
                        line_height,
                        wrap_width.unwrap_or(Pixels::MAX),
                        window,
                    )
                    .measured();
                }
                let Some(lines) = window
                    .text_system()
                    .shape_text(content.clone().into(), font_size, &runs, wrap_width, None)
                    .ok()
                else {
                    return size(px(0.), px(0.));
                };

                let mut measured = size(px(0.), px(0.));
                for line in &lines {
                    let line_size = line.size(line_height);
                    measured.height += line_size.height;
                    measured.width = measured.width.max(line_size.width).ceil();
                }

                if let Some(w) = wrap_width {
                    measured.width = measured.width.min(w);
                }

                measured
            }),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let view = self.view.read(cx);
        let is_focused = view.focus_handle.is_focused(window);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let runs = view.build_text_runs(style.color, style.font());
        let mixed = view.mixed_layout_ranges().map(|code_ranges| {
            let width = bounds.size.width.max(px(1.));
            if let Some(cached) = view
                .last_layout
                .as_ref()
                .and_then(|layout| layout.mixed.as_ref())
                && cached.matches(font_size, window.line_height(), width, &runs)
            {
                return cached.clone();
            }
            MixedLayout::shape(
                &view.content,
                &runs,
                &code_ranges,
                font_size,
                window.line_height(),
                width,
                window,
            )
        });
        let lines = if mixed.is_some() { Vec::new() } else { window
            .text_system()
            .shape_text(
                view.content.clone().into(),
                font_size,
                &runs,
                Some(bounds.size.width.max(px(1.0))),
                None,
            )
            .map(|lines| lines.into_vec())
            .unwrap_or_default() };

        let layout = TextLayoutSnapshot {
            mixed,
            lines,
            line_height: window.line_height(),
            text_len: view.content.len(),
            content: view.content.clone(),
        };
        log_selectable_text_layout_debug(
            view.debug_label.as_deref().unwrap_or("<unlabeled>"),
            bounds,
            &view.content,
            &view.inline_attachments,
            &layout,
        );
        let selection = if !is_focused || view.selected_range.is_empty() {
            Vec::new()
        } else {
            layout.selection_quads(
                view.selected_range.clone(),
                bounds.origin,
                view.colors.selection,
            )
        };
        let attachments = view
            .inline_attachments
            .iter()
            .flat_map(|attachment| {
                layout
                    .bounds_for_range(attachment.byte_range.clone(), bounds.origin)
                    .into_iter()
                    .map(move |attachment_bounds| InlineAttachmentPaint {
                        source: attachment.source.clone(),
                        size_px: attachment.size_px,
                        bounds: attachment_bounds,
                    })
            })
            .collect::<Vec<_>>();
        let cursor = None;

        PrepaintState {
            layout,
            selection,
            cursor,
            attachments,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut line_origin = bounds.origin;
        let align = window.text_style().text_align;
        for line in &prepaint.layout.lines {
            let _ = line.paint_background(
                line_origin,
                prepaint.layout.line_height,
                align,
                Some(bounds),
                window,
                cx,
            );
            line_origin.y += line.size(prepaint.layout.line_height).height;
        }

        if let Some(mixed) = &prepaint.layout.mixed {
            mixed.paint(bounds.origin, true, window, cx);
        }

        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }

        let mut line_origin = bounds.origin;
        for line in &prepaint.layout.lines {
            let _ = line.paint(
                line_origin,
                prepaint.layout.line_height,
                align,
                Some(bounds),
                window,
                cx,
            );
            line_origin.y += line.size(prepaint.layout.line_height).height;
        }

        if let Some(mixed) = &prepaint.layout.mixed {
            mixed.paint(bounds.origin, false, window, cx);
        }

        for attachment in &prepaint.attachments {
            let resource = resource_from_attachment_source(&attachment.source);
            let Some(Ok(render_image)) = window.use_asset::<gpui::ImgResourceLoader>(&resource, cx)
            else {
                continue;
            };
            let mut draw_width = attachment.bounds.size.width.max(px(1.));
            let mut draw_height = attachment.bounds.size.height.max(px(1.));
            if attachment.size_px > 0 {
                let desired = px(attachment.size_px as f32);
                draw_width = draw_width.min(desired).max(px(1.));
                draw_height = draw_height.min(desired).max(px(1.));
            }
            let origin = point(
                attachment.bounds.origin.x,
                attachment.bounds.origin.y + (attachment.bounds.size.height - draw_height) / 2.,
            );
            let _ = window.paint_image(
                Bounds::new(origin, size(draw_width, draw_height)),
                gpui::Corners::default(),
                render_image,
                0,
                false,
            );
        }

        let layout = std::mem::take(&mut prepaint.layout);
        self.view.update(cx, |view, _cx| {
            view.last_layout = Some(layout);
            view.last_bounds = Some(bounds);
        });
    }
}

impl Render for SelectableText {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.blur_subscription.is_none() {
            self.blur_subscription = Some(cx.on_blur(
                &self.focus_handle,
                window,
                |this, window, cx| {
                    // Cached paragraphs outlive their visible selection. A hidden
                    // selection must not suppress quick reactions in other messages.
                    if this.has_selection() || this.is_selecting {
                        this.selected_range = 0..0;
                        this.selection_reversed = false;
                        this.is_selecting = false;
                        cx.notify();
                        window.refresh();
                    }
                },
            ));
        }
        div()
            .when(self.fill_width, |container| {
                container.w_full().min_w(px(0.))
            })
            .key_context("SelectableText")
            .track_focus(&self.focus_handle(cx))
            .cursor(if self.hovering_link {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            })
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(SelectableTextElement { view: cx.entity() })
    }
}

impl Focusable for SelectableText {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub fn resolve_selectable_text(
    registry: &mut HashMap<String, Entity<SelectableText>>,
    key: impl Into<String>,
    content: impl Into<String>,
    link_ranges: Vec<LinkRange>,
    styled_ranges: Vec<StyledRange>,
    cx: &mut Context<AppWindow>,
) -> Entity<SelectableText> {
    resolve_selectable_text_with_options(
        registry,
        key,
        content,
        link_ranges,
        styled_ranges,
        Vec::new(),
        true,
        cx,
    )
}

pub fn resolve_selectable_text_inline(
    registry: &mut HashMap<String, Entity<SelectableText>>,
    key: impl Into<String>,
    content: impl Into<String>,
    link_ranges: Vec<LinkRange>,
    styled_ranges: Vec<StyledRange>,
    cx: &mut Context<AppWindow>,
) -> Entity<SelectableText> {
    resolve_selectable_text_with_options(
        registry,
        key,
        content,
        link_ranges,
        styled_ranges,
        Vec::new(),
        false,
        cx,
    )
}

pub fn resolve_selectable_text_with_attachments(
    registry: &mut HashMap<String, Entity<SelectableText>>,
    key: impl Into<String>,
    content: impl Into<String>,
    link_ranges: Vec<LinkRange>,
    styled_ranges: Vec<StyledRange>,
    inline_attachments: Vec<InlineAttachment>,
    cx: &mut Context<AppWindow>,
) -> Entity<SelectableText> {
    resolve_selectable_text_with_options(
        registry,
        key,
        content,
        link_ranges,
        styled_ranges,
        inline_attachments,
        true,
        cx,
    )
}

pub fn resolve_selectable_text_inline_with_attachments(
    registry: &mut HashMap<String, Entity<SelectableText>>,
    key: impl Into<String>,
    content: impl Into<String>,
    link_ranges: Vec<LinkRange>,
    styled_ranges: Vec<StyledRange>,
    inline_attachments: Vec<InlineAttachment>,
    cx: &mut Context<AppWindow>,
) -> Entity<SelectableText> {
    resolve_selectable_text_with_options(
        registry,
        key,
        content,
        link_ranges,
        styled_ranges,
        inline_attachments,
        false,
        cx,
    )
}

fn resolve_selectable_text_with_options(
    registry: &mut HashMap<String, Entity<SelectableText>>,
    key: impl Into<String>,
    content: impl Into<String>,
    link_ranges: Vec<LinkRange>,
    styled_ranges: Vec<StyledRange>,
    inline_attachments: Vec<InlineAttachment>,
    fill_width: bool,
    cx: &mut Context<AppWindow>,
) -> Entity<SelectableText> {
    let key = key.into();
    let content = content.into();

    if let Some(view) = registry.get(&key) {
        view.update(cx, |view, cx| {
            view.set_content_with_options_and_attachments(
                content.clone(),
                link_ranges.clone(),
                styled_ranges.clone(),
                inline_attachments.clone(),
                fill_width,
                cx,
            )
        });
        return view.clone();
    }

    let links = link_ranges.clone();
    let styles = styled_ranges.clone();
    let attachments = inline_attachments.clone();
    let text = content.clone();
    let debug_label = key.clone();
    let view = cx.new(|cx| {
        let mut st = SelectableText::new(cx.focus_handle(), text);
        st.debug_label = Some(debug_label);
        st.link_ranges = links;
        st.styled_ranges = styles;
        st.inline_attachments = attachments;
        st.fill_width = fill_width;
        st
    });
    registry.insert(key, view.clone());
    view
}

#[derive(Default)]
struct TextLayoutSnapshot {
    mixed: Option<MixedLayout>,
    lines: Vec<WrappedLine>,
    line_height: Pixels,
    text_len: usize,
    content: String,
}

impl TextLayoutSnapshot {
    fn next_line_start(&self, line_start_ix: usize, line_len: usize) -> usize {
        let mut next = line_start_ix.saturating_add(line_len);
        if self.content.as_bytes().get(next) == Some(&b'\n') {
            next += 1;
        }
        next
    }

    fn closest_index_for_position(&self, position: Point<Pixels>) -> usize {
        if let Some(mixed) = &self.mixed {
            return mixed.closest_index(position);
        }
        if position.y < px(0.) {
            return 0;
        }

        let mut line_origin_y = px(0.);
        let mut line_start_ix = 0;

        for line in &self.lines {
            let line_bottom = line_origin_y + line.size(self.line_height).height;
            if position.y > line_bottom {
                line_origin_y = line_bottom;
                line_start_ix = self.next_line_start(line_start_ix, line.len());
                continue;
            }

            let local_position = point(position.x, position.y - line_origin_y);
            let local_index =
                match line.closest_index_for_position(local_position, self.line_height) {
                    Ok(index) | Err(index) => index,
                };
            return (line_start_ix + local_index).min(self.text_len);
        }

        self.text_len
    }

    fn position_for_index(&self, index: usize) -> Option<Point<Pixels>> {
        if let Some(mixed) = &self.mixed {
            return Some(mixed.position_for_index(index));
        }
        let mut line_origin_y = px(0.);
        let mut line_start_ix = 0;

        for line in &self.lines {
            let line_end_ix = line_start_ix + line.len();
            if index > line_end_ix {
                line_origin_y += line.size(self.line_height).height;
                line_start_ix = self.next_line_start(line_start_ix, line.len());
                continue;
            }

            let local_index = index.saturating_sub(line_start_ix);
            let position = line.position_for_index(local_index, self.line_height)?;
            return Some(point(position.x, position.y + line_origin_y));
        }

        Some(Point::default())
    }

    fn selection_quads(
        &self,
        range: Range<usize>,
        origin: Point<Pixels>,
        selection_color: u32,
    ) -> Vec<PaintQuad> {
        if let Some(mixed) = &self.mixed {
            return mixed
                .bounds_for_range(range, origin)
                .into_iter()
                .map(|bounds| fill(bounds, rgba(selection_color)))
                .collect();
        }
        if range.is_empty() {
            return Vec::new();
        }

        let mut quads = Vec::new();
        let mut line_start_ix = 0;
        let mut line_origin_y = px(0.);

        for line in &self.lines {
            for segment in visual_segments(line, line_start_ix, self.line_height) {
                let start = range.start.max(segment.global_start);
                let end = range.end.min(segment.global_end);

                if start >= end {
                    continue;
                }

                let start_x = if start == segment.global_start {
                    px(0.)
                } else {
                    line.position_for_index(start - line_start_ix, self.line_height)
                        .map(|position| position.x)
                        .unwrap_or(px(0.))
                };
                let end_x = if end == segment.global_end {
                    segment.width
                } else {
                    line.position_for_index(end - line_start_ix, self.line_height)
                        .map(|position| position.x)
                        .unwrap_or(segment.width)
                };

                if end_x <= start_x {
                    continue;
                }

                quads.push(fill(
                    Bounds::new(
                        point(origin.x + start_x, origin.y + line_origin_y + segment.top),
                        size(end_x - start_x, self.line_height),
                    ),
                    rgba(selection_color),
                ));
            }

            line_origin_y += line.size(self.line_height).height;
            line_start_ix = self.next_line_start(line_start_ix, line.len());
        }

        quads
    }

    fn bounds_for_range(&self, range: Range<usize>, origin: Point<Pixels>) -> Vec<Bounds<Pixels>> {
        if let Some(mixed) = &self.mixed {
            return mixed.bounds_for_range(range, origin);
        }
        if range.is_empty() {
            return Vec::new();
        }

        let mut bounds = Vec::new();
        let mut line_start_ix = 0;
        let mut line_origin_y = px(0.);

        for line in &self.lines {
            for segment in visual_segments(line, line_start_ix, self.line_height) {
                let start = range.start.max(segment.global_start);
                let end = range.end.min(segment.global_end);

                if start >= end {
                    continue;
                }

                let start_x = if start == segment.global_start {
                    px(0.)
                } else {
                    line.position_for_index(start - line_start_ix, self.line_height)
                        .map(|position| position.x)
                        .unwrap_or(px(0.))
                };
                let end_x = if end == segment.global_end {
                    segment.width
                } else {
                    line.position_for_index(end - line_start_ix, self.line_height)
                        .map(|position| position.x)
                        .unwrap_or(segment.width)
                };

                if end_x <= start_x {
                    continue;
                }

                bounds.push(Bounds::new(
                    point(origin.x + start_x, origin.y + line_origin_y + segment.top),
                    size(end_x - start_x, self.line_height),
                ));
            }

            line_origin_y += line.size(self.line_height).height;
            line_start_ix = self.next_line_start(line_start_ix, line.len());
        }

        bounds
    }
}

struct VisualLineSegment {
    global_start: usize,
    global_end: usize,
    top: Pixels,
    width: Pixels,
}

fn visual_segments(
    line: &WrappedLine,
    global_line_start: usize,
    line_height: Pixels,
) -> Vec<VisualLineSegment> {
    let mut segments = Vec::new();
    let mut segment_start = 0;
    let mut segment_start_x = px(0.);
    let mut top = px(0.);
    let boundaries = line
        .wrap_boundaries()
        .iter()
        .map(|boundary| {
            let run = &line.runs()[boundary.run_ix];
            let glyph = &run.glyphs[boundary.glyph_ix];
            (glyph.index, glyph.position.x)
        })
        .collect::<Vec<_>>();

    for (boundary_index, boundary_x) in boundaries {
        let width = (boundary_x - segment_start_x).max(px(0.));
        segments.push(VisualLineSegment {
            global_start: global_line_start + segment_start,
            global_end: global_line_start + boundary_index,
            top,
            width,
        });
        segment_start = boundary_index;
        segment_start_x = boundary_x;
        top += line_height;
    }

    let final_width = line
        .position_for_index(line.len(), line_height)
        .map(|position| position.x)
        .unwrap_or(px(0.));
    segments.push(VisualLineSegment {
        global_start: global_line_start + segment_start,
        global_end: global_line_start + line.len(),
        top,
        width: final_width.max(px(0.)),
    });

    if segments.is_empty() {
        segments.push(VisualLineSegment {
            global_start: global_line_start,
            global_end: global_line_start,
            top: px(0.),
            width: px(0.),
        });
    }

    segments
}

fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn word_range_at_offset(content: &str, offset: usize) -> Option<Range<usize>> {
    if content.is_empty() {
        return None;
    }

    let target = offset.min(content.len().saturating_sub(1));
    let mut previous_word = None;
    let mut next_word = None;
    for (start, word) in content.unicode_word_indices() {
        let end = start + word.len();
        if target >= start && target < end {
            return Some(start..end);
        }
        if end <= target {
            previous_word = Some(start..end);
            continue;
        }
        if start > target {
            next_word = Some(start..end);
            break;
        }
    }

    previous_word.or(next_word).or_else(|| {
        content
            .split_word_bound_indices()
            .find_map(|(start, segment)| {
                let end = start + segment.len();
                (target >= start && target < end).then_some(start..end)
            })
    })
}

fn line_range_at_offset(content: &str, offset: usize) -> Range<usize> {
    if content.is_empty() {
        return 0..0;
    }

    let cursor = offset.min(content.len());
    let start = content[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let end = content[cursor..]
        .find('\n')
        .map_or(content.len(), |index| cursor + index);
    start..end
}

fn resource_from_attachment_source(source: &str) -> Resource {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Resource::Embedded("".to_string().into());
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if trimmed.parse::<gpui::http_client::Uri>().is_ok() {
            return Resource::Uri(trimmed.to_string().into());
        }
        return Resource::Embedded(trimmed.to_string().into());
    }

    // Inline attachments for custom emoji are often absolute paths (or file:// URLs).
    // Use Resource::Path to ensure GPUI loads from the filesystem (not "embedded assets").
    let local_path = if lower.starts_with("file://") {
        crate::views::normalize_local_source_path(trimmed)
    } else {
        trimmed.to_string()
    };
    if !local_path.is_empty() && std::path::Path::new(&local_path).is_absolute() {
        return Resource::from(std::path::PathBuf::from(local_path));
    }

    Resource::Embedded(trimmed.to_string().into())
}

/// Inline images copy as their fallback text; annotations such as "(edited)" are
/// not part of the message and copy as nothing.
fn text_for_copy(
    content: &str,
    selected: Range<usize>,
    inline_attachments: &[InlineAttachment],
    styled_ranges: &[StyledRange],
) -> String {
    if selected.is_empty() {
        return String::new();
    }
    let mut replacements = inline_attachments
        .iter()
        .map(|attachment| {
            (
                attachment.byte_range.clone(),
                attachment.fallback_text.as_str(),
            )
        })
        .chain(
            styled_ranges
                .iter()
                .filter(|range| range.meta)
                .map(|range| (range.byte_range.clone(), "")),
        )
        .filter(|(range, _)| ranges_overlap(range, &selected))
        .collect::<Vec<_>>();
    if replacements.is_empty() {
        return content[selected].to_string();
    }
    replacements.sort_by_key(|(range, _)| range.start);

    let mut out = String::new();
    let mut cursor = selected.start;
    for (range, replacement) in replacements {
        let start = range.start.max(selected.start);
        let end = range.end.min(selected.end);
        if start >= end || start < cursor {
            continue;
        }
        out.push_str(&content[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    if cursor < selected.end {
        out.push_str(&content[cursor..selected.end]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_text_drops_annotations_and_expands_inline_images() {
        let content = "hi \u{FFFC} there (edited)";
        let image = content.find('\u{FFFC}').unwrap();
        let edited = content.find(" (edited)").unwrap();
        let attachments = [InlineAttachment {
            byte_range: image..image + '\u{FFFC}'.len_utf8(),
            source: String::new(),
            fallback_text: ":wave:".into(),
            size_px: 16,
        }];
        let styled = [StyledRange {
            byte_range: edited..content.len(),
            color: None,
            background_color: None,
            bold: false,
            italic: false,
            monospace: false,
            strikethrough: false,
            meta: true,
        }];
        assert_eq!(
            text_for_copy(content, 0..content.len(), &attachments, &styled),
            "hi :wave: there"
        );
        assert_eq!(text_for_copy(content, 0..2, &attachments, &styled), "hi");
        assert_eq!(text_for_copy(content, 0..0, &attachments, &styled), "");
    }

    #[test]
    fn deferred_text_layout_uses_captured_colors_after_parent_theme_ends() {
        use crate::{app::theme::ThemeVariant, views::with_theme};
        let dark = with_theme(ThemeVariant::Dark, SelectableTextColors::current);
        let light = with_theme(ThemeVariant::Light, SelectableTextColors::current);
        // Themes may legitimately share link colors. Include a distinct captured
        // style so this checks deferred rendering independently of palette choices.
        let configured = SelectableTextColors {
            link: 0x123456,
            selection: 0x654321ff,
        };
        for (captured, ambient) in [
            (dark, ThemeVariant::Light),
            (light, ThemeVariant::Dark),
            (configured, ThemeVariant::Light),
        ] {
            let runs = with_theme(ambient, || {
                build_text_runs_for_content(
                    "link",
                    &[LinkRange {
                        byte_range: 0..4,
                        url: "https://example.com".into(),
                    }],
                    &[],
                    rgb(0xd9d9d9).into(),
                    gpui::font(".SystemUIFont"),
                    captured.link,
                )
            });
            assert_eq!(runs[0].color, rgb(captured.link).into());
            assert_eq!(
                runs[0].underline.as_ref().unwrap().color,
                Some(rgb(captured.link).into())
            );
        }
        assert_eq!(dark.link, 0x6a9dff);
        assert_eq!(dark.selection, 0x4c8eff4d);
    }

    #[test]
    fn inline_code_keeps_its_font_and_colors_after_markdown_remaps_utf8_ranges() {
        use crate::views::inline_markdown::{InlineMarkdownConfig, apply_inline_markdown};
        let source = "**note** λvalue";
        let code = StyledRange::inline_code(9..source.len());
        let parsed = apply_inline_markdown(
            source,
            &[],
            &[code.clone()],
            InlineMarkdownConfig::default(),
        );
        assert_eq!(parsed.text, "note λvalue");
        let runs = build_text_runs_for_content(
            &parsed.text,
            &parsed.link_ranges,
            &parsed.styled_ranges,
            rgb(0x223344).into(),
            gpui::font(".SystemUIFont"),
            link(),
        );
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].font.family, gpui::font(".SystemUIFont").family);
        assert_eq!(runs[0].font.weight, FontWeight::BOLD);
        assert_eq!(runs[2].len, "λvalue".len());
        assert_eq!(
            runs[2].font.family,
            gpui::font(crate::views::mono_font_family()).family
        );
        assert_eq!(runs[2].color, rgb(code.color.unwrap()).into());
        assert_eq!(
            runs[2].background_color,
            Some(rgb(code.background_color.unwrap()).into())
        );
    }

    #[test]
    fn build_text_runs_merges_bold_italic_and_strike_styles() {
        let runs = build_text_runs_for_content(
            "abc",
            &[],
            &[
                StyledRange {
                    byte_range: 0..3,
                    color: None,
                    background_color: None,
                    bold: true,
                    italic: false,
                    monospace: false,
                    strikethrough: false,
                    meta: false,
                },
                StyledRange {
                    byte_range: 0..3,
                    color: None,
                    background_color: None,
                    bold: false,
                    italic: true,
                    monospace: false,
                    strikethrough: false,
                    meta: false,
                },
                StyledRange {
                    byte_range: 0..3,
                    color: None,
                    background_color: None,
                    bold: false,
                    italic: false,
                    monospace: false,
                    strikethrough: true,
                    meta: false,
                },
            ],
            rgb(0x223344).into(),
            gpui::font(".SystemUIFont"),
            link(),
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].font.weight, FontWeight::BOLD);
        assert_eq!(runs[0].font.style, FontStyle::Italic);
        assert!(runs[0].strikethrough.is_some());
    }

    #[test]
    fn build_text_runs_keeps_background_while_link_overrides_color() {
        let runs = build_text_runs_for_content(
            "hello",
            &[LinkRange {
                byte_range: 0..5,
                url: "https://example.com".to_string(),
            }],
            &[StyledRange {
                byte_range: 0..5,
                color: Some(0xff0000),
                background_color: Some(0x112233),
                bold: false,
                italic: false,
                monospace: false,
                strikethrough: false,
                meta: false,
            }],
            rgb(0x223344).into(),
            gpui::font(".SystemUIFont"),
            link(),
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].color, rgb(link()).into());
        assert_eq!(runs[0].background_color, Some(rgb(0x112233).into()));
        assert!(runs[0].underline.is_some());
    }

    #[test]
    fn build_text_runs_keeps_styled_color_for_internal_links() {
        let runs = build_text_runs_for_content(
            "@bob",
            &[LinkRange {
                byte_range: 0..4,
                url: "zbase-mention:bob".to_string(),
            }],
            &[StyledRange {
                byte_range: 0..4,
                color: Some(0x6588a7),
                background_color: Some(0xe9f0f5),
                bold: true,
                italic: false,
                monospace: false,
                strikethrough: false,
                meta: false,
            }],
            rgb(0x223344).into(),
            gpui::font(".SystemUIFont"),
            link(),
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].color, rgb(0x6588a7).into());
        assert_eq!(runs[0].background_color, Some(rgb(0xe9f0f5).into()));
        assert!(runs[0].underline.is_none());
    }

    #[test]
    fn build_text_runs_prefers_last_color_from_overlapping_styles() {
        let runs = build_text_runs_for_content(
            "abc",
            &[],
            &[
                StyledRange {
                    byte_range: 0..3,
                    color: Some(0xff0000),
                    background_color: None,
                    bold: false,
                    italic: false,
                    monospace: false,
                    strikethrough: false,
                    meta: false,
                },
                StyledRange {
                    byte_range: 0..3,
                    color: Some(0x00ff00),
                    background_color: None,
                    bold: false,
                    italic: false,
                    monospace: false,
                    strikethrough: false,
                    meta: false,
                },
            ],
            rgb(0x223344).into(),
            gpui::font(".SystemUIFont"),
            link(),
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].color, rgb(0x00ff00).into());
    }
}
