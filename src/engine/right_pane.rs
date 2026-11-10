//! Right pane: threads, members, details, files, in-conversation search, the
//! profile panel and profile card, and the window growing to fit the pane.
//!
//! Ports the right-pane controller half of `AppWindow` (toggle_simple_pane,
//! open_thread, close_right_pane, open_user_profile_card, the profile actions,
//! the files pane, leave channel, thread resize) and `views/pane_window.rs`.

mod pane_window;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use super::{Engine, shell, timeline};
use crate::{
    domain::{
        affinity::Affinity,
        attachment::{AttachmentKind, AttachmentSource, AttachmentSummary},
        backend::ProviderMessageRef,
        conversation::ConversationKind,
        ids::{ConversationId, MessageId, UserId},
        message::{MessageFragment, MessageRecord},
        presence::{Availability, Presence},
        profile::SocialGraphListType,
        route::Route,
        user::UserSummary,
    },
    models::{
        AppModels,
        navigation_model::RightPaneMode,
        notifications_model::ToastAction,
        timeline_model::{MessageRow, TimelineModel, TimelineRow},
    },
    state::{MessageBinding, UiAction, state::TimelineKey},
};
use pane_window::PaneWindowSizing;

/// `views::RIGHT_PANE_RESIZE_HANDLE_WIDTH_PX` and the other shell constants
/// `shell_layout` and `constrained_right_pane_width` use.
const RIGHT_PANE_RESIZE_HANDLE_WIDTH_PX: f32 = 4.0;
const MAIN_PANEL_MIN_WIDTH_PX: f32 = 560.0;
const SHELL_HORIZONTAL_PADDING_PX: f32 = 0.0;
const SHELL_GAP_PX: f32 = 0.0;
const SIDE_PANEL_OFF_TOAST: &str = "The side panel is turned off in Settings";

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    /// ToggleThreadPane / ToggleMembersPane / ToggleDetailsPane / OpenFilesPane /
    /// OpenSearchPane: `pane` is "thread", "members", "details", "files" or "search".
    TogglePane {
        pane: String,
    },
    /// The pane header's close button.
    Close,
    /// Reply in thread, thread summaries; toggles when the thread is already open.
    OpenThread {
        root_id: String,
    },
    /// `toggle_thread_following`.
    ToggleThreadFollowing,
    /// Resize drag on the pane's left edge; `viewport_width` is the window's.
    SetWidth {
        width_px: f32,
        viewport_width: f32,
    },
    IncreaseWidth,
    DecreaseWidth,
    /// Details pane "Leave Channel".
    LeaveChannel,
    /// Files pane tab: "media" or "documents".
    SelectFilesTab {
        tab: String,
    },
    LoadOlderFiles,
    /// Files pane "↗": jump to the message in the conversation.
    ShowFileInConversation {
        message_id: String,
    },
    /// A file in the Files or Details pane, by message and attachment index.
    OpenAttachment {
        message_id: String,
        index: usize,
    },
    /// `open_user_profile_card` / `open_user_profile_panel`: toggles the profile pane.
    OpenProfile {
        user_id: String,
    },
    /// The DM header avatar: the other person's profile.
    OpenDmHeaderProfile,
    /// The sidebar identity menu: your own profile.
    OpenOwnProfile,
    /// Profile "Message".
    ProfileOpenMessage {
        user_id: String,
    },
    /// Followers / Following tab, and Retry: `list` is "followers" or "following".
    ProfileSelectSocialTab {
        list: String,
    },
    FollowUser {
        user_id: String,
    },
    UnfollowUser {
        user_id: String,
    },
    /// `AppModels::show_profile_card` / `hide_profile_card`.
    ShowProfileCard {
        user_id: String,
    },
    HideProfileCard,
    /// A proof or site link in the profile.
    OpenUrl {
        url: String,
    },
    /// A hit in the search pane: `open_search_result_at`.
    OpenSearchResult {
        index: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum FilesTab {
    #[default]
    Media,
    Documents,
}

/// One attachment in the loaded messages, for the Files and Details panes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FileItem {
    pub message_id: MessageId,
    pub index: usize,
    pub author: String,
    pub timestamp_ms: Option<i64>,
    pub attachment: AttachmentSummary,
    /// `attachment_image_source`: what a thumbnail shows, as a path or URL.
    pub thumbnail: Option<String>,
    pub is_media: bool,
}

