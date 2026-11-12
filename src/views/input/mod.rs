mod blink;
mod scrolling;

use gpui_kit::{
    App, Bounds, ClipboardEntry, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, Hsla,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    Style, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill, point,
    prelude::*, px, relative, rgb, rgba, size,
};
use std::{
    collections::VecDeque,
    ops::Range,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::views::selection;

const DEFAULT_TEXT_FIELD_PROFILE_MIN_MS: u128 = 2;
const PLACEHOLDER_OPACITY: f32 = 0.70;
/// Trimmed off the caret's top and bottom.
const CARET_VERTICAL_INSET_PX: f32 = 2.;
const ENV_INPUT_DISABLE_LAYOUT_CACHE: &str = "ZBASE_INPUT_DISABLE_LAYOUT_CACHE";
const ENV_INPUT_PROFILE_MIN_MS: &str = "ZBASE_INPUT_PROFILE_MIN_MS";
const ENV_INPUT_USE_LEGACY_REPLACE: &str = "ZBASE_INPUT_USE_LEGACY_REPLACE";
const ENV_INPUT_CLEAR_BOUNDS_ON_EDIT: &str = "ZBASE_INPUT_CLEAR_BOUNDS_ON_EDIT";
const ENV_INPUT_DISABLE_UNDO_COALESCE: &str = "ZBASE_INPUT_DISABLE_UNDO_COALESCE";

fn text_field_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("ZBASE_INPUT_PROFILE")
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn text_field_profile_min_ms() -> u128 {
    static MIN_MS: OnceLock<u128> = OnceLock::new();
    *MIN_MS.get_or_init(|| {
        std::env::var(ENV_INPUT_PROFILE_MIN_MS)
            .ok()
            .and_then(|raw| raw.trim().parse::<u128>().ok())
            .unwrap_or(DEFAULT_TEXT_FIELD_PROFILE_MIN_MS)
    })
}

fn text_field_layout_cache_disabled() -> bool {
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var(ENV_INPUT_DISABLE_LAYOUT_CACHE)
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn text_field_use_legacy_replace() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(ENV_INPUT_USE_LEGACY_REPLACE)
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn text_field_clear_bounds_on_edit() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(ENV_INPUT_CLEAR_BOUNDS_ON_EDIT)
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn text_field_undo_coalesce_disabled() -> bool {
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var(ENV_INPUT_DISABLE_UNDO_COALESCE)
            .ok()
            .map(|raw| {
                matches!(
                    raw.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

actions!(
    text_field,
    [
        Backspace,
        DeleteWordBackward,
        DeleteWordForward,
        MoveToPreviousWordStart,
        MoveToNextWordEnd,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectToPreviousWordStart,
        SelectToNextWordEnd,
        SelectUp,
        SelectDown,
        SelectAll,
        MoveToBeginningOfLine,
        MoveToEndOfLine,
        MoveToBeginningOfParagraph,
        MoveToEndOfParagraph,
        Home,
        End,
        SelectToBeginningOfLine,
        SelectToEndOfLine,
        SelectHome,
        SelectEnd,
        DeleteToBeginningOfLine,
        DeleteToEndOfLine,
        DeleteToEndOfParagraph,
        InsertNewline,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFieldKind {
    SingleLine,
    SingleLineWithContext { key_context: &'static str },
    Multiline { line_count: usize },
    AutoGrow { max_lines: usize },
}

impl TextFieldKind {
    fn is_multiline(self) -> bool {
        matches!(self, Self::Multiline { .. } | Self::AutoGrow { .. })
    }

    fn preferred_height(self, line_height: Pixels) -> Pixels {
        match self {
            Self::SingleLine | Self::SingleLineWithContext { .. } => line_height,
            Self::Multiline { line_count } => line_height * line_count as f32,
            Self::AutoGrow { .. } => line_height,
        }
    }

    fn key_context(self) -> &'static str {
        match self {
            Self::SingleLine => "TextField",
            Self::SingleLineWithContext { key_context } => key_context,
            Self::Multiline { .. } | Self::AutoGrow { .. } => "MultilineTextField",
        }
    }
}

struct UndoEntry {
    content: String,
    selected_range: Range<usize>,
    selection_reversed: bool,
}

const UNDO_STACK_MAX: usize = 200;
const UNDO_COALESCE_WINDOW: Duration = Duration::from_millis(220);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UndoEditKind {
    Insert,
    Delete,
    Replace,
}

pub struct TextField {
    focus_handle: FocusHandle,
    content: String,
    placeholder: String,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<TextLayoutSnapshot>,
    last_bounds: Option<Bounds<Pixels>>,
    last_platform_selection: Option<Range<usize>>,
    is_selecting: bool,
    field_kind: TextFieldKind,
    scroll_y: Pixels,
    scroll_to_cursor: bool,
    selection_pointer: Option<Point<Pixels>>,
    selection_scroll_task: Option<gpui_kit::Task<()>>,
    /// Where Up and Down aim across lines, so a short line on the way doesn't
    /// pull the cursor left. Kept only while they are what moved the cursor.
    vertical_goal: Option<VerticalGoal>,
    /// The cursor sits where a long line wraps and belongs to the start of the
    /// row below, not the end of the row above: Up, Down or a click put it
    /// there. Where a line wraps, both are the same text offset.
    cursor_on_lower_row: Option<CursorMark>,
    pub pasted_image: Option<Vec<u8>>,
    last_content_height: Pixels,
    undo_stack: VecDeque<UndoEntry>,
    redo_stack: VecDeque<UndoEntry>,
    last_undo_snapshot_at: Option<Instant>,
    last_undo_edit_kind: Option<UndoEditKind>,
    pending_height_edit_at: Option<Instant>,
    blink: blink::CaretBlink,
}

impl TextField {
    pub fn new(
        focus_handle: FocusHandle,
        placeholder: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::with_kind(
            focus_handle,
            placeholder,
            content,
            TextFieldKind::SingleLine,
        )
    }

    pub fn new_with_key_context(
        focus_handle: FocusHandle,
        placeholder: impl Into<String>,
        content: impl Into<String>,
        key_context: &'static str,
    ) -> Self {
        Self::with_kind(
            focus_handle,
            placeholder,
            content,
            TextFieldKind::SingleLineWithContext { key_context },
        )
    }

    pub fn new_multiline(
        focus_handle: FocusHandle,
        placeholder: impl Into<String>,
        content: impl Into<String>,
        line_count: usize,
    ) -> Self {
        Self::with_kind(
            focus_handle,
            placeholder,
            content,
            TextFieldKind::Multiline {
                line_count: line_count.max(2),
            },
        )
    }

    pub fn new_auto_grow(
        focus_handle: FocusHandle,
        placeholder: impl Into<String>,
        content: impl Into<String>,
        max_lines: usize,
    ) -> Self {
        Self::with_kind(
            focus_handle,
            placeholder,
            content,
            TextFieldKind::AutoGrow {
                max_lines: max_lines.max(1),
            },
        )
    }

    fn with_kind(
        focus_handle: FocusHandle,
        placeholder: impl Into<String>,
        content: impl Into<String>,
        field_kind: TextFieldKind,
    ) -> Self {
        let content = content.into();
        let cursor = content.len();

        Self {
            focus_handle,
            content,
            placeholder: placeholder.into(),
            selected_range: cursor..cursor,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_platform_selection: None,
            is_selecting: false,
            field_kind,
            scroll_y: px(0.),
            scroll_to_cursor: true,
            selection_pointer: None,
            selection_scroll_task: None,
            vertical_goal: None,
            cursor_on_lower_row: None,
            pasted_image: None,
            last_content_height: px(0.),
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            last_undo_snapshot_at: None,
            last_undo_edit_kind: None,
            pending_height_edit_at: None,
            blink: blink::CaretBlink::default(),
        }
    }

    pub fn text(&self) -> String {
        self.content.clone()
    }

    /// True when the field holds only whitespace; cheaper than `text()` per keystroke.
    pub fn is_blank(&self) -> bool {
        self.content.trim().is_empty()
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<Self>) {
        let placeholder = placeholder.into();
        if self.placeholder == placeholder {
            return;
        }
        self.placeholder = placeholder;
        self.clear_layout_cache();
        cx.notify();
    }

    pub fn text_if_different(&self, current: &str) -> Option<String> {
        (self.content != current).then(|| self.content.clone())
    }

    pub fn is_multiline(&self) -> bool {
        self.field_kind.is_multiline()
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        if self.content == text {
            return;
        }

        let cursor = text.len();
        self.content = text;
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.scroll_y = px(0.);
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
    }

    pub fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let profile_enabled = text_field_profile_enabled();
        let started_at = profile_enabled.then(Instant::now);
        let text = if self.is_multiline() {
            text.to_string()
        } else {
            text.replace('\n', " ")
        };

        let range = self
            .marked_range
            .clone()
            .unwrap_or(self.selected_range.clone());
        let replaced_len = range.end.saturating_sub(range.start);
        let inserted_len = text.len();
        self.push_undo_snapshot(
            undo_edit_kind(replaced_len, inserted_len),
            self.marked_range.is_none() && inserted_len <= 1 && replaced_len <= 1,
        );
        self.replace_content_range(range.clone(), &text);
        let cursor = range.start + text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
        if let Some(started_at) = started_at {
            let elapsed = started_at.elapsed();
            if elapsed.as_millis() >= text_field_profile_min_ms() {
                tracing::warn!(
                    target: "zbase.input.perf",
                    phase = "insert_text",
                    elapsed_ms = elapsed.as_millis(),
                    elapsed_us = elapsed.as_micros(),
                    content_len = self.content.len(),
                    replacement_len = text.len(),
                    is_multiline = self.is_multiline(),
                    "slow_text_field_path"
                );
            }
        }
    }

    pub fn replace_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) {
        let clamped_start = range.start.min(self.content.len());
        let clamped_end = range.end.min(self.content.len()).max(clamped_start);
        let clamped = clamped_start..clamped_end;
        let replaced_len = clamped.end.saturating_sub(clamped.start);
        let inserted_len = replacement.len();
        self.push_undo_snapshot(undo_edit_kind(replaced_len, inserted_len), false);
        self.replace_content_range(clamped.clone(), replacement);
        let cursor = clamped.start + replacement.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
    }

    fn push_undo_snapshot(&mut self, kind: UndoEditKind, coalesce: bool) {
        if coalesce
            && !text_field_undo_coalesce_disabled()
            && let (Some(last_snapshot_at), Some(last_kind)) =
                (self.last_undo_snapshot_at, self.last_undo_edit_kind)
            && last_kind == kind
            && last_snapshot_at.elapsed() <= UNDO_COALESCE_WINDOW
        {
            return;
        }
        self.undo_stack.push_back(UndoEntry {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        });
        if self.undo_stack.len() > UNDO_STACK_MAX {
            let _ = self.undo_stack.pop_front();
        }
        self.redo_stack.clear();
        self.last_undo_snapshot_at = Some(Instant::now());
        self.last_undo_edit_kind = Some(kind);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.undo_stack.pop_back() else {
            return;
        };
        self.redo_stack.push_back(UndoEntry {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        });
        self.content = entry.content;
        self.selected_range = entry.selected_range;
        self.selection_reversed = entry.selection_reversed;
        self.marked_range = None;
        self.last_undo_snapshot_at = None;
        self.last_undo_edit_kind = None;
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.redo_stack.pop_back() else {
            return;
        };
        self.undo_stack.push_back(UndoEntry {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        });
        self.content = entry.content;
        self.selected_range = entry.selected_range;
        self.selection_reversed = entry.selection_reversed;
        self.marked_range = None;
        self.last_undo_snapshot_at = None;
        self.last_undo_edit_kind = None;
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
    }

    fn clear_layout_cache(&mut self) {
        self.last_layout = None;
        if text_field_clear_bounds_on_edit() {
            self.last_bounds = None;
        }
    }

    fn mark_content_edited(&mut self) {
        self.scroll_to_cursor = true;
        self.stop_selection_drag();
        self.pending_height_edit_at = Some(Instant::now());
    }

    fn replace_content_range(&mut self, range: Range<usize>, replacement: &str) {
        if text_field_use_legacy_replace() {
            self.content =
                self.content[0..range.start].to_owned() + replacement + &self.content[range.end..];
        } else {
            self.content.replace_range(range, replacement);
        }
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
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn move_to_previous_word_start(
        &mut self,
        _: &MoveToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.move_to(previous_word_start(&self.content, self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn move_to_next_word_end(
        &mut self,
        _: &MoveToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.move_to(next_word_end(&self.content, self.selected_range.end), cx);
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

    fn select_to_previous_word_start(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(previous_word_start(&self.content, self.cursor_offset()), cx);
    }

    fn select_to_next_word_end(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(next_word_end(&self.content, self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_vertical(-1.0, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_vertical(1.0, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn move_to_beginning_of_line(
        &mut self,
        _: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = if self.selected_range.is_empty() {
            self.cursor_offset()
        } else {
            self.selected_range.start
        };
        let start = self.row_start(offset);
        self.move_to(start, cx);
        self.keep_row_start_affinity(start);
    }

    fn move_to_end_of_line(&mut self, _: &MoveToEndOfLine, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selected_range.is_empty() {
            self.cursor_offset()
        } else {
            self.selected_range.end
        };
        let end = self.row_end(offset);
        self.move_to(end, cx);
        self.cursor_on_lower_row = None;
    }

    /// Ctrl+A: the start of the line, across the rows it wraps onto.
    fn move_to_beginning_of_paragraph(
        &mut self,
        _: &MoveToBeginningOfParagraph,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = if self.selected_range.is_empty() {
            self.cursor_offset()
        } else {
            self.selected_range.start
        };
        self.move_to(current_line_start(&self.content, offset), cx);
    }

    /// Ctrl+E: the end of the line, across the rows it wraps onto.
    fn move_to_end_of_paragraph(
        &mut self,
        _: &MoveToEndOfParagraph,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = if self.selected_range.is_empty() {
            self.cursor_offset()
        } else {
            self.selected_range.end
        };
        self.move_to(current_line_end(&self.content, offset), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn select_to_beginning_of_line(
        &mut self,
        _: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.row_start(self.cursor_offset());
        self.select_to(start, cx);
        self.keep_row_start_affinity(start);
    }

    fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let end = self.row_end(self.cursor_offset());
        self.select_to(end, cx);
        self.cursor_on_lower_row = None;
    }

    fn insert_newline(&mut self, _: &InsertNewline, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_multiline() {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_forward(
        &mut self,
        _: &DeleteWordForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(next_word_end(&self.content, self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_backward(
        &mut self,
        _: &DeleteWordBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(
                delete_to_previous_word_start(&self.content, self.cursor_offset()),
                cx,
            );
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_beginning_of_line(
        &mut self,
        _: &DeleteToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.row_start(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_end_of_line(
        &mut self,
        _: &DeleteToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.row_end(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Ctrl+K: delete to the end of the line, across the rows it wraps onto.
    fn delete_to_end_of_paragraph(
        &mut self,
        _: &DeleteToEndOfParagraph,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(current_line_end(&self.content, self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_selection_drag();
        self.is_selecting = true;
        self.selection_pointer = Some(event.position);
        window.focus(&self.focus_handle(cx), cx);
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
            // Where a line wraps, a click at the start of the lower row is the
            // same offset as the end of the row above; keep the row clicked.
            let on_lower_row = self
                .local_mouse_position(event.position)
                .zip(self.last_layout.as_ref())
                .is_some_and(|(position, layout)| layout.is_on_lower_row(offset, position.y));
            self.cursor_on_lower_row = on_lower_row.then(|| self.cursor_mark());
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let up_offset = self.selection_index_for_position(event.position);
            // For double/triple click selection, mouse-down may have already established a full
            // word/line range. Avoid shrinking it on mouse-up when releasing within that range.
            if self.selected_range.is_empty()
                || !(self.selected_range.start <= up_offset && up_offset <= self.selected_range.end)
            {
                self.select_to(up_offset, cx);
            }
            self.stop_selection_drag();
            self.scroll_to_cursor = false;
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            if event.pressed_button != Some(MouseButton::Left) {
                self.stop_selection_drag();
                return;
            }
            self.selection_pointer = Some(event.position);
            self.select_to(self.selection_index_for_position(event.position), cx);
            self.scroll_to_cursor = false;
            self.scroll_selection_once(cx);
            self.start_selection_scroll(window, cx);
            cx.stop_propagation();
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if let Some(text) = item.text() {
            let normalized = if self.is_multiline() {
                text
            } else {
                text.replace('\n', " ")
            };
            self.replace_text_in_range(None, &normalized, window, cx);
            return;
        }
        for entry in item.entries() {
            if let ClipboardEntry::Image(image) = entry {
                let bytes = image.bytes().to_vec();
                if !bytes.is_empty() {
                    self.pasted_image = Some(bytes);
                    cx.notify();
                    return;
                }
            }
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.forget_cursor_marks_unless_at(offset);
        self.scroll_to_cursor = true;
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn move_vertical(&mut self, direction: f32, cx: &mut Context<Self>) {
        if let Some(target) = self.vertical_target(direction) {
            self.move_to(target.offset, cx);
            self.keep_vertical_target(target);
        }
    }

    fn select_vertical(&mut self, direction: f32, cx: &mut Context<Self>) {
        if let Some(target) = self.vertical_target(direction) {
            self.select_to(target.offset, cx);
            self.keep_vertical_target(target);
        }
    }

    /// Where the cursor goes one row up (`direction` -1) or down (1). Above the
    /// first row is the start of the text, below the last its end.
    fn vertical_target(&self, direction: f32) -> Option<VerticalTarget> {
        if !self.is_multiline() {
            return None;
        }
        let layout = self.last_layout.as_ref()?;
        let position =
            layout.cursor_position(self.cursor_offset(), self.cursor_is_on_lower_row())?;
        let x = self
            .vertical_goal
            .filter(|goal| goal.at == self.cursor_mark())
            .map_or(position.x, |goal| goal.x);
        // `position` is the top of the cursor's row. Aim at the middle of the
        // next one: its top edge belongs to the row above.
        let y = position.y + layout.line_height * (direction + 0.5);
        let offset = layout.closest_index_for_position(point(x, y));
        Some(VerticalTarget {
            offset,
            x,
            on_lower_row: layout.is_on_lower_row(offset, y),
        })
    }

    fn keep_vertical_target(&mut self, target: VerticalTarget) {
        let at = self.cursor_mark();
        self.vertical_goal = Some(VerticalGoal { at, x: target.x });
        self.cursor_on_lower_row = target.on_lower_row.then_some(at);
    }

    /// The start of the visible row holding `offset`: where a long line wraps,
    /// one row of it, as Cmd+Left does on macOS.
    fn row_start(&self, offset: usize) -> usize {
        self.row_range(offset).map_or_else(
            || current_line_start(&self.content, offset),
            |row| row.start,
        )
    }

    /// The end of the visible row holding `offset`, as Cmd+Right does.
    fn row_end(&self, offset: usize) -> usize {
        self.row_range(offset)
            .map_or_else(|| current_line_end(&self.content, offset), |row| row.end)
    }

    fn row_range(&self, offset: usize) -> Option<Range<usize>> {
        let on_lower_row = offset == self.cursor_offset() && self.cursor_is_on_lower_row();
        self.last_layout.as_ref()?.row_range(offset, on_lower_row)
    }

    /// A row's start where a line wraps is also the end of the row above;
    /// keep the cursor on the row it was moved to the start of.
    fn keep_row_start_affinity(&mut self, start: usize) {
        let wraps_here = self
            .last_layout
            .as_ref()
            .is_some_and(|layout| layout.lower_row_position(start).is_some());
        self.cursor_on_lower_row = wraps_here.then(|| self.cursor_mark());
    }

    fn cursor_mark(&self) -> CursorMark {
        CursorMark {
            offset: self.cursor_offset(),
            text_len: self.content.len(),
        }
    }

    fn cursor_is_on_lower_row(&self) -> bool {
        self.cursor_on_lower_row == Some(self.cursor_mark())
    }

    /// Moving the cursor elsewhere makes the remembered column and row stale.
    fn forget_cursor_marks_unless_at(&mut self, offset: usize) {
        if offset != self.cursor_offset() {
            self.vertical_goal = None;
            self.cursor_on_lower_row = None;
        }
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    /// Replaces the whole text as one undoable edit and selects `selection`.
    pub fn replace_all_and_select(
        &mut self,
        text: String,
        selection: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let len = self.content.len();
        self.replace_range(0..len, &text, cx);
        let end = selection.end.min(self.content.len());
        let start = selection.start.min(end);
        if self.content.is_char_boundary(start) && self.content.is_char_boundary(end) {
            self.selected_range = start..end;
            self.selection_reversed = false;
            self.scroll_to_cursor = true;
        }
        cx.notify();
    }

    /// Where the caret is drawn, in the text's own coordinates.
    #[cfg(test)]
    pub(crate) fn caret_position(&self) -> Option<(Point<Pixels>, Pixels)> {
        let layout = self.last_layout.as_ref()?;
        let position =
            layout.cursor_position(self.cursor_offset(), self.cursor_is_on_lower_row())?;
        Some((position, layout.line_height))
    }

    /// A point in the text's own coordinates, in window coordinates.
    #[cfg(test)]
    pub(crate) fn window_point(&self, local: Point<Pixels>) -> Option<Point<Pixels>> {
        let bounds = self.last_bounds.as_ref()?;
        Some(point(
            bounds.origin.x + local.x,
            bounds.origin.y + local.y - self.scroll_y,
        ))
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(local_position), Some(layout)) = (
            self.local_mouse_position(position),
            self.last_layout.as_ref(),
        ) else {
            return 0;
        };
        layout.closest_index_for_position(local_position)
    }

    /// `position` in the text's own coordinates, scrolling included.
    fn local_mouse_position(&self, position: Point<Pixels>) -> Option<Point<Pixels>> {
        let bounds = self.last_bounds.as_ref()?;
        Some(point(
            position.x - bounds.origin.x,
            position.y - bounds.origin.y + self.scroll_y,
        ))
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.forget_cursor_marks_unless_at(offset);
        self.scroll_to_cursor = true;
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

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.take().is_some() {
            self.clear_layout_cache();
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile_enabled = text_field_profile_enabled();
        let started_at = profile_enabled.then(Instant::now);
        let replacement = if self.is_multiline() {
            new_text.to_string()
        } else {
            new_text.replace('\n', " ")
        };
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let replaced_len = range.end.saturating_sub(range.start);
        let inserted_len = replacement.len();
        self.push_undo_snapshot(
            undo_edit_kind(replaced_len, inserted_len),
            self.marked_range.is_none() && inserted_len <= 1 && replaced_len <= 1,
        );
        self.replace_content_range(range.clone(), &replacement);
        let cursor = range.start + replacement.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range.take();
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
        if let Some(started_at) = started_at {
            let elapsed = started_at.elapsed();
            if elapsed.as_millis() >= text_field_profile_min_ms() {
                tracing::warn!(
                    target: "zbase.input.perf",
                    phase = "replace_text_in_range",
                    elapsed_ms = elapsed.as_millis(),
                    elapsed_us = elapsed.as_micros(),
                    content_len = self.content.len(),
                    replacement_len = replacement.len(),
                    is_multiline = self.is_multiline(),
                    "slow_text_field_path"
                );
            }
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let replacement = if self.is_multiline() {
            new_text.to_string()
        } else {
            new_text.replace('\n', " ")
        };
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let replaced_len = range.end.saturating_sub(range.start);
        let inserted_len = replacement.len();
        self.push_undo_snapshot(undo_edit_kind(replaced_len, inserted_len), false);
        self.replace_content_range(range.clone(), &replacement);
        if !replacement.is_empty() {
            self.marked_range = Some(range.start..range.start + replacement.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| range.start + new_range.start..range.start + new_range.end)
            .unwrap_or_else(|| range.start + replacement.len()..range.start + replacement.len());
        self.mark_content_edited();
        self.clear_layout_cache();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        self.last_layout.as_ref().and_then(|layout| {
            layout.bounds_for_range(
                range,
                point(bounds.origin.x, bounds.origin.y - self.scroll_y),
            )
        })
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let local_point = self.last_bounds?.localize(&position)?;
        let local_point = point(local_point.x, local_point.y + self.scroll_y);
        let index = self
            .last_layout
            .as_ref()?
            .closest_index_for_position(local_point);
        Some(self.offset_to_utf16(index))
    }
}

struct TextLineElement {
    input: Entity<TextField>,
}

struct PrepaintState {
    layout: TextLayoutSnapshot,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
    scroll_y: Pixels,
}

impl IntoElement for TextLineElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextLineElement {
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
        _inspector_id: Option<&gpui_kit::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let input = self.input.read(cx);
        let field_kind = input.field_kind;
        let height = match field_kind {
            TextFieldKind::AutoGrow { max_lines } => {
                let line_height = window.line_height();
                let max_height = line_height * max_lines as f32;
                let wrap_width = input
                    .last_bounds
                    .as_ref()
                    .map(|b| b.size.width.max(px(1.0)));
                let profile_enabled = text_field_profile_enabled();
                let profile_min_ms = text_field_profile_min_ms();
                let content_height = if let Some(wrap_width) = wrap_width {
                    let text_style = window.text_style();
                    let is_placeholder = input.content.is_empty();
                    let display_text = if input.content.is_empty() {
                        &input.placeholder
                    } else {
                        &input.content
                    };
                    let text_color = if is_placeholder {
                        text_style.color.opacity(PLACEHOLDER_OPACITY)
                    } else {
                        text_style.color
                    };
                    let font_size = text_style.font_size.to_pixels(window.rem_size());
                    if !text_field_layout_cache_disabled()
                        && let Some(cached_layout) = input.last_layout.as_ref().filter(|layout| {
                            layout.matches_shape_state(
                                is_placeholder,
                                input.marked_range.as_ref(),
                                Some(wrap_width),
                                font_size,
                                line_height,
                                text_color,
                            )
                        })
                    {
                        cached_layout.total_height()
                    } else {
                        let run = TextRun {
                            len: display_text.len(),
                            font: text_style.font(),
                            color: text_color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shape_started_at = profile_enabled.then(Instant::now);
                        let shaped_height = window
                            .text_system()
                            .shape_text(
                                display_text.clone().into(),
                                font_size,
                                &[run],
                                Some(wrap_width),
                                None,
                            )
                            .map(|lines| {
                                lines
                                    .iter()
                                    .fold(px(0.), |h, line| h + line.size(line_height).height)
                            })
                            .unwrap_or(line_height);
                        if let Some(shape_started_at) = shape_started_at {
                            let elapsed = shape_started_at.elapsed();
                            if elapsed.as_millis() >= profile_min_ms {
                                tracing::warn!(
                                    target: "zbase.input.perf",
                                    phase = "request_layout.shape_text",
                                    elapsed_ms = elapsed.as_millis(),
                                    elapsed_us = elapsed.as_micros(),
                                    content_len = display_text.len(),
                                    wrap_width = ?wrap_width,
                                    "slow_text_field_path"
                                );
                            }
                        }
                        shaped_height
                    }
                } else {
                    input.last_content_height.max(line_height)
                };
                content_height.min(max_height).max(line_height)
            }
            _ => field_kind.preferred_height(window.line_height()),
        };
        style.size.height = height.into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui_kit::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let is_focused = self.input.read(cx).focus_handle.is_focused(window);
        // Like native fields, the caret hides while the window is in the background.
        let caret_active = is_focused && window.is_window_active();
        let window_handle = window.window_handle();
        let caret_visible = self.input.update(cx, |input, cx| {
            input.sync_caret_blink(caret_active, window_handle, cx)
        });
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let cursor_on_lower_row = input.cursor_is_on_lower_row();
        let style = window.text_style();

        let (display_text, text_color, is_placeholder) = if content.is_empty() {
            (
                input.placeholder.clone(),
                style.color.opacity(PLACEHOLDER_OPACITY),
                true,
            )
        } else {
            (content.clone(), style.color, false)
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let wrap_width = input
            .is_multiline()
            .then_some(bounds.size.width.max(px(1.0)));
        let line_height = window.line_height();
        let layout = if !text_field_layout_cache_disabled()
            && let Some(cached_layout) = input.last_layout.as_ref().filter(|layout| {
                layout.matches_shape_state(
                    is_placeholder,
                    input.marked_range.as_ref(),
                    wrap_width,
                    font_size,
                    line_height,
                    text_color,
                )
            }) {
            cached_layout.clone()
        } else {
            let run = TextRun {
                len: display_text.len(),
                font: style.font(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = if let Some(marked_range) = input.marked_range.as_ref() {
                vec![
                    TextRun {
                        len: marked_range.start,
                        ..run.clone()
                    },
                    TextRun {
                        len: marked_range.end - marked_range.start,
                        underline: Some(UnderlineStyle {
                            color: Some(run.color),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..run.clone()
                    },
                    TextRun {
                        len: display_text.len() - marked_range.end,
                        ..run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            } else {
                vec![run]
            };

            let profile_enabled = text_field_profile_enabled();
            let profile_min_ms = text_field_profile_min_ms();
            let shape_started_at = profile_enabled.then(Instant::now);
            let lines = window
                .text_system()
                .shape_text(display_text.into(), font_size, &runs, wrap_width, None)
                .map(|lines| lines.into_iter().map(Arc::new).collect())
                .unwrap_or_default();
            if let Some(shape_started_at) = shape_started_at {
                let elapsed = shape_started_at.elapsed();
                if elapsed.as_millis() >= profile_min_ms {
                    tracing::warn!(
                        target: "zbase.input.perf",
                        phase = "prepaint.shape_text",
                        elapsed_ms = elapsed.as_millis(),
                        elapsed_us = elapsed.as_micros(),
                        content_len = content.len(),
                        wrap_width = ?wrap_width,
                        is_placeholder,
                        "slow_text_field_path"
                    );
                }
            }

            TextLayoutSnapshot {
                lines,
                line_height,
                is_placeholder,
                text_len: content.len(),
                content,
                wrap_width,
                font_size,
                text_color,
                marked_range: input.marked_range.clone(),
            }
        };
        let scroll_y = if input.is_multiline() {
            visible_scroll_offset(
                &layout,
                input.scroll_y,
                bounds.size.height,
                cursor,
                cursor_on_lower_row,
                input.scroll_to_cursor,
            )
        } else {
            px(0.)
        };
        let paint_origin = point(bounds.origin.x, bounds.origin.y - scroll_y);

        let selection = if !is_focused || selected_range.is_empty() || layout.is_placeholder {
            Vec::new()
        } else {
            layout.selection_quads(selected_range, paint_origin)
        };

        let cursor = if caret_visible && selection.is_empty() {
            let cursor_pos = layout
                .cursor_position(cursor, cursor_on_lower_row)
                .unwrap_or_default();
            // A little shorter than the line, centered on it, like a native caret.
            let inset = px(CARET_VERTICAL_INSET_PX);
            Some(fill(
                Bounds::new(
                    point(
                        bounds.left() + cursor_pos.x,
                        bounds.top() + cursor_pos.y - scroll_y + inset,
                    ),
                    size(px(1.), (layout.line_height - inset * 2.).max(px(1.))),
                ),
                // The inherited text color, like a native caret, not the accent.
                // Not `text_primary()`: the palette's theme scope has ended by
                // prepaint, so it would always return the light theme's color.
                style.color,
            ))
        } else {
            None
        };

        PrepaintState {
            layout,
            cursor,
            selection,
            scroll_y,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui_kit::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        // Register capture before a drag begins, so a fast pointer exit does not
        // depend on the next frame installing listeners after mouse-down.
        let input = self.input.downgrade();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == gpui_kit::DispatchPhase::Capture
                && let Some(input) = input.upgrade()
                && input.read(cx).is_selecting
            {
                input.update(cx, |input, cx| input.on_mouse_move(event, window, cx));
            }
        });
        let input = self.input.downgrade();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == gpui_kit::DispatchPhase::Capture
                && event.button == MouseButton::Left
                && let Some(input) = input.upgrade()
                && input.read(cx).is_selecting
            {
                input.update(cx, |input, cx| {
                    input.on_mouse_up(event, window, cx);
                    cx.stop_propagation();
                });
            }
        });

        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }

        // The placeholder starts where typed text would, under the caret.
        let mut line_origin = point(bounds.origin.x, bounds.origin.y - prepaint.scroll_y);
        let align = window.text_style().text_align;
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

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        let content_height = prepaint.layout.total_height();
        let layout = std::mem::take(&mut prepaint.layout);
        let scroll_y = prepaint.scroll_y;
        let focused = focus_handle.is_focused(window);
        let (text_changed, selection_changed) = self.input.update(cx, |input, _cx| {
            let text_changed = input.last_layout.as_ref().is_none_or(|previous| {
                previous.content != layout.content || previous.marked_range != layout.marked_range
            });
            let selection_changed = input.last_platform_selection.as_ref()
                != Some(&input.selected_range)
                || input.last_bounds != Some(bounds)
                || input.scroll_y != scroll_y;
            if focused {
                input.last_platform_selection = Some(input.selected_range.clone());
            }
            let previous_content_height = input.last_content_height;
            input.last_content_height = content_height;
            input.last_layout = Some(layout);
            input.last_bounds = Some(bounds);
            input.scroll_y = scroll_y;
            input.scroll_to_cursor = false;
            if !pixels_match(previous_content_height, content_height)
                && let Some(started_at) = input.pending_height_edit_at.take()
            {
                let elapsed = started_at.elapsed();
                if text_field_profile_enabled()
                    && elapsed.as_millis() >= text_field_profile_min_ms()
                {
                    tracing::warn!(
                        target: "zbase.input.perf",
                        phase = "content_height_update_lag",
                        elapsed_ms = elapsed.as_millis(),
                        elapsed_us = elapsed.as_micros(),
                        previous_height = f32::from(previous_content_height),
                        next_height = f32::from(content_height),
                        "slow_text_field_path"
                    );
                }
            }
            (text_changed, selection_changed)
        });
        // Publish native text-service state after committing layout and dropping
        // the entity borrow. AppKit may immediately ask for caret/IME geometry.
        if focused {
            if text_changed {
                window.notify_text_input_changed(&focus_handle);
            } else if selection_changed {
                window.notify_text_selection_changed(&focus_handle);
            }
        }
    }
}

impl Render for TextField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .overflow_hidden()
            .key_context(self.field_kind.key_context())
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete_word_backward))
            .on_action(cx.listener(Self::delete_word_forward))
            .on_action(cx.listener(Self::move_to_previous_word_start))
            .on_action(cx.listener(Self::move_to_next_word_end))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_to_previous_word_start))
            .on_action(cx.listener(Self::select_to_next_word_end))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::move_to_beginning_of_line))
            .on_action(cx.listener(Self::move_to_end_of_line))
            .on_action(cx.listener(Self::move_to_beginning_of_paragraph))
            .on_action(cx.listener(Self::move_to_end_of_paragraph))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_to_beginning_of_line))
            .on_action(cx.listener(Self::select_to_end_of_line))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::delete_to_beginning_of_line))
            .on_action(cx.listener(Self::delete_to_end_of_line))
            .on_action(cx.listener(Self::delete_to_end_of_paragraph))
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll(cx.listener(Self::on_scroll))
            .child(TextLineElement { input: cx.entity() })
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The cursor offset and text length something about the cursor was learned
/// at; once either changes, it no longer applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorMark {
    offset: usize,
    text_len: usize,
}

/// See `TextField::vertical_goal`.
#[derive(Clone, Copy, Debug)]
struct VerticalGoal {
    at: CursorMark,
    x: Pixels,
}

#[derive(Clone, Copy, Debug)]
struct VerticalTarget {
    offset: usize,
    /// The x it aimed at, kept as the column for the next Up or Down.
    x: Pixels,
    on_lower_row: bool,
}

#[derive(Clone)]
struct TextLayoutSnapshot {
    lines: Vec<Arc<WrappedLine>>,
    line_height: Pixels,
    is_placeholder: bool,
    text_len: usize,
    content: String,
    wrap_width: Option<Pixels>,
    font_size: Pixels,
    text_color: Hsla,
    marked_range: Option<Range<usize>>,
}

impl Default for TextLayoutSnapshot {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            line_height: px(0.),
            is_placeholder: false,
            text_len: 0,
            content: String::new(),
            wrap_width: None,
            font_size: px(0.),
            text_color: rgb(0).into(),
            marked_range: None,
        }
    }
}

impl TextLayoutSnapshot {
    fn matches_shape_state(
        &self,
        is_placeholder: bool,
        marked_range: Option<&Range<usize>>,
        wrap_width: Option<Pixels>,
        font_size: Pixels,
        line_height: Pixels,
        text_color: Hsla,
    ) -> bool {
        self.is_placeholder == is_placeholder
            && self.marked_range.as_ref() == marked_range
            && optional_pixels_match(self.wrap_width, wrap_width)
            && pixels_match(self.font_size, font_size)
            && pixels_match(self.line_height, line_height)
            && self.text_color == text_color
    }

    fn next_line_start(&self, line_start_ix: usize, line_len: usize) -> usize {
        let mut next = line_start_ix.saturating_add(line_len);
        if self.content.as_bytes().get(next) == Some(&b'\n') {
            next += 1;
        }
        next
    }

    fn total_height(&self) -> Pixels {
        self.lines.iter().fold(px(0.), |height, line| {
            height + line.size(self.line_height).height
        })
    }

    fn closest_index_for_position(&self, position: Point<Pixels>) -> usize {
        if self.is_placeholder {
            return 0;
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

    /// Where the cursor is drawn and measured from: at a wrap, the start of
    /// the lower row when it belongs there, else the end of the upper one.
    fn cursor_position(&self, index: usize, on_lower_row: bool) -> Option<Point<Pixels>> {
        on_lower_row
            .then(|| self.lower_row_position(index))
            .flatten()
            .or_else(|| self.position_for_index(index))
    }

    /// Whether `index`, where a line wraps, reads as the start of the row
    /// below rather than the end of the row above, for a point at height `y`.
    fn is_on_lower_row(&self, index: usize, y: Pixels) -> bool {
        self.lower_row_position(index)
            .is_some_and(|row_start| y >= row_start.y)
    }

    /// The visible row holding `index`, as text offsets. Where a line wraps,
    /// `index` is the end of the upper row unless `on_lower_row`.
    fn row_range(&self, index: usize, on_lower_row: bool) -> Option<Range<usize>> {
        let mut line_start_ix = 0;
        for line in &self.lines {
            let line_end_ix = line_start_ix + line.len();
            if index > line_end_ix {
                line_start_ix = self.next_line_start(line_start_ix, line.len());
                continue;
            }
            let local_index = index.checked_sub(line_start_ix)?;
            let mut row_start = 0;
            for row_end in wrap_points(line).chain([line.len()]) {
                let wraps_here = row_end < line.len();
                if local_index < row_end
                    || (local_index == row_end && !(wraps_here && on_lower_row))
                {
                    return Some(line_start_ix + row_start..line_start_ix + row_end);
                }
                row_start = row_end;
            }
            return Some(line_start_ix + row_start..line_end_ix);
        }
        None
    }

    /// The start of the row below the wrap at `index`; `None` when no line
    /// wraps there.
    fn lower_row_position(&self, index: usize) -> Option<Point<Pixels>> {
        let mut line_origin_y = px(0.);
        let mut line_start_ix = 0;
        for line in &self.lines {
            let line_end_ix = line_start_ix + line.len();
            if index > line_end_ix {
                line_origin_y += line.size(self.line_height).height;
                line_start_ix = self.next_line_start(line_start_ix, line.len());
                continue;
            }
            let local_index = index.checked_sub(line_start_ix)?;
            let row = wrap_points(line).position(|wrap| wrap == local_index)? + 1;
            return Some(point(px(0.), line_origin_y + self.line_height * row as f32));
        }
        None
    }

    fn position_for_index(&self, index: usize) -> Option<Point<Pixels>> {
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

    fn bounds_for_range(
        &self,
        range: Range<usize>,
        origin: Point<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let quads = self.selection_quads(range, origin);
        let first = quads.first()?;
        let mut bounds = first.bounds;

        for quad in quads.into_iter().skip(1) {
            bounds = Bounds::from_corners(
                point(
                    bounds.left().min(quad.bounds.left()),
                    bounds.top().min(quad.bounds.top()),
                ),
                point(
                    bounds.right().max(quad.bounds.right()),
                    bounds.bottom().max(quad.bounds.bottom()),
                ),
            );
        }

        Some(bounds)
    }

    fn selection_quads(&self, range: Range<usize>, origin: Point<Pixels>) -> Vec<PaintQuad> {
        if range.is_empty() || self.is_placeholder {
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
                    rgba(selection()),
                ));
            }

            line_origin_y += line.size(self.line_height).height;
            line_start_ix = self.next_line_start(line_start_ix, line.len());
        }

        quads
    }
}

fn pixels_match(left: Pixels, right: Pixels) -> bool {
    (f32::from(left) - f32::from(right)).abs() <= 0.5
}

fn optional_pixels_match(left: Option<Pixels>, right: Option<Pixels>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => pixels_match(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn undo_edit_kind(replaced_len: usize, inserted_len: usize) -> UndoEditKind {
    if replaced_len == 0 && inserted_len > 0 {
        UndoEditKind::Insert
    } else if replaced_len > 0 && inserted_len == 0 {
        UndoEditKind::Delete
    } else {
        UndoEditKind::Replace
    }
}

fn visible_scroll_offset(
    layout: &TextLayoutSnapshot,
    current_scroll_y: Pixels,
    visible_height: Pixels,
    cursor: usize,
    cursor_on_lower_row: bool,
    follow_cursor: bool,
) -> Pixels {
    if layout.is_placeholder {
        return px(0.);
    }
    scrolling::resolve_scroll_offset(
        current_scroll_y,
        layout.total_height(),
        visible_height,
        layout
            .cursor_position(cursor, cursor_on_lower_row)
            .map(|point| point.y),
        layout.line_height,
        follow_cursor,
    )
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

fn previous_boundary_in_content(content: &str, offset: usize) -> usize {
    if content.is_empty() || offset == 0 {
        return 0;
    }

    content
        .grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

fn previous_word_start(content: &str, offset: usize) -> usize {
    if content.is_empty() || offset == 0 {
        return 0;
    }

    let target = offset.min(content.len());
    let segments: Vec<_> = content.split_word_bound_indices().collect();
    segments
        .into_iter()
        .rev()
        .find_map(|(start, segment)| {
            (start < target && !segment.chars().all(char::is_whitespace)).then_some(start)
        })
        .unwrap_or(0)
}

fn next_word_end(content: &str, offset: usize) -> usize {
    if content.is_empty() {
        return 0;
    }

    let target = offset.min(content.len());
    content
        .split_word_bound_indices()
        .find_map(|(start, segment)| {
            let end = start + segment.len();
            (end > target && !segment.chars().all(char::is_whitespace)).then_some(end)
        })
        .unwrap_or(content.len())
}

fn current_line_start(content: &str, offset: usize) -> usize {
    let target = offset.min(content.len());
    content[..target]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0)
}

fn current_line_end(content: &str, offset: usize) -> usize {
    let target = offset.min(content.len());
    content[target..]
        .find('\n')
        .map(|idx| target + idx)
        .unwrap_or(content.len())
}

fn delete_to_previous_word_start(content: &str, offset: usize) -> usize {
    if content.is_empty() || offset == 0 {
        return 0;
    }

    let target = offset.min(content.len());
    let line_start = current_line_start(content, target);
    if line_start == target {
        return previous_boundary_in_content(content, target);
    }

    let line_slice = &content[line_start..target];
    line_start + previous_word_start(line_slice, line_slice.len())
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

struct VisualLineSegment {
    global_start: usize,
    global_end: usize,
    top: Pixels,
    width: Pixels,
}

/// Offsets within `line` where it wraps onto a new row.
fn wrap_points(line: &WrappedLine) -> impl Iterator<Item = usize> + '_ {
    line.wrap_boundaries()
        .iter()
        .map(|boundary| line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix].index)
}

fn visual_segments(
    line: &WrappedLine,
    global_line_start: usize,
    line_height: Pixels,
) -> Vec<VisualLineSegment> {
    let mut segments = Vec::new();
    let mut segment_start = 0;
    let mut top = px(0.);
    let mut boundaries = wrap_points(line).collect::<Vec<_>>();
    boundaries.push(line.len());

    for boundary in boundaries {
        let width = line
            .position_for_index(boundary, line_height)
            .map(|position| position.x)
            .unwrap_or(px(0.));
        segments.push(VisualLineSegment {
            global_start: global_line_start + segment_start,
            global_end: global_line_start + boundary,
            top,
            width,
        });
        segment_start = boundary;
        top += line_height;
    }

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

#[cfg(test)]
mod tests {
    use super::{
        current_line_end, current_line_start, delete_to_previous_word_start, next_word_end,
        previous_word_start,
    };

    #[test]
    fn theme_color_changes_invalidate_cached_text_and_placeholder_runs() {
        use super::{PLACEHOLDER_OPACITY, TextLayoutSnapshot};
        use gpui_kit::{Hsla, px, rgb};
        for is_placeholder in [false, true] {
            let dark: Hsla = rgb(0xd9d9d9).into();
            let light: Hsla = rgb(0x262626).into();
            let dark = if is_placeholder {
                dark.opacity(PLACEHOLDER_OPACITY)
            } else {
                dark
            };
            let light = if is_placeholder {
                light.opacity(PLACEHOLDER_OPACITY)
            } else {
                light
            };
            let snapshot = TextLayoutSnapshot {
                is_placeholder,
                font_size: px(14.),
                line_height: px(22.),
                text_color: dark,
                ..Default::default()
            };
            assert!(snapshot.matches_shape_state(
                is_placeholder,
                None,
                None,
                px(14.),
                px(22.),
                dark
            ));
            assert!(!snapshot.matches_shape_state(
                is_placeholder,
                None,
                None,
                px(14.),
                px(22.),
                light
            ));
        }
    }

    #[test]
    fn previous_word_start_moves_to_prior_word() {
        assert_eq!(previous_word_start("hello world", "hello world".len()), 6);
    }

    #[test]
    fn previous_word_start_skips_intervening_whitespace() {
        assert_eq!(previous_word_start("hello   world", 8), 0);
    }

    #[test]
    fn previous_word_start_stops_at_punctuation_segment() {
        assert_eq!(previous_word_start("hello,", "hello,".len()), 5);
    }

    #[test]
    fn next_word_end_skips_whitespace_to_following_word() {
        assert_eq!(next_word_end("hello   world", 5), "hello   world".len());
    }

    #[test]
    fn current_line_boundaries_are_local_to_the_line() {
        assert_eq!(current_line_start("one\ntwo", 5), 4);
        assert_eq!(current_line_end("one\ntwo", 4), 7);
        assert_eq!(current_line_end("one\ntwo", 2), 3);
    }

    #[test]
    fn delete_to_previous_word_start_keeps_newline_separate() {
        assert_eq!(delete_to_previous_word_start("hello\n   world", 9), 6);
    }

    #[test]
    fn delete_to_previous_word_start_deletes_newline_from_line_start() {
        assert_eq!(delete_to_previous_word_start("hello\nworld", 6), 5);
    }
}
