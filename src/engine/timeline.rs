//! Conversation timeline: messages, attachments, reactions, link previews,
//! scrolling and jumps.
//!
//! Ports the timeline controller half of `AppWindow` (src/views/app_window.rs:
//! scroll anchoring and bottom pinning in `sync_models_from_store`,
//! `sync_scroll_indicators`, `request_timeline_older_page_if_needed`,
//! `jump_to_pinned_message`, `quick_react`, `copy_message_link`, the download
//! controls) and precomputes what `TimelineList` derives before drawing
//! (`present`), so the web view only lays it out.

mod code_highlight;
mod downloads;
mod inline_markdown;
mod present;
mod text;

use std::{
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use tauri_plugin_opener::OpenerExt;

use super::{Engine, message_id_is_after};
use crate::{
    domain::{
        backend::ProviderMessageRef, conversation::ConversationKind, ids::MessageId,
        message::MessageRecord, pins::PinnedTarget,
    },
    models::{
        app_model::Connectivity,
        navigation_model::RightPaneMode,
        notifications_model::ToastAction,
        store_sync::{SyncOutcome, timeline_row_index_for_message},
        timeline_model::TimelineRow,
    },
    state::{UiAction, bindings::MessageBinding, state::TimelineKey},
};
use code_highlight::CodeHighlightCache;
use downloads::{DownloadState, Downloads};
use present::{Inputs, MessageView, compact_pinned_preview_text, message_view};

/// `views::avatar::presence_label`.
fn presence_label(availability: &crate::domain::presence::Availability) -> Option<&'static str> {
    use crate::domain::presence::Availability;
    match availability {
        Availability::Active => Some("Active"),
        Availability::Away => Some("Away"),
        Availability::DoNotDisturb => Some("Do not disturb"),
        Availability::Offline => Some("Offline"),
        Availability::Unknown => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    /// A reaction tile, a quick-react button or the recent emoji: `quick_react`.
    React {
        message_id: String,
        emoji: String,
    },
    /// Hover toolbar "Delete": `delete_message`.
    DeleteMessage {
        message_id: String,
    },
    CopyMessageLink {
        message_id: String,
    },
    CopyMessageText {
        message_id: String,
    },
    /// "Not sent · Delete": `discard_failed_message`.
    DiscardFailedSend {
        message_id: String,
    },
    /// Links, mentions (`zbase-mention:`), channels (`zbase-channel:`) and
    /// `keybase://` links: `open_url_or_deep_link`.
    OpenUrl {
        url: String,
    },
    /// A `#channel` in a system event: `navigate_to_channel_link`.
    OpenChannelLink {
        channel_name: String,
        team_name: Option<String>,
        conv_id: Option<String>,
    },
    /// Click on an inline image: `open_image_lightbox`.
    OpenImage {
        message_id: String,
        index: usize,
        #[serde(default)]
        thread: bool,
    },
    /// Click on a video thumbnail: `open_video_in_native_player`.
    OpenVideo {
        path: String,
    },
    /// The download control and file rows: `activate_attachment_download`.
    Download {
        source: String,
        name: String,
    },
    JumpToPinned,
    DismissPinnedBanner,
    /// The "Jump to latest" / "N new messages" pill.
    JumpToLatest,
    /// The user scrolled the timeline (wheel, keys, drag).
    Scrolled {
        near_top: bool,
    },
    /// The web view finished a scroll instruction.
    ScrollApplied {
        seq: u64,
    },
}

/// Where the web view should put the timeline next; `seq` grows with each.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ScrollCommand {
    pub seq: u64,
    pub target: ScrollTarget,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScrollTarget {
    #[default]
    None,
    /// Pin to the newest message.
    Bottom,
    /// A jump (search, pins, deep links, find): the message's top at the top
    /// of the viewport, `offset` pixels into it.
    Message { message_id: String, offset: f32 },
    /// Keep this row at the top after rows were rebuilt or older history
    /// arrived; the web view keeps its pixel offset when it can.
    Anchor {
        message_id: Option<String>,
        row_index: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct HeaderView {
    pub title: String,
    pub subtitle: Option<String>,
    /// "channel", "dm" or "group".
    pub kind: &'static str,
    /// The team name the channel badge is drawn for.
    pub team_name: String,
    pub member_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Banner {
    pub title: &'static str,
    pub detail: &'static str,
    pub offline: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PinnedView {
    pub text: String,
    pub unavailable: bool,
}

/// State with no home in `AppModels`.
#[derive(Default, Serialize)]
pub struct TimelineState {
    pub jump_to_message_active: bool,
    /// Rendered messages of the timeline and the open thread, by message id.
    pub messages: HashMap<String, MessageView>,
    pub header: HeaderView,
    pub connection_banner: Option<Banner>,
    pub pinned: Option<PinnedView>,
    pub unseen_count: usize,
    pub scroll: ScrollCommand,
    /// An older page is loading and its anchor will be restored; report no
    /// first visible message meanwhile (the restore owns the position).
    pub older_restore_pending: bool,
    /// Download control states by `source\nname`.
    pub downloads: HashMap<String, DownloadState>,

    #[serde(skip)]
    code_cache: CodeHighlightCache,
    #[serde(skip)]
    download_jobs: Downloads,
    #[serde(skip)]
    last_message_count: usize,
    #[serde(skip)]
    last_latest_message_id: Option<MessageId>,
    #[serde(skip)]
    pending_older_scroll_anchor: Option<MessageId>,
    #[serde(skip)]
    pending_older_scroll_seq: Option<u64>,
    #[serde(skip)]
    suppress_next_bottom_snap: bool,
    #[serde(skip)]
    suppress_next_unseen_increment: bool,
    #[serde(skip)]
    timeline_scroll_seq: u64,
    #[serde(skip)]
    jump_scroll_seq: Option<u64>,
    #[serde(skip)]
    last_find_query: Option<String>,
    #[serde(skip)]
    last_preview_count: usize,
}

impl std::fmt::Debug for TimelineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimelineState")
            .field("jump_to_message_active", &self.jump_to_message_active)
            .field("unseen_count", &self.unseen_count)
            .finish_non_exhaustive()
    }
}

pub fn handle(engine: &mut Engine, intent: Intent) {
    match intent {
        Intent::React { message_id, emoji } => {
            quick_react(engine, MessageId::new(message_id), emoji)
        }
        Intent::DeleteMessage { message_id } => delete_message(engine, MessageId::new(message_id)),
        Intent::CopyMessageLink { message_id } => {
            copy_message_link(engine, &MessageId::new(message_id))
        }
        Intent::CopyMessageText { message_id } => {
            copy_message_text(engine, &MessageId::new(message_id))
        }
        Intent::DiscardFailedSend { message_id } => {
            engine.dispatch(UiAction::DiscardFailedMessage {
                message_id: MessageId::new(message_id),
            })
        }
        Intent::OpenUrl { url } => super::shell::open_url_or_deep_link(engine, &url),
        Intent::OpenChannelLink {
            channel_name,
            team_name,
            conv_id,
        } => open_channel_link(engine, &channel_name, team_name, conv_id),
        Intent::OpenImage {
            message_id,
            index,
            thread,
        } => open_image(engine, &MessageId::new(message_id), index, thread),
        Intent::OpenVideo { path } => {
            let path = present::normalize_local_source_path(&path);
            if let Err(error) = engine.app.opener().open_path(&path, None::<&str>) {
                tracing::warn!("could not open {path}: {error}");
            }
        }
        Intent::Download { source, name } => {
            engine
                .timeline
                .download_jobs
                .activate(&engine.app, &source, &name);
            engine.timeline.downloads = engine.timeline.download_jobs.states.clone();
        }
        Intent::JumpToPinned => jump_to_pinned_message(engine),
        Intent::DismissPinnedBanner => {
            if engine.models.dismiss_current_pinned_banner() {
                engine.persist_settings();
                refresh_header(engine);
            }
        }
        Intent::JumpToLatest => {
            engine.timeline.unseen_count = 0;
            engine.timeline.suppress_next_unseen_increment = false;
            scroll_to(engine, ScrollTarget::Bottom);
            engine.view.timeline_near_bottom = true;
            engine.request_mark_conversation_read_if_needed();
        }
        Intent::Scrolled { near_top } => {
            engine.timeline.timeline_scroll_seq =
                engine.timeline.timeline_scroll_seq.wrapping_add(1);
            sync_scroll_indicators(engine);
            if near_top {
                request_older_page_if_needed(engine);
            }
            engine.request_mark_conversation_read_if_needed();
        }
        Intent::ScrollApplied { seq } => {
            // `jump_to_message_active` lasts until the jump has been laid out.
            if engine
                .timeline
                .jump_scroll_seq
                .is_some_and(|jump| seq >= jump)
                && engine.models.timeline.pending_scroll_target.is_none()
            {
                engine.timeline.jump_scroll_seq = None;
                engine.timeline.jump_to_message_active = false;
            }
        }
    }
    engine.mark_dirty();
}

pub fn tick(engine: &mut Engine) {
    if engine.timeline.download_jobs.drain() {
        engine.timeline.downloads = engine.timeline.download_jobs.states.clone();
        engine.mark_dirty();
    }
    // The open thread's root stays marked in the channel (`sync_timeline_view_markers`).
    let open_root = (engine.models.navigation.right_pane == RightPaneMode::Thread)
        .then(|| engine.models.thread_pane.root_message_id.clone())
        .flatten();
    if engine.models.timeline.open_thread_root_id != open_root {
        engine.models.timeline.open_thread_root_id = open_root;
        engine.mark_dirty();
    }
    // Jumps to messages already loaded (find in chat, pins) need no sync.
    if engine.models.timeline.pending_scroll_target.is_some() {
        apply_pending_scroll_target(engine);
    }
    // Find-in-chat highlighting and link previews arrive outside a sync.
    let find_query = find_query(engine);
    if engine.timeline.last_find_query.is_some() && find_query.is_none() {
        // `close_find_in_chat`: back to the newest messages.
        reset_scroll_state(engine);
        scroll_to(engine, ScrollTarget::Bottom);
    }
    let preview_count = preview_count(engine);
    if find_query != engine.timeline.last_find_query
        || preview_count != engine.timeline.last_preview_count
    {
        refresh_messages(engine);
        engine.mark_dirty();
    }
    if refresh_header(engine) {
        engine.mark_dirty();
    }
}

/// Replays the view-side effects `AppWindow::sync_models_from_store` performs
/// after the shared projection (scroll anchoring and bottom pinning).
pub fn after_sync(engine: &mut Engine, outcome: &SyncOutcome) {
    if outcome.conversation_changed {
        engine.timeline.pending_older_scroll_anchor = None;
        engine.timeline.pending_older_scroll_seq = None;
        engine.timeline.suppress_next_bottom_snap = false;
        // `navigate_to`: a newly opened conversation starts at its newest message.
        reset_scroll_state(engine);
        if engine.models.timeline.pending_scroll_target.is_none() {
            scroll_to(engine, ScrollTarget::Bottom);
        }
    }
    if outcome.timeline_rows_changed {
        // `sync_timeline_list_state_len`.
        if engine.view.timeline_near_bottom && !engine.timeline.jump_to_message_active {
            scroll_to(engine, ScrollTarget::Bottom);
        }
        if let Some(row_index) = outcome.anchor_row_index {
            let message_id = match engine.models.timeline.rows.get(row_index) {
                Some(TimelineRow::Message(row)) => Some(row.message.id.0.clone()),
                _ => None,
            };
            scroll_to(
                engine,
                ScrollTarget::Anchor {
                    message_id,
                    row_index,
                },
            );
        }
    }
    restore_pending_older_scroll_anchor(
        engine,
        outcome.loading_older,
        outcome.loading_older_just_finished,
    );
    refresh_messages(engine);
    refresh_header(engine);
    apply_pending_scroll_target(engine);
    let pending_target = engine.models.timeline.pending_scroll_target.is_some();
    if (outcome.timeline_reactions_changed
        || outcome.timeline_rows_changed
        || outcome.timeline_link_previews_changed
        || outcome.pinned_banner_visibility_changed)
        && outcome.was_near_bottom_before_sync
        && !pending_target
    {
        // Previews, reactions, edited badges and the pinned banner change
        // heights without changing the row count; stay at the latest message.
        scroll_to(engine, ScrollTarget::Bottom);
    }
    if engine.timeline.jump_to_message_active && outcome.conversation_changed {
        engine.timeline.jump_to_message_active = false;
    }
    sync_scroll_indicators(engine);
}

fn scroll_to(engine: &mut Engine, target: ScrollTarget) {
    let seq = engine.timeline.scroll.seq + 1;
    engine.timeline.scroll = ScrollCommand { seq, target };
    engine.mark_dirty();
}

fn message_count(engine: &Engine) -> usize {
    engine
        .models
        .timeline
        .rows
        .iter()
        .filter(|row| matches!(row, TimelineRow::Message(_)))
        .count()
}

fn latest_message_id(engine: &Engine) -> Option<MessageId> {
    engine
        .models
        .timeline
        .rows
        .iter()
        .rev()
        .find_map(|row| match row {
            TimelineRow::Message(row) => Some(row.message.id.clone()),
            _ => None,
        })
}

fn first_visible_message_id(engine: &Engine) -> Option<MessageId> {
    engine
        .view
        .first_visible_message_id
        .clone()
        .map(MessageId::new)
}

/// `reset_timeline_scroll_state`.
fn reset_scroll_state(engine: &mut Engine) {
    engine.timeline.unseen_count = 0;
    engine.timeline.last_message_count = message_count(engine);
    engine.timeline.last_latest_message_id = latest_message_id(engine);
    engine.timeline.pending_older_scroll_anchor = None;
    engine.timeline.pending_older_scroll_seq = None;
    engine.timeline.older_restore_pending = false;
    engine.timeline.suppress_next_bottom_snap = false;
    engine.timeline.suppress_next_unseen_increment = false;
}

/// `sync_scroll_indicators`, timeline half: new messages below while reading
/// history count as unseen; at the bottom they keep the view pinned.
fn sync_scroll_indicators(engine: &mut Engine) {
    let near_bottom = engine.view.timeline_near_bottom;
    let count = message_count(engine);
    let latest = latest_message_id(engine);
    let state = &mut engine.timeline;
    if count < state.last_message_count {
        reset_scroll_state(engine);
        return;
    }
    if count > state.last_message_count {
        let delta = count - state.last_message_count;
        let has_new_latest = match (latest.as_ref(), state.last_latest_message_id.as_ref()) {
            (Some(latest), Some(previous)) => message_id_is_after(latest, previous),
            (Some(_), None) => true,
            _ => false,
        };
        let mut scroll_bottom = false;
        if state.suppress_next_bottom_snap {
            state.suppress_next_bottom_snap = false;
        } else if !has_new_latest {
            if near_bottom {
                state.unseen_count = 0;
            }
        } else if near_bottom {
            scroll_bottom = true;
            state.unseen_count = 0;
            state.suppress_next_unseen_increment = false;
        } else if state.suppress_next_unseen_increment {
            state.unseen_count = 0;
            state.suppress_next_unseen_increment = false;
        } else {
            state.unseen_count = state.unseen_count.saturating_add(delta);
        }
        state.last_message_count = count;
        state.last_latest_message_id = latest;
        if scroll_bottom {
            scroll_to(engine, ScrollTarget::Bottom);
        }
    } else {
        if near_bottom {
            state.unseen_count = 0;
            state.suppress_next_unseen_increment = false;
        }
        state.last_latest_message_id = latest;
    }
}

/// `request_timeline_older_page_if_needed`.
fn request_older_page_if_needed(engine: &mut Engine) -> bool {
    let snapshot = engine.store.snapshot();
    if snapshot.timeline.loading_older || engine.models.timeline.rows.is_empty() {
        return false;
    }
    let Some(cursor) = snapshot
        .timeline
        .older_cursor
        .as_deref()
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string)
    else {
        return false;
    };
    engine.timeline.pending_older_scroll_anchor = first_visible_message_id(engine);
    engine.timeline.pending_older_scroll_seq = Some(engine.timeline.timeline_scroll_seq);
    engine.timeline.older_restore_pending = engine.timeline.pending_older_scroll_anchor.is_some();
    engine.timeline.suppress_next_bottom_snap = true;
    let conversation_id = engine.models.timeline.conversation_id.clone();
    engine.dispatch(UiAction::LoadOlderMessages {
        key: TimelineKey::Conversation(conversation_id),
        cursor,
    });
    true
}

/// `restore_pending_older_scroll_anchor`.
fn restore_pending_older_scroll_anchor(
    engine: &mut Engine,
    loading_older: bool,
    just_finished: bool,
) {
    if loading_older {
        return;
    }
    let state = &mut engine.timeline;
    let anchor = state.pending_older_scroll_anchor.take();
    let seq = state.pending_older_scroll_seq.take();
    state.older_restore_pending = false;
    if !just_finished {
        return;
    }
    // The user scrolled again since asking: their position wins.
    // (Older pages are requested from a scroll event, which bumps the seq once.)
    if seq.is_some_and(|seq| seq.wrapping_add(1) < state.timeline_scroll_seq) {
        return;
    }
    if let Some(anchor) = anchor
        && let Some(row_index) =
            timeline_row_index_for_message(&engine.models.timeline.rows, &anchor)
    {
        scroll_to(
            engine,
            ScrollTarget::Anchor {
                message_id: Some(anchor.0),
                row_index,
            },
        );
    }
    if message_count(engine) <= engine.timeline.last_message_count {
        engine.timeline.suppress_next_bottom_snap = false;
    }
}

/// `apply_pending_timeline_scroll_target`.
fn apply_pending_scroll_target(engine: &mut Engine) {
    let Some(target) = engine.models.timeline.pending_scroll_target.clone() else {
        return;
    };
    if timeline_row_index_for_message(&engine.models.timeline.rows, &target).is_none() {
        return;
    }
    engine.models.timeline.pending_scroll_target = None;
    scroll_to(
        engine,
        ScrollTarget::Message {
            message_id: target.0,
            offset: 0.,
        },
    );
    engine.timeline.jump_scroll_seq = Some(engine.timeline.scroll.seq);
    // A jump is not news: what loads around it must not count as unseen.
    engine.timeline.last_message_count = message_count(engine);
    engine.timeline.last_latest_message_id = latest_message_id(engine);
    engine.timeline.unseen_count = 0;
    engine.timeline.suppress_next_unseen_increment = true;
}

/// `jump_to_pinned_message`.
fn jump_to_pinned_message(engine: &mut Engine) {
    let Some(message_id) = engine
        .models
        .conversation
        .pinned_message
        .as_ref()
        .and_then(|pinned| pinned.message_id().cloned())
    else {
        return;
    };
    engine.models.timeline.pending_scroll_target = Some(message_id.clone());
    engine.models.timeline.highlighted_message_id = Some(message_id.clone());
    apply_pending_scroll_target(engine);
    if engine.models.timeline.pending_scroll_target.is_some()
        && let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone()
    {
        engine.dispatch(UiAction::JumpToMessage {
            conversation_id,
            message_id,
        });
    }
    engine.timeline.suppress_next_unseen_increment = true;
}

fn find_query(engine: &Engine) -> Option<String> {
    engine
        .models
        .find_in_chat
        .open
        .then(|| engine.models.find_in_chat.query.clone())
}

fn preview_count(engine: &Engine) -> usize {
    timeline_messages(engine)
        .chain(engine.models.thread_pane.replies.iter())
        .map(|message| {
            message.link_previews.len()
                + message
                    .link_previews
                    .iter()
                    .filter(|preview| preview.github_pr.is_some())
                    .count()
        })
        .sum()
}

fn timeline_messages(engine: &Engine) -> impl Iterator<Item = &MessageRecord> {
    engine
        .models
        .timeline
        .rows
        .iter()
        .filter_map(|row| match row {
            TimelineRow::Message(row) => Some(&row.message),
            _ => None,
        })
}

/// Rebuilds the rendered messages of the timeline and the open thread.
fn refresh_messages(engine: &mut Engine) {
    let find_query = find_query(engine);
    let inputs = Inputs {
        timeline: &engine.models.timeline,
        find_query: find_query.as_deref(),
        upload_progress: &engine.models.timeline.upload_progress,
    };
    let mut messages = HashMap::new();
    let cache = &mut engine.timeline.code_cache;
    for message in engine
        .models
        .timeline
        .rows
        .iter()
        .filter_map(|row| match row {
            TimelineRow::Message(row) => Some(&row.message),
            _ => None,
        })
        .chain(engine.models.thread_pane.replies.iter())
    {
        if messages.contains_key(&message.id.0) {
            continue;
        }
        messages.insert(message.id.0.clone(), message_view(message, &inputs, cache));
    }
    engine.timeline.messages = messages;
    engine.timeline.last_preview_count = preview_count(engine);
    engine.timeline.last_find_query = find_query;
}

/// Header title and subtitle (`ConversationHeader`), the connection banner,
/// and the pinned banner's text. True when any changed.
fn refresh_header(engine: &mut Engine) -> bool {
    let conversation = &engine.models.conversation;
    let summary = &conversation.summary;
    let member_count = conversation
        .details
        .as_ref()
        .map(|details| details.member_count)
        .filter(|count| *count > 0)
        .unwrap_or(conversation.member_count);
    let topic = summary.topic.trim();
    let (kind, title, subtitle) = match summary.kind {
        ConversationKind::Channel => {
            let team = if summary.is_simple_team() {
                Some(format!("#{}", summary.title))
            } else {
                summary
                    .group
                    .as_ref()
                    .map(|group| group.display_name.clone())
            };
            let subtitle = match (team, topic.is_empty()) {
                (Some(team), false) => Some(format!("{team} · {topic}")),
                (Some(team), true) => Some(team),
                (None, false) => Some(topic.to_string()),
                (None, true) => None,
            };
            (
                "channel",
                summary
                    .simple_team_label()
                    .unwrap_or_else(|| format!("#{}", summary.title)),
                subtitle,
            )
        }
        ConversationKind::DirectMessage => {
            let subtitle = conversation
                .participant_avatars
                .first()
                .and_then(|participant| participant.presence.as_ref())
                .and_then(|presence| {
                    let label = presence_label(&presence.availability)?;
                    Some(
                        match presence
                            .status_text
                            .as_deref()
                            .map(str::trim)
                            .filter(|status| !status.is_empty())
                        {
                            Some(status) => format!("{label} · {status}"),
                            None => label.to_string(),
                        },
                    )
                });
            ("dm", summary.title.clone(), subtitle)
        }
        ConversationKind::GroupDirectMessage => (
            "group",
            summary.title.clone(),
            (member_count > 0).then(|| format!("{member_count} members")),
        ),
    };
    let header = HeaderView {
        title,
        subtitle,
        kind,
        team_name: summary
            .group
            .as_ref()
            .map(|group| group.display_name.clone())
            .unwrap_or_else(|| summary.title.clone()),
        member_count,
    };
    let connectivity = engine.models.app.connectivity.clone();
    let banner = connectivity.send_notice().map(|notice| Banner {
        title: notice.banner_title,
        detail: notice.banner_detail,
        offline: connectivity == Connectivity::Offline,
    });
    let pinned = conversation
        .pinned_message
        .as_ref()
        .map(|pinned| PinnedView {
            text: pinned
                .preview
                .as_ref()
                .and_then(|preview| preview.text.as_deref())
                .map(compact_pinned_preview_text)
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| "Pinned message".to_string()),
            unavailable: !matches!(pinned.target, PinnedTarget::Message { .. }),
        });
    let state = &mut engine.timeline;
    let changed =
        state.header != header || state.connection_banner != banner || state.pinned != pinned;
    state.header = header;
    state.connection_banner = banner;
    state.pinned = pinned;
    changed
}

/// `ensure_message_binding_for_active_conversation`.
fn ensure_message_binding(engine: &mut Engine, message_id: &MessageId) {
    let snapshot = engine.store.snapshot();
    if snapshot.backend.message_bindings.contains_key(message_id) {
        return;
    }
    let Some(conversation_id) = snapshot.timeline.conversation_id.clone() else {
        return;
    };
    let Some(conversation_binding) = snapshot
        .backend
        .conversation_bindings
        .get(&conversation_id)
        .cloned()
    else {
        return;
    };
    let binding = MessageBinding {
        message_id: message_id.clone(),
        backend_id: conversation_binding.backend_id,
        account_id: conversation_binding.account_id,
        provider_message_ref: ProviderMessageRef::new(message_id.0.clone()),
    };
    engine.router.register_message_binding(binding.clone());
    engine.store.register_message_binding(binding);
}

/// `quick_react`.
fn quick_react(engine: &mut Engine, message_id: MessageId, emoji: String) {
    let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone() else {
        return;
    };
    ensure_message_binding(engine, &message_id);
    engine.dispatch(UiAction::ReactToMessage {
        conversation_id,
        message_id,
        emoji: emoji.clone(),
    });
    engine.models.add_recent_emoji_alias(emoji);
    engine.persist_settings();
}

/// `delete_message`.
fn delete_message(engine: &mut Engine, message_id: MessageId) {
    ensure_message_binding(engine, &message_id);
    if engine.models.delete_message(&message_id)
        && let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone()
    {
        engine.dispatch(UiAction::DeleteMessage {
            conversation_id,
            message_id,
        });
    }
}

fn find_message<'a>(engine: &'a Engine, message_id: &MessageId) -> Option<&'a MessageRecord> {
    timeline_messages(engine)
        .chain(engine.models.thread_pane.replies.iter())
        .find(|message| &message.id == message_id)
}