/// Who wrote a search hit (`AppWindow::message_hit_context`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchHitContext {
    pub index: usize,
    pub author_name: String,
    pub author_avatar: Option<String>,
}

/// Window-resize transition facts `AppWindow::render` reads from `PaneWindowSizing`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct PaneWindowState {
    /// Growing the window for a newly opened pane: keep the chat layout until done.
    pub expanding: bool,
    /// Shrinking the window after a close: keep showing the closed pane until done.
    pub closing: bool,
}

/// State with no home in `AppModels`.
#[derive(Default, Serialize)]
pub struct RightPaneState {
    /// `build_thread_message_rows`: root and replies, sorted and grouped.
    pub thread_rows: Vec<MessageRow>,
    /// Replies, not counting the root.
    pub thread_reply_count: usize,
    /// `conversation_location`: "#general" or "Alice Johnson".
    pub location: Option<String>,
    /// The files pane tab of the open conversation.
    pub files_tab: FilesTab,
    /// Every attachment in the loaded messages, in timeline order.
    pub files: Vec<FileItem>,
    /// More history to load for the files pane, and whether it is loading.
    pub files_has_older: bool,
    pub files_loading_older: bool,
    /// Authors for the search hits of the open conversation.
    pub search_contexts: Vec<SearchHitContext>,
    pub window: PaneWindowState,
    #[serde(skip)]
    files_tabs: HashMap<ConversationId, FilesTab>,
    #[serde(skip)]
    sizing: PaneWindowSizing,
}

impl RightPaneState {
    /// `PaneWindowSizing::base_width`: the window width without the space the
    /// app added for the pane, which is what the shell should remember.
    pub fn window_base_width(&self, current: f32) -> f32 {
        self.sizing.base_width(current)
    }

    /// The window width before a pane widened it, if one did.
    pub fn window_base(&self) -> Option<f32> {
        self.sizing.base()
    }

    /// Whether the current window resize was made by the app, not the user.
    pub fn automatic_window_resize(&self) -> bool {
        self.sizing.automatic_resize()
    }
}

