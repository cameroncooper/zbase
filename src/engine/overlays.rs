//! Overlays: quick switcher, command palette, new chat, emoji picker, find in
//! chat, global search and the image lightbox, with `AppWindow`'s controller
//! logic for them (input observers, overlay actions, `dismiss_overlays_action`
//! and the overlay legs of `SelectPrevious` / `SelectNext` / `ConfirmPrimary`).

mod nav;
mod views;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use super::Engine;
use crate::{
    domain::{
        attachment::AttachmentSource,
        ids::{MessageId, UserId},
        route::Route,
        search::SearchFilter,
    },
    models::{
        QuickSwitcherLocalSearchOutput,
        composer_model::ComposerMode,
        compute_quick_switcher_local_results,
        emoji_picker_model::{EmojiPickerItem, recent_key_for_stock, selected_emoji_for_tone},
        navigation_model::RightPaneMode,
        notifications_model::ToastAction,
        quick_switcher_model::QuickSwitcherResultKind,
        timeline_model::TimelineRow,
    },
    state::{UiAction, event::BackendEvent},
    util::formatting::now_unix_ms,
};

pub use views::{EmojiPickerView, HitView, SwitcherRowView};

/// `QUICK_SWITCHER_REMOTE_MIN_QUERY_CHARS` in app_window.rs.
const QUICK_SWITCHER_REMOTE_MIN_QUERY_CHARS: usize = 2;
const QUICK_SWITCHER_CORPUS_REBUILD_COALESCE: Duration = Duration::from_millis(180);
const NEW_CHAT_REMOTE_DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    // Quick switcher (Cmd+K)
    ToggleQuickSwitcher,
    SetQuickSwitcherQuery {
        query: String,
    },
    OpenQuickSwitcherResult {
        index: usize,
    },
    /// Cmd+1..5 while the switcher shows recents (`OpenQuickSwitcherRecent2..5`, `ShowHome`).
    OpenQuickSwitcherRecent {
        recent_index: usize,
    },
    // Command palette (Cmd+Shift+P)
    ToggleCommandPalette,
    RunPaletteAction {
        action: String,
    },
    // New chat (Cmd+N)
    OpenNewChat,
    SetNewChatQuery {
        query: String,
    },
    NewChatAddParticipant {
        user_id: String,
    },
    NewChatRemoveParticipant {
        user_id: String,
    },
    NewChatCreate,
    /// Cmd+Return.
    StartNewChat,
    // Emoji picker
    ToggleEmojiPicker,
    OpenReactionPicker {
        message_id: String,
    },
    SetEmojiQuery {
        query: String,
    },
    SetEmojiGroup {
        group: Option<String>,
    },
    SetEmojiHovered {
        key: Option<String>,
    },
    ToggleEmojiToneExpanded,
    SetEmojiTone {
        tone: String,
    },
    PickEmoji {
        key: String,
        #[serde(default)]
        thread_focused: bool,
        #[serde(default)]
        composer_focused: bool,
    },
    // Find in chat (Cmd+F)
    ToggleFindInChat,
    SetFindQuery {
        query: String,
    },
    FindNext,
    FindPrevious,
    CloseFindInChat,
    // Global search (Cmd+J)
    SetSearchQuery {
        query: String,
    },
    /// `submit_search_input` (Cmd+J, Return in the field, palette).
    SubmitSearch,
    /// Return on the search page: open the highlighted result or search.
    ConfirmSearch,
    ToggleSearchFilter {
        filter: String,
    },
    OpenSearchResult {
        index: usize,
    },
    NavigateBack,
    // Lightbox and modals
    OpenImageLightbox {
        url: Option<String>,
        path: Option<String>,
        caption: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
    },
    OpenAttachmentModal {
        label: String,
    },
    // Window-level keys
    /// Up / Down (`SelectPrevious` / `SelectNext`) while an overlay or Search is up.
    Select {
        direction: isize,
    },
    /// Return (`ConfirmPrimary`) while the switcher or new chat is open.
    ConfirmPrimary,
    /// Escape (`dismiss_overlays_action`), after the web view gave the update
    /// panel, autocomplete and format menu their turn.
    DismissOverlaysAction,
    /// Backdrop clicks and Close buttons (`AppWindow::dismiss_overlays`).
    DismissOverlays,
}

/// The emoji the picker asks a composer to insert at its caret.
#[derive(Clone, Debug, Serialize)]
pub struct EmojiInsert {
    /// Grows with every pick so the same emoji twice is two inserts.
    pub seq: u64,
    /// "composer" or "thread".
    pub target: &'static str,
    pub text: String,
}