/// `copy_message_link`.
fn copy_message_link(engine: &mut Engine, message_id: &MessageId) {
    let stored = find_message(engine, message_id)
        .map(|message| message.permalink.clone())
        .filter(|link| !link.is_empty());
    let link = stored.unwrap_or_else(|| {
        let summary = &engine.models.conversation.summary;
        match &summary.group {
            Some(group) => format!(
                "keybase://chat/{}#{}/{}",
                group.display_name, summary.topic, message_id.0
            ),
            None => format!("keybase://chat/{}/{}", summary.topic, message_id.0),
        }
    });
    write_clipboard(&link);
    engine.models.push_toast(
        "Copied message link",
        Some(ToastAction::OpenCurrentConversation),
    );
}

/// `copy_message_text`.
fn copy_message_text(engine: &mut Engine, message_id: &MessageId) {
    match find_message(engine, message_id)
        .and_then(|message| message.source_text.clone())
        .filter(|text| !text.is_empty())
    {
        Some(text) => {
            write_clipboard(&text);
            engine.models.push_toast("Copied message text", None);
        }
        None => engine.models.push_toast("No text to copy", None),
    }
}

/// gpui's `write_to_clipboard`, through the platform's clipboard tool.
fn write_clipboard(text: &str) {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
    };
    for (program, args) in candidates {
        let Ok(mut child) = Command::new(program)
            .args(*args)
            .stdin(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().is_ok_and(|status| status.success()) {
            return;
        }
    }
    tracing::warn!("could not write to the clipboard");
}