pub fn handle(engine: &mut Engine, intent: Intent) {
    match intent {
        Intent::TogglePane { pane } => {
            let Some(pane) = parse_pane(&pane) else {
                tracing::warn!("unknown right pane {pane}");
                return;
            };
            if pane == RightPaneMode::Files {
                let conversation = engine.models.timeline.conversation_id.clone();
                engine
                    .right_pane
                    .files_tabs
                    .entry(conversation)
                    .or_default();
            }
            toggle_simple_pane(engine, pane);
        }
        Intent::Close => close_right_pane_state(engine),
        Intent::OpenThread { root_id } => open_thread(engine, MessageId::new(root_id)),
        Intent::ToggleThreadFollowing => {
            engine.models.toggle_thread_following();
        }
        Intent::SetWidth {
            width_px,
            viewport_width,
        } => {
            let width = constrained_right_pane_width(
                width_px,
                engine.models.sidebar.width_px,
                viewport_width,
            );
            engine.models.set_thread_width(width);
        }
        Intent::IncreaseWidth => {
            let next = engine.models.thread_pane.width_px + 32.0;
            engine.models.set_thread_width(next);
        }
        Intent::DecreaseWidth => {
            let next = engine.models.thread_pane.width_px - 32.0;
            engine.models.set_thread_width(next);
        }
        Intent::LeaveChannel => {
            let conversation_id = engine.models.conversation.summary.id.clone();
            engine.dispatch(UiAction::LeaveChannel { conversation_id });
        }
        Intent::SelectFilesTab { tab } => {
            let tab = if tab.eq_ignore_ascii_case("documents") {
                FilesTab::Documents
            } else {
                FilesTab::Media
            };
            let conversation = engine.models.timeline.conversation_id.clone();
            engine.right_pane.files_tabs.insert(conversation, tab);
        }
        Intent::LoadOlderFiles => request_timeline_older_page(engine),
        Intent::ShowFileInConversation { message_id } => {
            show_file_in_conversation(engine, MessageId::new(message_id))
        }
        Intent::OpenAttachment { message_id, index } => {
            let attachment =
                find_message_record(&engine.models.timeline, &MessageId::new(message_id))
                    .and_then(|message| message.attachments.get(index).cloned());
            match attachment {
                Some(attachment) => open_files_attachment(engine, &attachment),
                None => notify_attachment_not_ready(engine),
            }
        }
        Intent::OpenProfile { user_id } => open_user_profile_card(engine, UserId::new(user_id)),
        Intent::OpenDmHeaderProfile => {
            let conversation_id = &engine.models.conversation.summary.id;
            let username = engine
                .models
                .workspace
                .direct_messages
                .iter()
                .find(|dm| dm.id == *conversation_id)
                .and_then(|dm| {
                    dm.title
                        .split(',')
                        .map(str::trim)
                        .find(|name| !name.is_empty())
                        .map(str::to_string)
                });
            if let Some(username) = username {
                open_user_profile_card(engine, UserId::new(username));
            }
        }
        Intent::OpenOwnProfile => {
            if let Some(user_id) = engine.models.app.current_user_id.clone() {
                open_user_profile_card(engine, user_id);
            }
        }
        Intent::ProfileOpenMessage { user_id } => {
            engine.dispatch(UiAction::OpenOrCreateDirectMessage {
                user_id: UserId::new(user_id),
            });
        }
        Intent::ProfileSelectSocialTab { list } => {
            let Some(user_id) = engine.models.profile_panel.user_id.clone() else {
                return;
            };
            let list_type = if list.eq_ignore_ascii_case("following") {
                SocialGraphListType::Following
            } else {
                SocialGraphListType::Followers
            };
            engine.dispatch(UiAction::LoadSocialGraphList { user_id, list_type });
        }
        Intent::FollowUser { user_id } => engine.dispatch(UiAction::FollowUser {
            user_id: UserId::new(user_id),
        }),
        Intent::UnfollowUser { user_id } => engine.dispatch(UiAction::UnfollowUser {
            user_id: UserId::new(user_id),
        }),
        Intent::ShowProfileCard { user_id } => {
            let user_id = UserId::new(user_id);
            engine.models.show_profile_card(user_id.clone());
            // The card renders from the profile panel model; load the profile it shows.
            engine.dispatch(UiAction::ShowUserProfilePanel { user_id });
        }
        // The card's Close is `dismiss_overlays`.
        Intent::HideProfileCard => shell::dismiss_overlays(engine),
        Intent::OpenSearchResult { index } => {
            if let Some(result) = engine.models.search.results.get(index).cloned() {
                shell::navigate_to_message(engine, result.route, result.message.id);
            }
        }
        Intent::OpenUrl { url } => {
            if let Err(error) = engine.app.opener().open_url(&url, None::<&str>) {
                tracing::warn!("could not open {url}: {error}");
            }
        }
    }
    engine.mark_dirty();
}

pub fn tick(engine: &mut Engine) {
    refresh_state(engine);
    sync_pane_window_size(engine);
}

fn parse_pane(pane: &str) -> Option<RightPaneMode> {
    Some(match pane {
        "thread" => RightPaneMode::Thread,
        "members" => RightPaneMode::Members,
        "details" => RightPaneMode::Details,
        "files" => RightPaneMode::Files,
        "search" => RightPaneMode::Search,
        _ => return None,
    })
}

/// `AppWindow::toggle_simple_pane`.
fn toggle_simple_pane(engine: &mut Engine, pane: RightPaneMode) {
    if pane != RightPaneMode::Hidden && !engine.models.settings.show_right_pane {
        engine
            .models
            .push_toast(SIDE_PANEL_OFF_TOAST, Some(ToastAction::OpenPreferences));
        return;
    }
    if engine.models.navigation.right_pane == pane {
        close_right_pane_state(engine);
    } else {
        // Members: each opening starts with the full list; the web view's
        // filter field mounts empty with the pane.
        engine.models.set_right_pane(pane);
    }
}

/// `AppWindow::close_right_pane_state`. The closing snapshot lives in the web view.
fn close_right_pane_state(engine: &mut Engine) {
    engine.dispatch(UiAction::CloseRightPane);
    engine.models.set_right_pane(RightPaneMode::Hidden);
    engine.models.profile_panel.user_id = None;
    engine.models.profile_panel.profile = None;
}