/// State with no home in `AppModels`.
#[derive(Default, Serialize)]
pub struct OverlaysState {
    pub quick_switcher_query_seq: u64,
    /// "Indexing older messages…" while the backend crawls history.
    pub quick_switcher_indexing_status: Option<String>,
    /// Display data for each `quick_switcher.results` entry, by index.
    pub quick_switcher_rows: Vec<SwitcherRowView>,
    /// Display data for each `search.results` entry, by index.
    pub search_rows: Vec<HitView>,
    pub emoji_picker: Option<EmojiPickerView>,
    pub emoji_insert: Option<EmojiInsert>,
    #[serde(skip)]
    private: Private,
}

#[derive(Default)]
struct Private {
    last_local_query: String,
    last_local_matches: Arc<Vec<usize>>,
    last_local_corpus_revision: u64,
    pending_local: Option<(u64, String, Instant)>,
    pending_remote: Option<(u64, String, Instant)>,
    pending_corpus_rebuild: Option<Instant>,
    pending_new_chat_remote: Option<(crate::state::ids::QueryId, Instant)>,
    indexing_active: bool,
    indexing_total: Option<u64>,
    indexing_completed: u64,
    indexing_messages: u64,
    emoji_catalog: views::EmojiCatalog,
}

impl std::fmt::Debug for OverlaysState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OverlaysState")
            .field("quick_switcher_query_seq", &self.quick_switcher_query_seq)
            .finish_non_exhaustive()
    }
}

pub fn handle(engine: &mut Engine, intent: Intent) {
    match intent {
        Intent::ToggleQuickSwitcher => toggle_quick_switcher(engine),
        Intent::SetQuickSwitcherQuery { query } => set_quick_switcher_query(engine, query),
        Intent::OpenQuickSwitcherResult { index } => open_quick_switcher_result_at(engine, index),
        Intent::OpenQuickSwitcherRecent { recent_index } => {
            open_quick_switcher_recent_result(engine, recent_index);
        }
        Intent::ToggleCommandPalette => {
            close_new_chat_if_open(engine);
            engine.models.toggle_command_palette();
        }
        Intent::RunPaletteAction { action } => run_palette_action(engine, &action),
        Intent::OpenNewChat => open_new_chat(engine),
        Intent::SetNewChatQuery { query } => set_new_chat_query(engine, query),
        Intent::NewChatAddParticipant { user_id } => {
            if let Some(user) = new_chat_user(engine, &user_id) {
                engine.dispatch(UiAction::NewChatAddParticipant { user });
            }
        }
        Intent::NewChatRemoveParticipant { user_id } => {
            engine.dispatch(UiAction::NewChatRemoveParticipant {
                user_id: UserId::new(user_id),
            });
        }
        Intent::NewChatCreate => engine.dispatch(UiAction::NewChatCreate),
        Intent::StartNewChat => start_new_chat(engine),
        Intent::ToggleEmojiPicker => engine.models.toggle_emoji_picker(),
        Intent::OpenReactionPicker { message_id } => engine
            .models
            .open_reaction_picker_for_message(&MessageId::new(message_id)),
        Intent::SetEmojiQuery { query } => {
            if engine.models.overlay.emoji_picker_open && engine.models.emoji_picker.query != query
            {
                engine.models.set_emoji_picker_query(query);
            }
        }
        Intent::SetEmojiGroup { group } => {
            let group = group.as_deref().and_then(views::group_from_name);
            engine.models.set_emoji_picker_active_group(group);
        }
        Intent::SetEmojiHovered { key } => {
            let current = engine
                .models
                .emoji_picker
                .hovered
                .as_ref()
                .map(EmojiPickerItem::key);
            if current != key {
                let item = key
                    .as_deref()
                    .and_then(|key| engine.overlays.private.emoji_catalog.item(key));
                engine.models.set_emoji_picker_hovered(item);
            }
        }
        Intent::ToggleEmojiToneExpanded => {
            let expanded = !engine.models.emoji_picker.skin_tone_expanded;
            engine.models.set_emoji_picker_skin_tone_expanded(expanded);
        }
        Intent::SetEmojiTone { tone } => {
            if let Some(tone) = views::tone_from_name(&tone) {
                engine.models.set_emoji_picker_skin_tone(Some(tone));
                engine.models.set_emoji_picker_skin_tone_expanded(false);
                engine.persist_settings();
            }
        }
        Intent::PickEmoji {
            key,
            thread_focused,
            composer_focused,
        } => {
            if let Some(item) = engine.overlays.private.emoji_catalog.item(&key) {
                emoji_picker_pick_item(engine, item, thread_focused, composer_focused);
            }
        }
        Intent::ToggleFindInChat => toggle_find_in_chat(engine),
        Intent::SetFindQuery { query } => set_find_query(engine, query),
        Intent::FindNext => {
            if let Some(result) = engine.models.find_in_chat.select_next() {
                jump_to_find_match(engine, result.message.id);
            }
        }
        Intent::FindPrevious => {
            if let Some(result) = engine.models.find_in_chat.select_previous() {
                jump_to_find_match(engine, result.message.id);
            }
        }
        Intent::CloseFindInChat => close_find_in_chat(engine),
        Intent::SetSearchQuery { query } => {
            engine.models.sync_live_search_query(query.clone());
            engine.dispatch(UiAction::SetSearchQuery(query));
        }
        Intent::SubmitSearch => submit_search_input(engine),
        Intent::ConfirmSearch => {
            if matches!(engine.models.navigation.current, Route::Search { .. })
                && let Some(index) = engine.models.search.highlighted_index
            {
                open_search_result_at(engine, index);
            } else {
                submit_search_input(engine);
            }
        }
        Intent::ToggleSearchFilter { filter } => {
            if let Some(filter) = search_filter(&filter) {
                engine.dispatch(UiAction::ToggleSearchFilter(filter));
                // Filters narrow the current search, so run it again right away.
                if engine.models.search.submitted_query.is_some() {
                    engine.dispatch(UiAction::SubmitSearch);
                }
            }
        }
        Intent::OpenSearchResult { index } => open_search_result_at(engine, index),
        Intent::NavigateBack => nav::navigate_back(engine),
        Intent::OpenImageLightbox {
            url,
            path,
            caption,
            width,
            height,
        } => {
            let source = match (path, url) {
                (Some(path), _) => AttachmentSource::LocalPath(path),
                (None, Some(url)) => AttachmentSource::Url(url),
                (None, None) => return,
            };
            engine
                .models
                .open_image_lightbox(source, caption, width, height);
        }
        Intent::OpenAttachmentModal { label } => engine.models.open_attachment_modal(&label),
        Intent::Select { direction } => select(engine, direction.signum()),
        Intent::ConfirmPrimary => confirm_primary(engine),
        Intent::DismissOverlaysAction => dismiss_overlays_action(engine),
        Intent::DismissOverlays => dismiss_overlays(engine),
    }
    engine.mark_dirty();
}