/// `open_image_lightbox`, for the image at `index` of a timeline or thread message.
fn open_image(engine: &mut Engine, message_id: &MessageId, index: usize, thread: bool) {
    let message = if thread {
        engine
            .models
            .thread_pane
            .replies
            .iter()
            .find(|message| &message.id == message_id)
    } else {
        timeline_messages(engine).find(|message| &message.id == message_id)
    }
    .or_else(|| find_message(engine, message_id));
    let Some(message) = message else {
        return;
    };
    let Some(attachment) = message.attachments.get(index) else {
        return;
    };
    let Some(source) = present::attachment_lightbox_source(attachment) else {
        return;
    };
    let caption = message
        .attachments
        .iter()
        .all(|attachment| attachment.kind == crate::domain::attachment::AttachmentKind::Image)
        .then(|| present::message_caption_text(message))
        .flatten();
    let preview = attachment.preview.as_ref();
    let width = attachment
        .width
        .or(preview.and_then(|preview| preview.width));
    let height = attachment
        .height
        .or(preview.and_then(|preview| preview.height));
    engine
        .models
        .open_image_lightbox(source, caption, width, height);
}

/// `navigate_to_channel_link`: by conversation id when the event carries one,
/// else by name in the current team; channels you are not in are resolved
/// through the backend and opened as a preview.
fn open_channel_link(
    engine: &mut Engine,
    channel_name: &str,
    team_name: Option<String>,
    conv_id: Option<String>,
) {
    use crate::{
        domain::{ids::ConversationId, route::Route},
        models::store_sync::{conversation_id_from_navigated_route, looks_like_keybase_team_id},
        state::effect::BackendCommand,
    };
    let workspace_id = engine.models.app.active_workspace_id.clone();
    let sidebar_route = |engine: &Engine, matches: &dyn Fn(&Route, &str) -> bool| {
        engine
            .models
            .sidebar
            .sections
            .iter()
            .flat_map(|section| &section.rows)
            .find(|row| matches(&row.route, &row.label))
            .map(|row| row.route.clone())
    };
    if let Some(conv_id) = conv_id {
        let found = sidebar_route(
            engine,
            &|route, _| matches!(route, Route::Channel { channel_id, .. } if channel_id.0 == conv_id),
        );
        match found {
            Some(route) => super::shell::navigate_to(engine, route),
            None => resolve_and_navigate(
                engine,
                BackendCommand::ResolveChannelById {
                    workspace_id,
                    conversation_id: ConversationId::new(conv_id),
                },
                None,
            ),
        }
        return;
    }
    let name = channel_name
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_string();
    if name.is_empty() {
        engine.models.push_toast("Channel name is empty", None);
        return;
    }
    let current = conversation_id_from_navigated_route(&engine.models.navigation.current);
    let group_of = |engine: &Engine, id: &ConversationId| {
        engine
            .models
            .workspace
            .channels
            .iter()
            .find(|channel| &channel.id == id)
            .and_then(|channel| channel.group.as_ref())
            .map(|group| group.id.clone())
    };
    let current_team = team_name
        .map(|team| team.trim().to_string())
        .filter(|team| !team.is_empty())
        .or_else(|| current.as_ref().and_then(|id| group_of(engine, id)))
        .or_else(|| {
            engine
                .models
                .conversation
                .summary
                .group
                .as_ref()
                .map(|group| group.id.clone())
        })
        .or_else(|| {
            current.as_ref().and_then(|id| {
                engine
                    .store
                    .snapshot()
                    .backend
                    .conversation_team_ids
                    .get(id)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty() && !looks_like_keybase_team_id(value))
            })
        });
    let Some(team) = current_team else {
        tracing::warn!("no team context for #{name}");
        return;
    };
    let found = sidebar_route(engine, &|route, label| {
        label.eq_ignore_ascii_case(&name)
            && matches!(route, Route::Channel { channel_id, .. }
                if group_of(engine, &ConversationId::new(channel_id.0.clone())).as_ref() == Some(&team))
    });
    match found {
        Some(route) => super::shell::navigate_to(engine, route),
        None => resolve_and_navigate(
            engine,
            BackendCommand::ResolveChannel {
                workspace_id,
                team_name: team.clone(),
                channel_name: name.clone(),
            },
            Some(format!("Unable to open #{name} in {team}")),
        ),
    }
}