/// `AppWindow::open_thread`.
fn open_thread(engine: &mut Engine, root_id: MessageId) {
    if !engine.models.settings.show_right_pane {
        engine.models.push_toast(
            "Enable the right pane before opening threads",
            Some(ToastAction::OpenPreferences),
        );
        return;
    }
    if engine.models.navigation.right_pane == RightPaneMode::Thread
        && engine.models.thread_pane.root_message_id.as_ref() == Some(&root_id)
    {
        close_right_pane_state(engine);
        return;
    }
    ensure_message_binding_for_active_conversation(engine, &root_id);
    engine.models.open_thread(root_id.clone());
    engine.dispatch(UiAction::OpenThread { root_id });
}

/// `AppWindow::ensure_message_binding_for_active_conversation`.
fn ensure_message_binding_for_active_conversation(engine: &mut Engine, message_id: &MessageId) {
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

/// `AppWindow::open_user_profile_card`: toggles the profile pane for `user_id`.
fn open_user_profile_card(engine: &mut Engine, user_id: UserId) {
    if engine.models.navigation.right_pane == RightPaneMode::Profile(user_id.clone()) {
        close_right_pane_state(engine);
        return;
    }
    if !engine.models.settings.show_right_pane {
        engine
            .models
            .push_toast(SIDE_PANEL_OFF_TOAST, Some(ToastAction::OpenPreferences));
        return;
    }
    engine.models.hide_profile_card();
    engine.models.profile_panel.user_id = Some(user_id.clone());
    engine
        .models
        .set_right_pane(RightPaneMode::Profile(user_id.clone()));
    engine.dispatch(UiAction::ShowUserProfilePanel {
        user_id: user_id.clone(),
    });
    let conversation_id = engine.store.snapshot().timeline.conversation_id.clone();
    engine.dispatch(UiAction::RefreshProfilePresence {
        user_id,
        conversation_id,
    });
}

/// `AppWindow::request_timeline_older_page_if_needed`, without the scroll
/// anchor, which the timeline view keeps itself.
fn request_timeline_older_page(engine: &mut Engine) {
    let snapshot = engine.store.snapshot();
    if snapshot.timeline.loading_older || engine.models.timeline.rows.is_empty() {
        return;
    }
    let Some(cursor) = snapshot
        .timeline
        .older_cursor
        .as_deref()
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let conversation_id = engine.models.timeline.conversation_id.clone();
    engine.dispatch(UiAction::LoadOlderMessages {
        key: TimelineKey::Conversation(conversation_id),
        cursor,
    });
}

/// `AppWindow::show_file_in_conversation` (= `jump_to_find_match`).
fn show_file_in_conversation(engine: &mut Engine, message_id: MessageId) {
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

/// `AppWindow::open_files_attachment`.
fn open_files_attachment(engine: &mut Engine, attachment: &AttachmentSummary) {
    if attachment.kind == AttachmentKind::Image
        && let Some(source) = attachment_lightbox_source(attachment)
    {
        engine.models.open_image_lightbox(
            source,
            Some(attachment.name.clone()),
            attachment.width,
            attachment.height,
        );
    } else if let Some(path) = attachment_local_path(attachment) {
        if matches!(
            attachment.kind,
            AttachmentKind::Video | AttachmentKind::Audio
        ) {
            // `open_video_in_native_player`.
            timeline::handle(engine, timeline::Intent::OpenVideo { path });
        } else {
            // `save_attachment_copy`: the timeline's download jobs save, then open.
            let name = attachment.name.clone();
            timeline::handle(engine, timeline::Intent::Download { source: path, name });
        }
    } else if let Some(target) = attachment_open_target(attachment) {
        if target.starts_with("https://") || target.starts_with("http://") {
            // `download_attachment_and_open`.
            let name = attachment.name.clone();
            timeline::handle(
                engine,
                timeline::Intent::Download {
                    source: target,
                    name,
                },
            );
        } else {
            timeline::handle(engine, timeline::Intent::OpenUrl { url: target });
        }
    } else {
        notify_attachment_not_ready(engine);
    }
}

fn notify_attachment_not_ready(engine: &mut Engine) {
    engine
        .models
        .push_toast("File is not available yet. Try again in a moment.", None);
}

/// `AppWindow::sync_pane_window_size`: grow a narrow window when the pane opens.
fn sync_pane_window_size(engine: &mut Engine) {
    let pane_width = (!matches!(engine.models.navigation.right_pane, RightPaneMode::Hidden))
        .then_some(engine.models.thread_pane.width_px);
    let state = &mut engine.right_pane;
    if pane_width.is_none() && state.sizing.idle() {
        // Nothing to grow or give back; the next open learns the size again.
        return;
    }
    let Some(window) = engine.app.get_webview_window("main") else {
        return;
    };
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let size = size.to_logical::<f64>(scale);
    let width = size.width as f32;
    let resizable =
        !window.is_fullscreen().unwrap_or(false) && !window.is_maximized().unwrap_or(false);
    let maximum = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            monitor
                .size()
                .to_logical::<f64>(monitor.scale_factor())
                .width as f32
        })
        .unwrap_or(width + pane_width.unwrap_or(0.));
    let required = min_shell_width(pane_width.unwrap_or(0.), engine.models.sidebar.width_px);
    if let Some(target) = state
        .sizing
        .sync(width, pane_width, required, maximum, resizable)
        && let Some(generation) = state.sizing.queued_generation()
        && state.sizing.submit(generation)
        && let Err(error) = window.set_size(tauri::LogicalSize::new(target as f64, size.height))
    {
        tracing::warn!("could not resize the window for the right pane: {error}");
    }
    let next = PaneWindowState {
        expanding: state.sizing.expanding(),
        closing: state.sizing.closing_width().is_some() && pane_width.is_none(),
    };
    if state.window != next {
        state.window = next;
        engine.mark_dirty();
    }
}