/// Debounced work, and the display data the web view renders from.
pub fn tick(engine: &mut Engine) {
    let now = Instant::now();
    run_pending_local_search(engine, now);
    run_pending_remote_search(engine, now);
    run_pending_corpus_rebuild(engine, now);
    if let Some((query_id, due)) = engine.overlays.private.pending_new_chat_remote.clone()
        && now >= due
    {
        engine.overlays.private.pending_new_chat_remote = None;
        engine.dispatch(UiAction::NewChatSearchRemote { query_id });
    }
    refresh_views(engine);
}

/// Replays the overlay side of `AppWindow::sync_models_from_store`.
pub fn after_sync(engine: &mut Engine, outcome: &crate::models::store_sync::SyncOutcome) {
    if outcome.quick_switcher_local_results_stale {
        clear_local_search_cache(engine);
    }
}

/// Sees each backend event before the reducer, like `drain_backend_events`.
pub fn observe_backend_event(engine: &mut Engine, event: &BackendEvent) {
    match event {
        BackendEvent::UserProfileUpserted {
            user_id,
            display_name,
            ..
        } => {
            if engine
                .models
                .upsert_quick_switcher_profile_name(user_id, display_name)
            {
                engine.overlays.private.pending_corpus_rebuild =
                    Some(Instant::now() + QUICK_SWITCHER_CORPUS_REBUILD_COALESCE);
            }
        }
        BackendEvent::KeybaseNotifyStub {
            method,
            payload_preview,
        } => {
            if !engine.models.overlay.quick_switcher_open
                && !engine.overlays.private.indexing_active
            {
                return;
            }
            if update_indexing(engine, method, payload_preview.as_deref())
                && engine.models.overlay.quick_switcher_open
            {
                engine.mark_dirty();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Quick switcher

/// `AppWindow::toggle_quick_switcher`.
fn toggle_quick_switcher(engine: &mut Engine) {
    close_new_chat_if_open(engine);
    engine.models.toggle_quick_switcher();
    let overlays = &mut engine.overlays;
    overlays.quick_switcher_query_seq = overlays.quick_switcher_query_seq.wrapping_add(1);
    overlays.private.pending_local = None;
    overlays.private.pending_remote = None;
}

fn clear_local_search_cache(engine: &mut Engine) {
    let private = &mut engine.overlays.private;
    private.last_local_query.clear();
    private.last_local_matches = Arc::new(Vec::new());
    private.last_local_corpus_revision = 0;
}

/// The `quick_switcher_input` observer.
fn set_quick_switcher_query(engine: &mut Engine, query: String) {
    if !engine.models.overlay.quick_switcher_open || engine.models.quick_switcher.query == query {
        return;
    }
    if engine
        .models
        .flush_quick_switcher_local_search_corpus_if_dirty()
    {
        clear_local_search_cache(engine);
    }
    let query_trimmed = query.trim().to_string();
    let previous_trimmed = engine.overlays.private.last_local_query.trim().to_string();
    let prefix_extend = !previous_trimmed.is_empty()
        && query_trimmed.starts_with(&previous_trimmed)
        && query_trimmed.chars().count() > previous_trimmed.chars().count();
    let conversation_count = engine
        .models
        .quick_switcher_local_search_snapshot()
        .entries
        .len();
    let local_debounce = quick_switcher_local_debounce(
        conversation_count,
        prefix_extend,
        query_trimmed.chars().count() <= 2,
    );
    let remote_debounce = quick_switcher_remote_debounce(conversation_count);
    engine.overlays.quick_switcher_query_seq =
        engine.overlays.quick_switcher_query_seq.wrapping_add(1);
    let seq = engine.overlays.quick_switcher_query_seq;
    engine.models.quick_switcher.query = query.clone();

    if query_trimmed.is_empty() {
        clear_local_search_cache(engine);
        engine.overlays.private.pending_local = None;
        engine.overlays.private.pending_remote = None;
        engine.models.quick_switcher.loading_messages = false;
        engine.models.update_quick_switcher_query(query);
        return;
    }
    let run_remote_search = query_trimmed.chars().count() >= QUICK_SWITCHER_REMOTE_MIN_QUERY_CHARS;
    engine.models.quick_switcher.loading_messages = run_remote_search;
    let now = Instant::now();
    engine.overlays.private.pending_local = Some((seq, query.clone(), now + local_debounce));
    engine.overlays.private.pending_remote =
        run_remote_search.then(|| (seq, query, now + remote_debounce));
    if local_debounce.is_zero() {
        run_pending_local_search(engine, now);
    }
}

fn run_pending_local_search(engine: &mut Engine, now: Instant) {
    let Some((seq, query, due)) = engine.overlays.private.pending_local.clone() else {
        return;
    };
    if now < due {
        return;
    }
    engine.overlays.private.pending_local = None;
    if engine.overlays.quick_switcher_query_seq != seq || !engine.models.overlay.quick_switcher_open
    {
        return;
    }
    let corpus = engine.models.quick_switcher_local_search_snapshot();
    let private = &engine.overlays.private;
    let output: QuickSwitcherLocalSearchOutput = compute_quick_switcher_local_results(
        &query,
        &corpus,
        Some(&private.last_local_query),
        private.last_local_matches.as_slice(),
        Some(private.last_local_corpus_revision),
        Some(&engine.models.settings.quick_switcher_affinity),
        now_unix_ms(),
    );
    let run_remote_search = query.trim().chars().count() >= QUICK_SWITCHER_REMOTE_MIN_QUERY_CHARS;
    let switcher = &mut engine.models.quick_switcher;
    switcher.query = query.clone();
    switcher.results = output.results;
    switcher.selected_index = switcher
        .selected_index
        .min(switcher.results.len().saturating_sub(1));
    switcher.loading_messages = run_remote_search;
    let private = &mut engine.overlays.private;
    private.last_local_query = query;
    private.last_local_matches = Arc::new(output.matched_entry_indices);
    private.last_local_corpus_revision = corpus.revision;
    engine.mark_dirty();
}

fn run_pending_remote_search(engine: &mut Engine, now: Instant) {
    let Some((seq, query, due)) = engine.overlays.private.pending_remote.clone() else {
        return;
    };
    if now < due {
        return;
    }
    engine.overlays.private.pending_remote = None;
    if engine.overlays.quick_switcher_query_seq != seq {
        return;
    }
    if !engine.models.overlay.quick_switcher_open {
        engine.models.quick_switcher.loading_messages = false;
        return;
    }
    engine.dispatch(UiAction::QuickSwitcherSearch { seq, query });
}

/// `schedule_quick_switcher_corpus_rebuild`, coalesced.
fn run_pending_corpus_rebuild(engine: &mut Engine, now: Instant) {
    let Some(due) = engine.overlays.private.pending_corpus_rebuild else {
        return;
    };
    if now < due {
        return;
    }
    engine.overlays.private.pending_corpus_rebuild = None;
    if !engine
        .models
        .flush_quick_switcher_local_search_corpus_if_dirty()
    {
        return;
    }
    clear_local_search_cache(engine);
    if engine.models.overlay.quick_switcher_open
        && engine.models.quick_switcher.query.trim().is_empty()
    {
        engine.models.update_quick_switcher_query(String::new());
    }
    engine.mark_dirty();
}

/// `AppWindow::open_quick_switcher_result_at`.
fn open_quick_switcher_result_at(engine: &mut Engine, index: usize) {
    let Some(result) = engine.models.quick_switcher.results.get(index).cloned() else {
        return;
    };
    engine.models.quick_switcher.selected_index = index;
    open_quick_switcher_result(engine, result);
}

fn open_quick_switcher_result(
    engine: &mut Engine,
    result: crate::models::quick_switcher_model::QuickSwitcherResult,
) {
    if result.kind != QuickSwitcherResultKind::Message {
        engine
            .models
            .record_quick_switcher_selection_affinity(&result.conversation_id);
        engine.persist_settings();
    }
    if let Some(message_id) = result.message_id {
        nav::navigate_to_message(engine, result.route, message_id);
    } else {
        // `quick_switch_to`.
        engine.models.dismiss_overlays();
        nav::navigate_to(engine, result.route);
    }
}

/// `AppWindow::open_quick_switcher_recent_result`.
fn open_quick_switcher_recent_result(engine: &mut Engine, recent_index: usize) -> bool {
    if !engine.models.overlay.quick_switcher_open
        || !engine.models.quick_switcher.query.trim().is_empty()
    {
        return false;
    }
    let Some(index) = engine
        .models
        .quick_switcher
        .results
        .iter()
        .enumerate()
        .filter(|(_, result)| result.kind != QuickSwitcherResultKind::UnreadChannel)
        .nth(recent_index)
        .map(|(index, _)| index)
    else {
        return false;
    };
    open_quick_switcher_result_at(engine, index);
    true
}

/// `update_quick_switcher_indexing_from_internal`.
fn update_indexing(engine: &mut Engine, method: &str, payload: Option<&str>) -> bool {
    let private = &mut engine.overlays.private;
    let before = (
        private.indexing_active,
        private.indexing_total,
        private.indexing_completed,
        private.indexing_messages,
    );
    match method {
        "zbase.internal.crawl.start" => {
            private.indexing_active = true;
            private.indexing_total = extract_u64(payload, "conversation_count");
            private.indexing_completed = 0;
            private.indexing_messages = 0;
        }
        "zbase.internal.crawl.conversation_progress" => {
            private.indexing_active = true;
            if let Some(indexed) = extract_u64(payload, "messages_indexed") {
                private.indexing_messages = indexed;
            }
        }
        "zbase.internal.crawl.conversation_complete" => {
            private.indexing_active = true;
            private.indexing_completed = private.indexing_completed.saturating_add(1);
            if let Some(indexed) = extract_u64(payload, "messages_indexed") {
                private.indexing_messages = indexed;
            }
        }
        "zbase.internal.crawl.finished" => {
            if let Some(completed) = extract_u64(payload, "completed_conversations") {
                private.indexing_completed = completed;
            }
            if let Some(total) = extract_u64(payload, "conversation_count") {
                private.indexing_total = Some(total);
            }
            if let Some(indexed) = extract_u64(payload, "indexed_messages") {
                private.indexing_messages = indexed;
            }
            private.indexing_active = false;
        }
        _ => {}
    }
    let after = (
        private.indexing_active,
        private.indexing_total,
        private.indexing_completed,
        private.indexing_messages,
    );
    engine.overlays.quick_switcher_indexing_status = indexing_status_text(private);
    after != before
}

/// `quick_switcher_indexing_status_text`.
fn indexing_status_text(private: &Private) -> Option<String> {
    if !private.indexing_active {
        return None;
    }
    let progress = private
        .indexing_total
        .map(|total| {
            format!(
                "{} / {} conversations",
                private.indexing_completed.min(total),
                total
            )
        })
        .unwrap_or_else(|| format!("{} conversations", private.indexing_completed));
    Some(if private.indexing_messages > 0 {
        format!(
            "Indexing older messages: {} indexed ({progress})",
            private.indexing_messages
        )
    } else {
        format!("Indexing older messages in background ({progress})")
    })
}

fn extract_u64(payload: Option<&str>, key: &str) -> Option<u64> {
    let payload = payload?;
    let after_key = &payload[payload.find(key)? + key.len()..];
    let digits_start = after_key.find(|ch: char| ch.is_ascii_digit())?;
    let digits = after_key[digits_start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}

fn quick_switcher_local_debounce(
    conversation_count: usize,
    prefix_extend: bool,
    short_query: bool,
) -> Duration {
    let millis = match (prefix_extend, short_query, conversation_count) {
        (true, _, 10_000..) => 16,
        (true, _, 5_000..) => 12,
        (true, _, 2_000..) => 8,
        (true, _, _) => 0,
        (false, true, 10_000..) => 28,
        (false, true, 5_000..) => 22,
        (false, true, 2_000..) => 18,
        (false, true, _) => 12,
        (false, false, 10_000..) => 70,
        (false, false, 5_000..) => 56,
        (false, false, 2_000..) => 42,
        (false, false, _) => 28,
    };
    Duration::from_millis(millis)
}

fn quick_switcher_remote_debounce(conversation_count: usize) -> Duration {
    Duration::from_millis(match conversation_count {
        10_000.. => 200,
        5_000.. => 180,
        2_000.. => 150,
        _ => 120,
    })
}

// ---------------------------------------------------------------------------
// Command palette

/// `AppWindow::run_palette_action`.
fn run_palette_action(engine: &mut Engine, action: &str) {
    engine.models.dismiss_overlays();
    match action {
        "new-chat" => open_new_chat(engine),
        "search" => submit_search_input(engine),
        "preferences" => nav::navigate_to(engine, Route::Preferences),
        "activity" => {
            let route = Route::Activity {
                workspace_id: engine.models.app.active_workspace_id.clone(),
            };
            nav::navigate_to(engine, route);
        }
        "thread" => toggle_simple_pane(engine, RightPaneMode::Thread),
        "call" => super::shell::start_or_open_call(engine),
        "mark-all-read" => super::shell::mark_all_conversations_read(engine),
        _ => {}
    }
}

/// `AppWindow::toggle_simple_pane`.
fn toggle_simple_pane(engine: &mut Engine, pane: RightPaneMode) {
    if pane != RightPaneMode::Hidden && !engine.models.settings.show_right_pane {
        engine.models.push_toast(
            "The side panel is turned off in Settings",
            Some(ToastAction::OpenPreferences),
        );
        return;
    }
    if engine.models.navigation.right_pane == pane {
        nav::close_right_pane_state(engine);
    } else {
        engine.models.set_right_pane(pane);
    }
}

// ---------------------------------------------------------------------------
// New chat

fn close_new_chat_if_open(engine: &mut Engine) {
    if engine.models.overlay.new_chat_open || engine.models.new_chat.open {
        engine.dispatch(UiAction::CloseNewChat);
    }
}

/// `AppWindow::open_new_chat`.
fn open_new_chat(engine: &mut Engine) {
    engine.models.dismiss_overlays();
    engine.dispatch(UiAction::OpenNewChat);
    engine.models.overlay.new_chat_open = true;
}

/// The `new_chat_input` observer.
fn set_new_chat_query(engine: &mut Engine, query: String) {
    if !engine.models.overlay.new_chat_open || engine.models.new_chat.search_query == query {
        return;
    }
    engine.dispatch(UiAction::NewChatSearchUsers { query });
    engine.overlays.private.pending_new_chat_remote = engine
        .store
        .snapshot()
        .new_chat
        .active_query_id
        .clone()
        .map(|query_id| (query_id, Instant::now() + NEW_CHAT_REMOTE_DEBOUNCE));
}

fn new_chat_user(engine: &Engine, user_id: &str) -> Option<crate::domain::user::UserSummary> {
    let new_chat = &engine.models.new_chat;
    new_chat
        .search_results
        .iter()
        .chain(new_chat.selected_participants.iter())
        .find(|user| user.id.0 == user_id)
        .cloned()
}

/// Return with new chat open: add the highlighted person, or start the chat.
fn new_chat_confirm(engine: &mut Engine) {
    if let Some(candidate) = engine.models.new_chat.highlighted_candidate().cloned() {
        let already_added = engine
            .models
            .new_chat
            .selected_participants
            .iter()
            .any(|user| user.id == candidate.id);
        if !already_added {
            engine.dispatch(UiAction::NewChatAddParticipant { user: candidate });
        }
        // The field clears, like `new_chat_input.set_text("")`.
        set_new_chat_query(engine, String::new());
    } else {
        engine.dispatch(UiAction::NewChatCreate);
    }
}

/// `start_new_chat_action` (Cmd+Return).
fn start_new_chat(engine: &mut Engine) {
    if !engine.models.overlay.new_chat_open {
        return;
    }
    if engine.models.new_chat.selected_participants.is_empty()
        && let Some(candidate) = engine.models.new_chat.highlighted_candidate().cloned()
    {
        engine.dispatch(UiAction::NewChatAddParticipant { user: candidate });
    }
    engine.dispatch(UiAction::NewChatCreate);
}

// ---------------------------------------------------------------------------
// Emoji picker

/// `AppWindow::emoji_picker_insert_text`: what lands in the text and the recents key.
fn emoji_picker_insert_text(engine: &Engine, item: &EmojiPickerItem) -> (String, String) {
    match item {
        EmojiPickerItem::Stock(emoji) => {
            let selected =
                selected_emoji_for_tone(emoji, engine.models.emoji_picker.selected_skin_tone);
            (
                format!("{} ", selected.as_str()),
                recent_key_for_stock(emoji),
            )
        }
        EmojiPickerItem::Custom { alias, .. } => {
            (format!(":{alias}: "), alias.to_ascii_lowercase())
        }
    }
}

/// `AppWindow::emoji_picker_pick_item`: react to the target message, or insert
/// into the thread or conversation composer.
fn emoji_picker_pick_item(
    engine: &mut Engine,
    item: EmojiPickerItem,
    thread_focused: bool,
    composer_focused: bool,
) {
    let (text, recent_alias) = emoji_picker_insert_text(engine, &item);
    engine.models.add_recent_emoji_alias(recent_alias);
    engine.persist_settings();
    if let Some(message_id) = engine.models.overlay.reaction_target_message_id.clone() {
        engine.models.overlay.emoji_picker_open = false;
        engine.models.overlay.reaction_target_message_id = None;
        quick_react(engine, message_id, text.trim().to_string());
        return;
    }
    let thread = thread_focused
        || (!composer_focused
            && engine.models.navigation.right_pane == RightPaneMode::Thread
            && engine.models.thread_pane.open);
    // `composer_insert_emoji` / `thread_insert_emoji`: insert at the caret.
    let target = if thread {
        super::composer::Target::Thread
    } else {
        super::composer::Target::Conversation
    };
    super::composer::insert_text(engine, target, &text);
}

/// `AppWindow::quick_react`.
fn quick_react(engine: &mut Engine, message_id: MessageId, emoji: String) {
    let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone() else {
        return;
    };
    nav::ensure_message_binding_for_active_conversation(engine, &message_id);
    engine.dispatch(UiAction::ReactToMessage {
        conversation_id,
        message_id,
        emoji: emoji.clone(),
    });
    engine.models.add_recent_emoji_alias(emoji);
    engine.persist_settings();
}

// ---------------------------------------------------------------------------
// Find in chat

/// `AppWindow::toggle_find_in_chat`.
fn toggle_find_in_chat(engine: &mut Engine) {
    if engine.models.find_in_chat.open {
        close_find_in_chat(engine);
        return;
    }
    if !matches!(
        engine.models.navigation.current,
        Route::Channel { .. } | Route::DirectMessage { .. }
    ) {
        return;
    }
    let conversation_id = engine.models.conversation.summary.id.clone();
    let anchor = find_in_chat_anchor_timestamp(engine);
    engine
        .models
        .find_in_chat
        .open_for_conversation(conversation_id, anchor);
}

fn find_in_chat_anchor_timestamp(engine: &Engine) -> Option<i64> {
    let rows = &engine.models.timeline.rows;
    let timestamp = |message_id: &MessageId| {
        rows.iter().find_map(|row| match row {
            TimelineRow::Message(row) if &row.message.id == message_id => row.message.timestamp_ms,
            _ => None,
        })
    };
    if let Some(message_id) = engine.models.timeline.highlighted_message_id.as_ref()
        && let Some(timestamp_ms) = timestamp(message_id)
    {
        return Some(timestamp_ms);
    }
    if let Some(message_id) = engine.view.first_visible_message_id.as_deref()
        && let Some(timestamp_ms) = timestamp(&MessageId::new(message_id))
    {
        return Some(timestamp_ms);
    }
    rows.iter().rev().find_map(|row| match row {
        TimelineRow::Message(row) => row.message.timestamp_ms,
        _ => None,
    })
}

/// `AppWindow::close_find_in_chat`. The timeline returns to the bottom and the
/// composer takes focus in the web view.
fn close_find_in_chat(engine: &mut Engine) {
    if !engine.models.find_in_chat.open {
        return;
    }
    engine.models.find_in_chat.close();
    engine.models.timeline.pending_scroll_target = None;
    engine.models.timeline.highlighted_message_id = None;
    engine.timeline.jump_to_message_active = false;
    // A find match may have replaced the loaded timeline with an older window.
    engine.dispatch(UiAction::ReturnToLatestMessages);
}

/// The `find_in_chat_input` observer.
fn set_find_query(engine: &mut Engine, query: String) {
    let find = &mut engine.models.find_in_chat;
    if !find.open || find.query == query {
        return;
    }
    find.query_seq = find.query_seq.wrapping_add(1);
    let seq = find.query_seq;
    find.set_query(query.clone());
    if query.trim().is_empty() {
        return;
    }
    let Some(conversation_id) = find.conversation_id.clone() else {
        find.loading = false;
        return;
    };
    engine.dispatch(UiAction::FindInChatSearch {
        seq,
        conversation_id,
        query,
    });
}

/// `AppWindow::jump_to_find_match`. A match already in the loaded rows is
/// scrolled to by the timeline from `pending_scroll_target`; others are loaded.
fn jump_to_find_match(engine: &mut Engine, message_id: MessageId) {
    engine.models.timeline.pending_scroll_target = Some(message_id.clone());
    engine.models.timeline.highlighted_message_id = Some(message_id.clone());
    let loaded = engine
        .models
        .timeline
        .rows
        .iter()
        .any(|row| matches!(row, TimelineRow::Message(row) if row.message.id == message_id));
    if !loaded
        && let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone()
    {
        engine.dispatch(UiAction::JumpToMessage {
            conversation_id,
            message_id,
        });
    }
}

// ---------------------------------------------------------------------------
// Search

/// `AppWindow::submit_search_input`.
fn submit_search_input(engine: &mut Engine) {
    engine.models.dismiss_overlays();
    engine.dispatch(UiAction::SubmitSearch);
    let route = Route::Search {
        workspace_id: engine.models.app.active_workspace_id.clone(),
        query: engine.models.search.query.clone(),
    };
    nav::navigate_to(engine, route);
}

fn open_search_result_at(engine: &mut Engine, index: usize) {
    let Some(result) = engine.models.search.results.get(index).cloned() else {
        return;
    };
    nav::navigate_to_message(engine, result.route, result.message.id);
}

fn search_filter(name: &str) -> Option<SearchFilter> {
    match name {
        "has_file" => Some(SearchFilter::HasFile),
        "has_link" => Some(SearchFilter::HasLink),
        "mentions_me" => Some(SearchFilter::MentionsMe),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Window-level keys

/// The overlay legs of `select_previous_action` / `select_next_action`.
fn select(engine: &mut Engine, direction: isize) {
    if engine.models.overlay.quick_switcher_open {
        engine.models.move_quick_switcher_selection(direction);
    } else if engine.models.overlay.new_chat_open {
        engine.models.new_chat.move_highlight(direction);
    } else if matches!(engine.models.navigation.current, Route::Search { .. }) {
        engine.models.move_search_highlight(direction);
    }
}

/// The overlay legs of `confirm_primary_action`.
fn confirm_primary(engine: &mut Engine) {
    if engine.models.overlay.quick_switcher_open {
        if let Some(selected) = engine.models.quick_switcher_selected_result().cloned() {
            open_quick_switcher_result(engine, selected);
        }
    } else if engine.models.overlay.new_chat_open {
        new_chat_confirm(engine);
    } else if matches!(engine.models.navigation.current, Route::Search { .. })
        && let Some(index) = engine.models.search.highlighted_index
    {
        open_search_result_at(engine, index);
    }
}

/// `AppWindow::dismiss_overlays` (also closes the update panel, inspector and splash).
fn dismiss_overlays(engine: &mut Engine) {
    super::shell::dismiss_overlays(engine);
}

/// `dismiss_overlays_action` from the overlay check on: dismiss overlays, else
/// cancel an edit, else close the thread, profile or details pane.
fn dismiss_overlays_action(engine: &mut Engine) {
    let overlay = &engine.models.overlay;
    let had_overlay = overlay.new_chat_open
        || overlay.quick_switcher_open
        || overlay.command_palette_open
        || overlay.emoji_picker_open
        || overlay.fullscreen_image.is_some()
        || overlay.file_upload_lightbox.is_some()
        || overlay.profile_card_user_id.is_some()
        || engine.shell.inspector.open
        || engine.shell.splash_open;
    let is_editing = matches!(engine.models.composer.mode, ComposerMode::Edit { .. });
    let should_close_right_pane = !had_overlay
        && !is_editing
        && matches!(
            engine.models.navigation.right_pane,
            RightPaneMode::Thread | RightPaneMode::Profile(_) | RightPaneMode::Details
        );
    if !had_overlay && is_editing {
        engine.models.cancel_edit();
        return;
    }
    dismiss_overlays(engine);
    if should_close_right_pane {
        nav::close_right_pane_state(engine);
    }
}

// ---------------------------------------------------------------------------
// Display data

fn refresh_views(engine: &mut Engine) {
    let snapshot = engine.store.snapshot();
    let switcher_rows = if engine.models.overlay.quick_switcher_open {
        views::switcher_rows(
            &engine.models,
            snapshot,
            &engine.store_sync.sidebar_conversation_avatars,
        )
    } else {
        Vec::new()
    };
    let showing_search = matches!(engine.models.navigation.current, Route::Search { .. })
        || engine.models.navigation.right_pane == RightPaneMode::Search;
    let search_rows = if showing_search {
        views::search_rows(&engine.models, snapshot)
    } else {
        Vec::new()
    };
    let emoji_picker = if engine.models.overlay.emoji_picker_open {
        Some(
            engine
                .overlays
                .private
                .emoji_catalog
                .view(&engine.models.emoji_picker, snapshot),
        )
    } else {
        engine.overlays.private.emoji_catalog.clear();
        None
    };
    let overlays = &mut engine.overlays;
    let changed = overlays.quick_switcher_rows != switcher_rows
        || overlays.search_rows != search_rows
        || overlays.emoji_picker != emoji_picker;
    overlays.quick_switcher_rows = switcher_rows;
    overlays.search_rows = search_rows;
    overlays.emoji_picker = emoji_picker;
    if changed {
        engine.mark_dirty();
    }
}