/// `resolve_channel_and_navigate` / `navigate_to_channel_by_conv_id`.
fn resolve_and_navigate(
    engine: &mut Engine,
    command: crate::state::effect::BackendCommand,
    failure_toast: Option<String>,
) {
    use crate::{
        domain::{ids::ChannelId, route::Route},
        state::event::BackendEvent,
    };
    let Ok(events) = engine.router.route_command(command) else {
        return;
    };
    for event in events {
        match event {
            BackendEvent::ChannelResolved {
                workspace_id,
                conversation,
                conversation_binding,
                can_post,
            } => {
                engine
                    .router
                    .register_conversation_binding(conversation_binding.clone());
                let effects = engine
                    .store
                    .dispatch_backend(BackendEvent::ChannelResolved {
                        workspace_id: workspace_id.clone(),
                        conversation: conversation.clone(),
                        conversation_binding,
                        can_post,
                    });
                engine.run_effects(effects);
                engine
                    .preview_summaries
                    .insert(conversation.id.clone(), (conversation.clone(), can_post));
                super::shell::navigate_to(
                    engine,
                    Route::Channel {
                        workspace_id,
                        channel_id: ChannelId::new(conversation.id.0.clone()),
                    },
                );
                engine.models.composer.conversation_id = conversation.id.clone();
                engine.models.conversation.summary = conversation;
                engine.models.conversation.can_post = can_post;
                return;
            }
            BackendEvent::ChannelResolveFailed { error, .. } => {
                tracing::warn!("channel resolve failed: {error}");
                return;
            }
            other => {
                let effects = engine.store.dispatch_backend(other);
                engine.run_effects(effects);
            }
        }
    }
    engine.sync_models();
    if let Some(toast) = failure_toast {
        engine.models.push_toast(toast, None);
    }
}