/// `shell_layout(..).min_shell_width`.
fn min_shell_width(pane_width: f32, sidebar_width: f32) -> f32 {
    let base = SHELL_HORIZONTAL_PADDING_PX
        + sidebar_width
        + RIGHT_PANE_RESIZE_HANDLE_WIDTH_PX
        + MAIN_PANEL_MIN_WIDTH_PX
        + SHELL_GAP_PX;
    if pane_width > 0.0 {
        base + SHELL_GAP_PX + pane_width
    } else {
        base
    }
}

/// `constrained_right_pane_width`.
fn constrained_right_pane_width(requested: f32, sidebar_width: f32, viewport_width: f32) -> f32 {
    let available = viewport_width
        - SHELL_HORIZONTAL_PADDING_PX
        - sidebar_width
        - RIGHT_PANE_RESIZE_HANDLE_WIDTH_PX
        - MAIN_PANEL_MIN_WIDTH_PX
        - 2.0 * SHELL_GAP_PX;
    requested.clamp(280.0, available.clamp(280.0, 520.0))
}

/// Derives what the panes render from the models, for the open mode only.
fn refresh_state(engine: &mut Engine) {
    let models = &engine.models;
    let mode = models.navigation.right_pane.clone();
    let state = &mut engine.right_pane;
    state.location = conversation_location(models);
    state.files_tab = state
        .files_tabs
        .get(&models.timeline.conversation_id)
        .copied()
        .unwrap_or_default();

    if mode == RightPaneMode::Thread {
        let rows = build_thread_message_rows(models);
        state.thread_reply_count = rows
            .iter()
            .filter(|row| models.thread_pane.root_message_id.as_ref() != Some(&row.message.id))
            .count();
        state.thread_rows = rows;
    } else if !state.thread_rows.is_empty() && !state.window.closing {
        state.thread_rows.clear();
        state.thread_reply_count = 0;
    }

    if matches!(mode, RightPaneMode::Files | RightPaneMode::Details) {
        state.files = file_items(&models.timeline);
        state.files_has_older = models
            .timeline
            .older_cursor
            .as_ref()
            .is_some_and(|cursor| !cursor.trim().is_empty());
        state.files_loading_older = models.timeline.loading_older;
    } else if !state.files.is_empty() && !state.window.closing {
        state.files.clear();
    }

    if mode == RightPaneMode::Search {
        let profiles = &engine.store.snapshot().backend.user_profiles;
        let conversation_id = &models.conversation.summary.id;
        state.search_contexts = models
            .search
            .results
            .iter()
            .enumerate()
            .filter(|(_, result)| match &result.route {
                Route::Channel { channel_id, .. } => channel_id.0 == conversation_id.0,
                Route::DirectMessage { dm_id, .. } => dm_id.0 == conversation_id.0,
                _ => false,
            })
            .map(|(index, result)| {
                let author = &result.message.author_id;
                let profile = profiles
                    .get(author)
                    .or_else(|| profiles.get(&UserId::new(author.0.to_ascii_lowercase())));
                SearchHitContext {
                    index,
                    author_name: profile
                        .map(|profile| profile.display_name.trim())
                        .filter(|name| !name.is_empty())
                        .unwrap_or(author.0.as_str())
                        .to_string(),
                    author_avatar: profile.and_then(|profile| profile.avatar_asset.clone()),
                }
            })
            .collect();
    } else if !state.search_contexts.is_empty() && !state.window.closing {
        state.search_contexts.clear();
    }
}

/// `conversation_location` in views/right_pane.
fn conversation_location(models: &AppModels) -> Option<String> {
    let summary = &models.conversation.summary;
    let title = summary.title.trim();
    if title.is_empty() {
        return None;
    }
    Some(match summary.kind {
        ConversationKind::Channel => format!("#{title}"),
        _ => title.to_string(),
    })
}

/// `files::items` for both tabs, in timeline order.
fn file_items(timeline: &TimelineModel) -> Vec<FileItem> {
    timeline
        .rows
        .iter()
        .filter_map(|row| match row {
            TimelineRow::Message(row) => Some(row),
            _ => None,
        })
        .flat_map(|row| {
            row.message
                .attachments
                .iter()
                .enumerate()
                .map(move |(index, attachment)| FileItem {
                    message_id: row.message.id.clone(),
                    index,
                    author: row.author.display_name.clone(),
                    timestamp_ms: row.message.timestamp_ms,
                    attachment: attachment.clone(),
                    thumbnail: attachment_image_source(attachment),
                    is_media: matches!(
                        attachment.kind,
                        AttachmentKind::Image | AttachmentKind::Video
                    ),
                })
        })
        .collect()
}

/// `build_thread_message_rows` in views/right_pane.
fn build_thread_message_rows(models: &AppModels) -> Vec<MessageRow> {
    const NON_TEXT_PLACEHOLDER_BODY: &str = "<non-text message>";
    const GROUP_WINDOW_MS: i64 = 15 * 60 * 1000;
    fn is_non_text_placeholder_message(message: &MessageRecord) -> bool {
        message.event.is_none()
            && message.attachments.is_empty()
            && message.link_previews.is_empty()
            && message.fragments.len() == 1
            && matches!(
                message.fragments.first(),
                Some(MessageFragment::Text(text)) if text.trim() == NON_TEXT_PLACEHOLDER_BODY
            )
    }

    let thread = &models.thread_pane;
    let timeline = &models.timeline;
    let mut root_message_fallback = None;
    if let Some(root_id) = thread.root_message_id.as_ref()
        && !thread.replies.iter().any(|message| &message.id == root_id)
    {
        root_message_fallback = find_message_record(timeline, root_id);
    }
    let mut messages = thread.replies.iter().collect::<Vec<_>>();
    if let Some(root_message) = root_message_fallback.as_ref() {
        messages.push(root_message);
    }
    messages.sort_by_key(|message| message.id.0.parse::<u64>().unwrap_or(0));
    let mut seen_ids = HashSet::new();
    messages.retain(|message| seen_ids.insert(message.id.clone()));
    messages.retain(|message| !is_non_text_placeholder_message(message));

    let author_lookup = build_thread_author_lookup(timeline);
    let mut rows = Vec::with_capacity(messages.len());
    let mut previous: Option<(UserId, Option<i64>)> = None;
    for message in messages {
        let show_header = previous.as_ref().is_none_or(|(author, timestamp)| {
            if author != &message.author_id {
                return true;
            }
            let (Some(previous), Some(current)) = (*timestamp, message.timestamp_ms) else {
                return true;
            };
            current <= previous || current.saturating_sub(previous) > GROUP_WINDOW_MS
        });
        rows.push(MessageRow {
            author: author_lookup
                .get(&message.author_id)
                .cloned()
                .unwrap_or_else(|| fallback_thread_author_summary(&message.author_id)),
            message: message.clone(),
            show_header,
        });
        previous = Some((message.author_id.clone(), message.timestamp_ms));
    }
    rows
}

fn build_thread_author_lookup(timeline: &TimelineModel) -> HashMap<UserId, UserSummary> {
    let mut lookup = HashMap::new();
    for row in &timeline.rows {
        let TimelineRow::Message(row) = row else {
            continue;
        };
        let author = row.author.clone();
        lookup
            .entry(author.id.clone())
            .or_insert_with(|| author.clone());
        lookup
            .entry(UserId::new(author.id.0.to_ascii_lowercase()))
            .or_insert(author);
    }
    lookup
}

fn fallback_thread_author_summary(author_id: &UserId) -> UserSummary {
    UserSummary {
        id: author_id.clone(),
        display_name: author_id.0.clone(),
        title: String::new(),
        avatar_asset: None,
        presence: Presence {
            availability: Availability::Offline,
            status_text: None,
        },
        affinity: Affinity::None,
    }
}

fn find_message_record(timeline: &TimelineModel, message_id: &MessageId) -> Option<MessageRecord> {
    timeline.rows.iter().find_map(|row| match row {
        TimelineRow::Message(row) if &row.message.id == message_id => Some(row.message.clone()),
        _ => None,
    })
}

// Attachment sources, as in views/mod.rs.

fn attachment_is_animated_gif(attachment: &AttachmentSummary) -> bool {
    attachment.name.to_ascii_lowercase().ends_with(".gif")
        || attachment
            .mime_type
            .as_deref()
            .is_some_and(|mime| mime.eq_ignore_ascii_case("image/gif"))
}

fn attachment_image_source(attachment: &AttachmentSummary) -> Option<String> {
    let preview = attachment.preview.as_ref().map(|preview| &preview.source);
    let source = if attachment_is_animated_gif(attachment) {
        attachment.source.as_ref().or(preview)
    } else {
        preview.or(attachment.source.as_ref())
    };
    source.and_then(renderable_source)
}

fn attachment_lightbox_source(attachment: &AttachmentSummary) -> Option<AttachmentSource> {
    attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|preview| &preview.source))
        .filter(|source| renderable_source(source).is_some())
        .cloned()
}

fn attachment_open_target(attachment: &AttachmentSummary) -> Option<String> {
    match attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|preview| &preview.source))?
    {
        AttachmentSource::Url(url) => {
            (!is_unrenderable_keybase_asset_url(url)).then(|| url.clone())
        }
        AttachmentSource::LocalPath(_) => None,
    }
}

fn attachment_local_path(attachment: &AttachmentSummary) -> Option<String> {
    match attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|preview| &preview.source))?
    {
        AttachmentSource::LocalPath(path) if !path.trim().is_empty() => Some(path.clone()),
        _ => None,
    }
}

fn renderable_source(source: &AttachmentSource) -> Option<String> {
    match source {
        AttachmentSource::Url(url) => {
            (!is_unrenderable_keybase_asset_url(url)).then(|| url.clone())
        }
        AttachmentSource::LocalPath(path) => {
            let normalized = normalize_local_source_path(path);
            (!normalized.is_empty()).then_some(normalized)
        }
    }
}

fn is_unrenderable_keybase_asset_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://127.0.0.1:") || lower.starts_with("http://localhost:") {
        return true;
    }
    let prefix = if lower.starts_with("https://s3.amazonaws.com/") {
        "https://s3.amazonaws.com/"
    } else if lower.starts_with("http://s3.amazonaws.com/") {
        "http://s3.amazonaws.com/"
    } else {
        return false;
    };
    if lower.contains("x-amz-signature=") || lower.contains("awsaccesskeyid=") {
        return false;
    }
    url[prefix.len()..]
        .split('/')
        .next()
        .is_some_and(|segment| {
            segment.len() == 64 && segment.chars().all(|ch| ch.is_ascii_hexdigit())
        })
}

fn normalize_local_source_path(raw: &str) -> String {
    let value = raw.trim();
    let Some(path) = value.strip_prefix("file://") else {
        return value.to_string();
    };
    let path = match path.strip_prefix("localhost/") {
        Some(rest) => format!("/{rest}"),
        None => path.to_string(),
    };
    percent_decode(&path)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
