//! App shell: boot, navigation, sidebar, titlebar, home and activity, settings,
//! menus and window-level behavior (`AppWindow` init, navigation and sidebar actions).
//!
//! Other areas navigate through [`navigate_to`] and [`navigate_to_message`],
//! the counterparts of `AppWindow::navigate_to` and `navigate_to_message`.

mod native;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use super::Engine;
use crate::{
    domain::{
        ids::{
            CallId, ChannelId, ConversationId, DmId, MessageId, SidebarSectionId, UserId,
            WorkspaceId,
        },
        message::MessageSendState,
        route::Route,
    },
    models::{
        navigation_model::RightPaneMode,
        notifications_model::ToastAction,
        quick_switcher_model::QuickSwitcherResultKind,
        settings_model::{Density, SidebarMode, SidebarStyle, ThemeMode},
        sidebar_preview::{PreviewNames, live_preview},
        store_sync::conversation_id_from_navigated_route,
    },
    services::updates::{self, UpdateAction, UpdateSnapshot, session},
    state::{UiAction, event::BackendEvent, state::BootPhase},
};

/// `AppWindow::SPLASH_MIN_DURATION`.
const SPLASH_MIN_DURATION: Duration = Duration::from_secs(2);
const UPDATE_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A route as the frontend sends it back: the serialized `Route`, ids as strings.
#[derive(Clone, Debug, Deserialize)]
pub enum RouteInput {
    WorkspaceHome {
        workspace_id: String,
    },
    Channel {
        workspace_id: String,
        channel_id: String,
    },
    DirectMessage {
        workspace_id: String,
        dm_id: String,
    },
    Search {
        workspace_id: String,
        query: String,
    },
    Activity {
        workspace_id: String,
    },
    Preferences,
    ActiveCall {
        workspace_id: String,
        call_id: String,
    },
}

impl From<RouteInput> for Route {
    fn from(route: RouteInput) -> Self {
        match route {
            RouteInput::WorkspaceHome { workspace_id } => Route::WorkspaceHome {
                workspace_id: WorkspaceId::new(workspace_id),
            },
            RouteInput::Channel {
                workspace_id,
                channel_id,
            } => Route::Channel {
                workspace_id: WorkspaceId::new(workspace_id),
                channel_id: ChannelId::new(channel_id),
            },
            RouteInput::DirectMessage {
                workspace_id,
                dm_id,
            } => Route::DirectMessage {
                workspace_id: WorkspaceId::new(workspace_id),
                dm_id: DmId::new(dm_id),
            },
            RouteInput::Search {
                workspace_id,
                query,
            } => Route::Search {
                workspace_id: WorkspaceId::new(workspace_id),
                query,
            },
            RouteInput::Activity { workspace_id } => Route::Activity {
                workspace_id: WorkspaceId::new(workspace_id),
            },
            RouteInput::Preferences => Route::Preferences,
            RouteInput::ActiveCall {
                workspace_id,
                call_id,
            } => Route::ActiveCall {
                workspace_id: WorkspaceId::new(workspace_id),
                call_id: CallId::new(call_id),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    /// Sidebar rows, home cards: `navigate_to`.
    Navigate {
        route: RouteInput,
    },
    /// `navigate_to_message`, e.g. from an activity item.
    NavigateToMessage {
        route: RouteInput,
        message_id: String,
    },
    /// Cmd+[ and the toolbar's back button.
    NavigateBack,
    /// Cmd+] and the toolbar's forward button.
    NavigateForward,
    /// Cmd+1: the first recent result while the quick switcher is open, else Home.
    ShowHome,
    /// Cmd+2..5 while the quick switcher is open with an empty query (0-based).
    OpenRecent {
        index: usize,
    },
    /// Cmd+Shift+A.
    ShowActivity,
    OpenActivityItem {
        index: usize,
    },
    /// Alt+Up / Alt+Down.
    SelectSidebar {
        direction: isize,
    },
    /// Alt+Enter.
    ActivateSidebarSelection,
    /// "Recent" or "Grouped".
    SetSidebarMode {
        mode: String,
    },
    /// "Compact" or "Previews".
    SetSidebarStyle {
        style: String,
    },
    SetSidebarWidth {
        width_px: f32,
    },
    SetSidebarFilter {
        filter: String,
    },
    ToggleSection {
        section_id: String,
    },
    ReorderSection {
        dragged_id: String,
        target_id: String,
    },
    OpenOwnProfile,
    OpenPreferences,
    ClosePreferences,
    /// "Light", "Dark" or "System".
    SetThemeMode {
        mode: String,
    },
    /// "Comfortable" or "Compact".
    SetDensity {
        density: String,
    },
    SetReducedMotion {
        reduced: bool,
    },
    SetShowRightPane {
        show: bool,
    },
    /// `open_url_or_deep_link`: `keybase://chat/...`, `zbase-mention:`, `zbase-channel:`, or the browser.
    OpenUrl {
        url: String,
    },
    ActivateToast {
        index: usize,
    },
    DismissToast {
        index: usize,
    },
    MarkAllRead,
    CheckForUpdates,
    ShowUpdates,
    /// The update panel's click-away or Escape; `restore` is unused here (focus stays in the web view).
    CloseUpdates,
    /// "check", "download", "later", "skip", "cancel", "restart", "toggle_checks", "toggle_downloads".
    UpdateCommand {
        action: String,
    },
    ToggleUpdateNotes,
    StartOrOpenCall,
    LeaveCall,
    ToggleCallMute,
    ToggleCallScreenShare,
    /// Cmd+Shift+I.
    ToggleKeybaseInspector,
    InspectorTogglePause,
    InspectorToggleUnknownOnly,
    InspectorClear,
    /// The web view copied the rows; `count` is how many.
    InspectorCopied {
        count: usize,
    },
    CloseKeybaseInspector,
    /// Cmd+Shift+S.
    ToggleSplash,
    /// Escape while the inspector or splash shows: `AppWindow::dismiss_overlays`.
    DismissOverlays,
    /// Cmd+W.
    CloseWindow {
        window: String,
    },
    /// Cmd+Q and zBase › Quit.
    Quit,
}

/// Shell state with no home in `AppModels`.
#[derive(Debug, Serialize)]
pub struct ShellState {
    pub splash_open: bool,
    pub splash_status: String,
    /// The version zBase reports (the main crate's), for Settings and the update panel.
    pub version: String,
    pub update: UpdateUi,
    pub inspector: InspectorView,
    /// Bumped by every navigation, so the focused input can follow the route
    /// (`restore_chat_focus`).
    pub navigation_seq: u64,
    /// Ask the web view to focus an input: "composer" or "thread".
    pub focus_request: Option<FocusRequest>,
    #[serde(skip)]
    splash_shown_at: Instant,
    #[serde(skip)]
    splash_boot_ready: bool,
    #[serde(skip)]
    splash_close_at: Option<Instant>,
    /// Who is typing in each conversation (`sidebar_typing`).
    #[serde(skip)]
    sidebar_typing: HashMap<ConversationId, Vec<UserId>>,
    /// Conversations whose newest message is yours and failed to send (`sidebar_failed_sends`).
    #[serde(skip)]
    sidebar_failed_sends: HashMap<ConversationId, i64>,
    #[serde(skip)]
    inspector_entries: HashMap<String, InspectorEntry>,
    #[serde(skip)]
    last_update_poll: Option<Instant>,
    #[serde(skip)]
    reminder: session::Reminder,
    #[serde(skip)]
    native: native::Native,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            splash_open: true,
            splash_status: String::new(),
            version: zbase_version(),
            update: UpdateUi::default(),
            inspector: InspectorView::default(),
            navigation_seq: 0,
            focus_request: None,
            splash_shown_at: Instant::now(),
            splash_boot_ready: false,
            splash_close_at: None,
            sidebar_typing: HashMap::new(),
            sidebar_failed_sends: HashMap::new(),
            inspector_entries: HashMap::new(),
            last_update_poll: None,
            reminder: session::Reminder::load(),
            native: native::Native::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FocusRequest {
    pub seq: u64,
    pub target: &'static str,
}

/// `updates::UpdateUi`, without the gpui focus and text handles.
#[derive(Debug, Default, Serialize)]
pub struct UpdateUi {
    pub snapshot: UpdateSnapshot,
    pub open: bool,
    pub notes_open: bool,
    pub error: Option<String>,
    /// `snapshot.has_reminder()`: the toolbar's update button shows.
    pub has_reminder: bool,
}

/// What the Keybase inspector shows (`KeybaseInspectorState`).
#[derive(Debug, Default, Serialize)]
pub struct InspectorView {
    pub open: bool,
    pub paused: bool,
    pub unknown_only: bool,
    pub seq: u64,
    pub method_count: usize,
    /// Newest first, filtered as `keybase_inspector_entries` filters them.
    pub rows: Vec<InspectorEntry>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct InspectorEntry {
    pub method: String,
    pub count: u64,
    pub last_seen_seq: u64,
    pub payload_preview: Option<String>,
}

/// The app version, as gpui showed it.
fn zbase_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn start(engine: &mut Engine) {
    let style = engine.models.settings.sidebar_style;
    engine.shell.native.install(&engine.app, style);
    engine.shell.splash_shown_at = Instant::now();
    #[cfg(feature = "demo-fixtures")]
    if engine.is_demo {
        engine.shell.splash_open = false;
        super::demo::apply_scene(engine);
        refresh(engine);
        return;
    }
    engine.dispatch(UiAction::StartApp);
    let route = engine.models.navigation.current.clone();
    engine.dispatch(UiAction::Navigate(route));
    on_main_thread(&engine.app, updates::start);
    refresh(engine);
}

pub fn handle(engine: &mut Engine, intent: Intent) {
    match intent {
        Intent::Navigate { route } => navigate_to(engine, route.into()),
        Intent::NavigateToMessage { route, message_id } => {
            navigate_to_message(engine, route.into(), MessageId::new(message_id))
        }
        Intent::NavigateBack => navigate_history(engine, false),
        Intent::NavigateForward => navigate_history(engine, true),
        Intent::ShowHome => {
            if !open_quick_switcher_recent_result(engine, 0) {
                let route = engine.home_route();
                navigate_to(engine, route);
            }
        }
        Intent::OpenRecent { index } => {
            open_quick_switcher_recent_result(engine, index);
        }
        Intent::ShowActivity => {
            let route = activity_route(engine);
            navigate_to(engine, route);
        }
        Intent::OpenActivityItem { index } => {
            if let Some((route, message_id)) = engine.models.open_activity_item(index) {
                match message_id {
                    Some(message_id) => navigate_to_message(engine, route, message_id),
                    None => navigate_to(engine, route),
                }
            }
        }
        Intent::SelectSidebar { direction } => {
            engine.models.move_sidebar_highlight(direction);
            if let Some(route) = engine.models.sidebar.highlighted_route.clone()
                && route != engine.models.navigation.current
            {
                navigate_to(engine, route);
                return;
            }
            refresh(engine);
        }
        Intent::ActivateSidebarSelection => {
            if let Some(route) = engine.models.sidebar.highlighted_route.clone() {
                navigate_to(engine, route);
            }
        }
        Intent::SetSidebarMode { mode } => {
            let mode = match mode.as_str() {
                "Grouped" => SidebarMode::Grouped,
                _ => SidebarMode::Recent,
            };
            engine.models.set_sidebar_mode(mode);
            engine.persist_settings();
            refresh(engine);
        }
        Intent::SetSidebarStyle { style } => {
            let style = match style.as_str() {
                "Previews" => SidebarStyle::Previews,
                _ => SidebarStyle::Compact,
            };
            if engine.models.set_sidebar_style(style) {
                engine.persist_settings();
                refresh(engine);
            }
        }
        Intent::SetSidebarWidth { width_px } => {
            engine.models.set_sidebar_width(width_px);
            refresh(engine);
        }
        Intent::SetSidebarFilter { filter } => {
            engine.models.set_sidebar_filter(filter.clone());
            engine.dispatch(UiAction::SetSidebarFilter(filter));
        }
        Intent::ToggleSection { section_id } => toggle_section(engine, section_id),
        Intent::ReorderSection {
            dragged_id,
            target_id,
        } => reorder_section(engine, dragged_id, target_id),
        Intent::OpenOwnProfile => {
            if let Some(user_id) = engine.models.app.current_user_id.clone() {
                open_user_profile_card(engine, user_id);
            }
        }
        Intent::OpenPreferences => open_preferences(engine),
        Intent::ClosePreferences => native::close_window(&engine.app, native::SETTINGS_WINDOW),
        Intent::SetThemeMode { mode } => {
            let mode = match mode.as_str() {
                "Light" => ThemeMode::Light,
                "Dark" => ThemeMode::Dark,
                _ => ThemeMode::System,
            };
            if engine.models.set_theme_mode(mode) {
                engine.persist_settings();
                refresh(engine);
            }
        }
        Intent::SetDensity { density } => {
            let density = match density.as_str() {
                "Compact" => Density::Compact,
                _ => Density::Comfortable,
            };
            if engine.models.set_density(density) {
                engine.persist_settings();
                refresh(engine);
            }
        }
        Intent::SetReducedMotion { reduced } => {
            if engine.models.set_reduced_motion(reduced) {
                engine.persist_settings();
                refresh(engine);
            }
        }
        Intent::SetShowRightPane { show } => {
            if !show && engine.models.navigation.right_pane != RightPaneMode::Hidden {
                close_right_pane_state(engine);
            }
            if engine.models.set_show_right_pane(show) {
                engine.persist_settings();
                refresh(engine);
            }
        }
        Intent::OpenUrl { url } => open_url_or_deep_link(engine, &url),
        Intent::ActivateToast { index } => activate_toast(engine, index),
        Intent::DismissToast { index } => {
            engine.models.dismiss_toast(index);
            refresh(engine);
        }
        Intent::MarkAllRead => mark_all_conversations_read(engine),
        Intent::CheckForUpdates => {
            engine.shell.update.open = true;
            on_main_thread(&engine.app, move || updates::action(UpdateAction::Check));
            engine.shell.last_update_poll = None;
            poll_updates(engine);
            refresh(engine);
        }
        Intent::ShowUpdates => {
            engine.shell.update.open = true;
            refresh(engine);
        }
        Intent::CloseUpdates => close_update_panel(engine),
        Intent::UpdateCommand { action } => {
            if let Some(action) = update_action(&action) {
                update_command(engine, action);
                if matches!(action, UpdateAction::Later | UpdateAction::Skip) {
                    close_update_panel(engine);
                }
            }
        }
        Intent::ToggleUpdateNotes => {
            engine.shell.update.notes_open = !engine.shell.update.notes_open;
            engine.mark_dirty();
        }
        Intent::StartOrOpenCall => start_or_open_call(engine),
        Intent::LeaveCall => {
            let on_call_route =
                matches!(engine.models.navigation.current, Route::ActiveCall { .. });
            engine.models.leave_call();
            if on_call_route {
                let route = current_conversation_route(engine);
                navigate_to(engine, route);
            } else {
                refresh(engine);
            }
        }
        Intent::ToggleCallMute => {
            engine.models.toggle_call_mute();
            refresh(engine);
        }
        Intent::ToggleCallScreenShare => {
            engine.models.toggle_call_screen_share();
            refresh(engine);
        }
        Intent::ToggleKeybaseInspector => {
            engine.shell.inspector.open = !engine.shell.inspector.open;
            if engine.shell.inspector.open {
                if engine.models.overlay.new_chat_open || engine.models.new_chat.open {
                    engine.dispatch(UiAction::CloseNewChat);
                }
                engine.models.dismiss_overlays();
            }
            sync_inspector_rows(engine);
            refresh(engine);
        }
        Intent::InspectorTogglePause => {
            engine.shell.inspector.paused = !engine.shell.inspector.paused;
            refresh(engine);
        }
        Intent::InspectorToggleUnknownOnly => {
            engine.shell.inspector.unknown_only = !engine.shell.inspector.unknown_only;
            sync_inspector_rows(engine);
            refresh(engine);
        }
        Intent::InspectorClear => {
            engine.shell.inspector_entries.clear();
            engine.shell.inspector.seq = 0;
            sync_inspector_rows(engine);
            refresh(engine);
        }
        Intent::InspectorCopied { count } => {
            engine.models.push_toast(
                format!("Copied {count} inspector rows"),
                Some(ToastAction::OpenCurrentConversation),
            );
            refresh(engine);
        }
        Intent::CloseKeybaseInspector => {
            engine.shell.inspector.open = false;
            refresh(engine);
        }
        Intent::ToggleSplash => {
            engine.shell.splash_open = !engine.shell.splash_open;
            if engine.shell.splash_open {
                engine.shell.inspector.open = false;
                engine.models.dismiss_overlays();
            }
            refresh(engine);
        }
        Intent::DismissOverlays => dismiss_overlays(engine),
        Intent::CloseWindow { window } => {
            if window == native::MAIN_WINDOW {
                engine.shell.native.flush_window_preferences();
            }
            native::close_window(&engine.app, &window);
        }
        Intent::Quit => {
            engine.shell.native.flush_window_preferences();
            engine.app.exit(0);
        }
    }
}

/// What `AppWindow::refresh` does besides repainting.
pub fn tick(engine: &mut Engine) {
    engine.shell.native.set_pane_sizing(
        engine.right_pane.window_base(),
        engine.right_pane.automatic_window_resize(),
    );
    let toasts = engine.store.take_toasts();
    if !toasts.is_empty() {
        for error in toasts {
            engine.models.push_toast(error, None);
        }
        engine.mark_dirty();
    }
    sync_sidebar_live_previews(engine);
    check_splash_dismiss(engine);
    poll_updates(engine);
    let mode = engine.models.sidebar.mode;
    engine.shell.native.sync_view_menu(mode);
    let count = dock_badge_count(engine);
    engine.shell.native.sync_dock_badge(&engine.app, count);
    let theme = engine.models.settings.theme_mode.clone();
    engine.shell.native.sync_theme(&engine.app, &theme);
}

/// Sees each backend event before the reducer, like `drain_backend_events`.
pub fn observe_backend_event(engine: &mut Engine, event: &BackendEvent) {
    match event {
        BackendEvent::TypingUpdated {
            conversation_id,
            users,
        } => {
            let typing = &mut engine.shell.sidebar_typing;
            if users.is_empty() {
                typing.remove(conversation_id);
            } else {
                typing.insert(conversation_id.clone(), users.clone());
            }
        }
        BackendEvent::KeybaseNotifyStub {
            method,
            payload_preview,
        } if engine.shell.inspector.open => {
            record_keybase_stub_event(engine, method.clone(), payload_preview.clone());
        }
        _ => {}
    }
}

fn refresh(engine: &mut Engine) {
    tick(engine);
    engine.mark_dirty();
}

fn activity_route(engine: &Engine) -> Route {
    Route::Activity {
        workspace_id: engine.models.app.active_workspace_id.clone(),
    }
}

/// `current_conversation_route`.
fn current_conversation_route(engine: &Engine) -> Route {
    Route::Channel {
        workspace_id: engine.models.app.active_workspace_id.clone(),
        channel_id: ChannelId::new(engine.models.conversation.summary.id.0.clone()),
    }
}

/// `AppWindow::navigate_to`. Settings open in their own window.
pub fn navigate_to(engine: &mut Engine, route: Route) {
    if route == Route::Preferences {
        open_preferences(engine);
        return;
    }
    engine.models.dismiss_overlays();
    engine.timeline.jump_to_message_active = false;
    let route_changed = engine.models.navigation.current != route;
    close_right_pane_when_leaving_conversation(engine, &route, route_changed);
    if route_changed {
        engine.models.navigate_to(route.clone());
    }
    engine.dispatch(UiAction::Navigate(route.clone()));
    if let Some(conversation_id) = conversation_id_from_navigated_route(&route) {
        engine.dispatch(UiAction::MarkConversationRead {
            conversation_id,
            message_id: None,
        });
    }
    engine.models.expand_section_for_route(&route);
    engine.shell.navigation_seq += 1;
    refresh(engine);
}

/// `AppWindow::navigate_to_message`: open `route` scrolled to `message_id`.
pub fn navigate_to_message(engine: &mut Engine, route: Route, message_id: MessageId) {
    engine.models.dismiss_overlays();
    let route_changed = engine.models.navigation.current != route;
    close_right_pane_when_leaving_conversation(engine, &route, route_changed);
    if route_changed {
        engine.models.navigate_to(route.clone());
    }
    // Sets navigation without LoadConversation, which would race the jump.
    engine.dispatch(UiAction::NavigateQuiet(route.clone()));
    engine.models.expand_section_for_route(&route);
    engine.models.timeline.pending_scroll_target = Some(message_id.clone());
    engine.models.timeline.highlighted_message_id = Some(message_id.clone());
    engine.timeline.jump_to_message_active = true;
    if let Some(conversation_id) = engine.store.snapshot().timeline.conversation_id.clone() {
        engine.dispatch(UiAction::JumpToMessage {
            conversation_id,
            message_id,
        });
    }
    if route_changed {
        engine.dispatch(UiAction::Navigate(route));
    }
    engine.shell.navigation_seq += 1;
    refresh(engine);
}

/// Panes are scoped to a conversation: switching to another closes them.
fn close_right_pane_when_leaving_conversation(engine: &mut Engine, route: &Route, changed: bool) {
    if changed
        && conversation_id_from_navigated_route(route).is_some()
        && conversation_id_from_navigated_route(&engine.models.navigation.current)
            != conversation_id_from_navigated_route(route)
        && engine.models.navigation.right_pane != RightPaneMode::Hidden
    {
        close_right_pane_state(engine);
    }
}

/// `close_right_pane_state`, without the gpui closing animation.
fn close_right_pane_state(engine: &mut Engine) {
    engine.dispatch(UiAction::CloseRightPane);
    engine.models.set_right_pane(RightPaneMode::Hidden);
    engine.models.profile_panel.user_id = None;
    engine.models.profile_panel.profile = None;
}

/// `navigate_back` / `navigate_forward`.
fn navigate_history(engine: &mut Engine, forward: bool) {
    let moved = if forward {
        engine.models.navigate_forward()
    } else {
        engine.models.navigate_back()
    };
    if !moved {
        return;
    }
    let route = engine.models.navigation.current.clone();
    engine.dispatch(UiAction::Navigate(route.clone()));
    engine.models.expand_section_for_route(&route);
    engine.shell.navigation_seq += 1;
    refresh(engine);
}

/// `open_quick_switcher_recent_result`: Cmd+1..5 pick a recent conversation
/// while the quick switcher shows them (open with an empty query).
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
    // `open_quick_switcher_result_at`.
    let result = engine.models.quick_switcher.results[index].clone();
    engine.models.quick_switcher.selected_index = index;
    if result.kind != QuickSwitcherResultKind::Message {
        engine
            .models
            .record_quick_switcher_selection_affinity(&result.conversation_id);
        engine.persist_settings();
    }
    match result.message_id {
        Some(message_id) => navigate_to_message(engine, result.route, message_id),
        None => navigate_to(engine, result.route),
    }
    true
}

/// `toggle_sidebar_section_click`: collapse state is saved per account.
fn toggle_section(engine: &mut Engine, section_id: String) {
    engine
        .models
        .toggle_sidebar_section(&SidebarSectionId::new(section_id));
    let account_key = engine.models.app.active_workspace_id.0.clone();
    let collapsed = engine
        .models
        .sidebar
        .sections
        .iter()
        .filter(|section| section.collapsed && section.id.0 != "unread")
        .map(|section| section.id.0.clone())
        .collect();
    engine
        .models
        .settings
        .sidebar_collapsed_sections
        .insert(account_key, collapsed);
    engine.persist_settings();
    refresh(engine);
}

/// `reorder_sidebar_section`: the order is saved per account.
fn reorder_section(engine: &mut Engine, dragged_id: String, target_id: String) {
    engine.models.reorder_sidebar_section(
        &SidebarSectionId::new(dragged_id),
        &SidebarSectionId::new(target_id),
    );
    let account_key = engine.models.app.active_workspace_id.0.clone();
    let order = engine
        .models
        .sidebar
        .sections
        .iter()
        .filter(|section| section.id.0 != "unread")
        .map(|section| section.id.0.clone())
        .collect();
    engine
        .models
        .settings
        .sidebar_section_order
        .insert(account_key, order);
    engine.persist_settings();
    refresh(engine);
}

fn open_preferences(engine: &mut Engine) {
    engine.models.dismiss_overlays();
    native::open_settings_window(&engine.app);
    engine.mark_dirty();
}

/// `open_user_profile_card`: the profile pane, or closes it when it shows this user.
fn open_user_profile_card(engine: &mut Engine, user_id: UserId) {
    if engine.models.navigation.right_pane == RightPaneMode::Profile(user_id.clone()) {
        close_right_pane_state(engine);
        refresh(engine);
        return;
    }
    if !engine.models.settings.show_right_pane {
        engine.models.push_toast(
            "The side panel is turned off in Settings",
            Some(ToastAction::OpenPreferences),
        );
        refresh(engine);
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
    refresh(engine);
}

/// `open_url_or_deep_link`.
pub fn open_url_or_deep_link(engine: &mut Engine, url: &str) {
    if let Some((route, message_id)) = engine.models.resolve_deep_link(url) {
        navigate_to_message(engine, route, message_id);
    } else if let Some(username) = url.strip_prefix("zbase-mention:") {
        open_user_profile_card(engine, UserId::new(username.to_string()));
    } else if let Some(channel_name) = url.strip_prefix("zbase-channel:") {
        if let Some(route) = engine.models.resolve_channel_mention(channel_name) {
            navigate_to(engine, route);
        }
    } else if url.starts_with("keybase://") {
        // A link to a conversation this account doesn't list: nothing to open.
        tracing::info!("unresolved keybase link: {url}");
    } else {
        use tauri_plugin_opener::OpenerExt;
        if let Err(error) = engine.app.opener().open_url(url, None::<&str>) {
            tracing::warn!("could not open {url}: {error}");
        }
    }
}

/// `activate_toast`.
fn activate_toast(engine: &mut Engine, index: usize) {
    let action = engine
        .models
        .notifications
        .toasts
        .get(index)
        .and_then(|toast| toast.action.clone());
    engine.models.dismiss_toast(index);
    let Some(action) = action else {
        refresh(engine);
        return;
    };
    match action {
        ToastAction::OpenPreferences => open_preferences(engine),
        ToastAction::FocusComposer => {
            let route = current_conversation_route(engine);
            navigate_to(engine, route);
            request_focus(engine, "composer");
        }
        ToastAction::OpenThread | ToastAction::FocusThreadReply => {
            match engine.models.thread_pane.root_message_id.clone() {
                Some(root_id) => {
                    // `AppWindow::open_thread`.
                    engine.models.open_thread(root_id.clone());
                    engine.dispatch(UiAction::OpenThread { root_id });
                    if matches!(action, ToastAction::FocusThreadReply) {
                        request_focus(engine, "thread");
                    }
                }
                None => refresh(engine),
            }
        }
        ToastAction::OpenCurrentConversation => {
            let route = current_conversation_route(engine);
            navigate_to(engine, route);
        }
        ToastAction::OpenActiveCall => match engine.models.active_call_route() {
            Some(route) => navigate_to(engine, route),
            None => refresh(engine),
        },
    }
}

fn request_focus(engine: &mut Engine, target: &'static str) {
    let seq = engine
        .shell
        .focus_request
        .as_ref()
        .map_or(0, |request| request.seq)
        + 1;
    engine.shell.focus_request = Some(FocusRequest { seq, target });
    engine.mark_dirty();
}

/// `AppWindow::dismiss_overlays`: every overlay, the update panel, the
/// inspector and the splash.
pub fn dismiss_overlays(engine: &mut Engine) {
    engine.shell.update.open = false;
    if engine.models.overlay.new_chat_open || engine.models.new_chat.open {
        engine.dispatch(UiAction::CloseNewChat);
    }
    engine.models.dismiss_overlays();
    engine.shell.inspector.open = false;
    engine.shell.splash_open = false;
    refresh(engine);
}

/// `mark_all_conversations_read` (command palette's "Mark all as read").
pub fn mark_all_conversations_read(engine: &mut Engine) {
    let unread = engine
        .models
        .workspace
        .channels
        .iter()
        .chain(engine.models.workspace.direct_messages.iter())
        .filter(|conversation| conversation.unread_count > 0)
        .map(|conversation| conversation.id.clone())
        .collect::<Vec<_>>();
    let count = unread.len();
    for conversation_id in unread {
        engine.dispatch(UiAction::MarkConversationRead {
            conversation_id,
            message_id: None,
        });
    }
    if count > 0 {
        engine.models.push_toast(
            format!(
                "Marked {} conversation{} as read",
                count,
                if count == 1 { "" } else { "s" }
            ),
            None,
        );
    }
    refresh(engine);
}

/// `start_or_open_call`.
pub fn start_or_open_call(engine: &mut Engine) {
    match engine.models.start_or_open_call() {
        Some(route) => navigate_to(engine, route),
        None => refresh(engine),
    }
}

/// `sync_sidebar_live_previews`: a failed send, someone typing, or your unsent draft.
fn sync_sidebar_live_previews(engine: &mut Engine) {
    let snapshot = engine.store.snapshot();
    if let Some(open) = snapshot.timeline.conversation_id.clone() {
        let me = engine.models.app.current_user_id.as_ref();
        let failed = snapshot.timeline.messages.last().filter(|message| {
            Some(&message.author_id) == me && message.send_state == MessageSendState::Failed
        });
        match failed {
            Some(message) => {
                engine
                    .shell
                    .sidebar_failed_sends
                    .insert(open, message.timestamp_ms.unwrap_or(i64::MAX));
            }
            None => {
                engine.shell.sidebar_failed_sends.remove(&open);
            }
        }
    }
    let models = &engine.models;
    let names = PreviewNames {
        me: models.app.current_user_id.as_ref().map(|id| id.0.as_str()),
        display_names: &models.quick_switcher_profile_names,
    };
    let drafts = models.composer_draft_texts().collect::<HashMap<_, _>>();
    let open = &models.timeline.conversation_id;
    let live = models
        .workspace
        .channels
        .iter()
        .chain(&models.workspace.direct_messages)
        .filter_map(|summary| {
            let typing = engine
                .shell
                .sidebar_typing
                .get(&summary.id)
                .and_then(|users| {
                    names.typing_line(&users.iter().map(|user| user.0.as_str()).collect::<Vec<_>>())
                });
            let newest = summary
                .last_message
                .as_ref()
                .map_or(summary.last_activity_ms, |message| message.timestamp_ms);
            let not_sent = engine
                .shell
                .sidebar_failed_sends
                .get(&summary.id)
                .is_some_and(|failed_at| *failed_at >= newest);
            live_preview(
                typing,
                drafts.get(&summary.id).copied(),
                not_sent,
                &summary.id == open,
            )
            .map(|live| (summary.id.0.clone(), live))
        })
        .collect::<HashMap<_, _>>();
    if engine.models.sidebar.live_previews != live {
        engine.models.sidebar.live_previews = live;
        engine.mark_dirty();
    }
}

/// `check_splash_dismiss`: the splash stays at least two seconds, until boot is ready.
fn check_splash_dismiss(engine: &mut Engine) {
    let app = &engine.store.snapshot().app;
    let status = if !app.boot_status.is_empty() {
        app.boot_status.clone()
    } else {
        match app.boot_phase {
            BootPhase::Launching => "Starting up…".to_string(),
            BootPhase::HydratingLocalState => "Loading local data…".to_string(),
            BootPhase::ConnectingBackend => "Connecting to Keybase…".to_string(),
            BootPhase::Ready => String::new(),
            BootPhase::Degraded => "Connection degraded".to_string(),
            BootPhase::FatalError => "Failed to connect".to_string(),
        }
    };
    let ready = matches!(app.boot_phase, BootPhase::Ready);
    if engine.shell.splash_status != status {
        engine.shell.splash_status = status;
        engine.mark_dirty();
    }
    // gpui closes it once, from a timer started when boot became ready; a
    // splash reopened with Cmd+Shift+S later stays until toggled again.
    if engine.shell.splash_open && !engine.shell.splash_boot_ready && ready {
        engine.shell.splash_boot_ready = true;
        engine.shell.splash_close_at = Some(engine.shell.splash_shown_at + SPLASH_MIN_DURATION);
    }
    if let Some(at) = engine.shell.splash_close_at
        && Instant::now() >= at
    {
        engine.shell.splash_close_at = None;
        engine.shell.splash_open = false;
        engine.mark_dirty();
    }
}

/// `sync_dock_badge`: unread messages for DMs, Keybase's badge count for channels.
fn dock_badge_count(engine: &Engine) -> u32 {
    let workspace = &engine.store.snapshot().workspace;
    let channels = workspace.channels.iter().map(|c| c.mention_count);
    let dms = workspace
        .direct_messages
        .iter()
        .map(|c| c.unread_count.max(c.mention_count));
    channels.chain(dms).sum()
}

fn record_keybase_stub_event(engine: &mut Engine, method: String, payload_preview: Option<String>) {
    let inspector = &mut engine.shell.inspector;
    if inspector.paused {
        return;
    }
    inspector.seq = inspector.seq.saturating_add(1);
    let seq = inspector.seq;
    let entry = engine
        .shell
        .inspector_entries
        .entry(method.clone())
        .or_insert_with(|| InspectorEntry {
            method,
            ..InspectorEntry::default()
        });
    entry.count = entry.count.saturating_add(1);
    entry.last_seen_seq = seq;
    entry.payload_preview = payload_preview;
    sync_inspector_rows(engine);
    engine.mark_dirty();
}

/// `keybase_inspector_entries`: newest first, internal methods only when asked.
fn sync_inspector_rows(engine: &mut Engine) {
    let unknown_only = engine.shell.inspector.unknown_only;
    let mut rows = engine
        .shell
        .inspector_entries
        .values()
        .filter(|entry| !unknown_only || entry.method.starts_with("zbase.internal."))
        .cloned()
        .collect::<Vec<_>>();
    rows.sort_by_key(|entry| std::cmp::Reverse(entry.last_seen_seq));
    engine.shell.inspector.method_count = engine.shell.inspector_entries.len();
    engine.shell.inspector.rows = rows;
}

fn update_action(action: &str) -> Option<UpdateAction> {
    Some(match action {
        "check" => UpdateAction::Check,
        "download" => UpdateAction::Download,
        "later" => UpdateAction::Later,
        "skip" => UpdateAction::Skip,
        "cancel" => UpdateAction::Cancel,
        "restart" => UpdateAction::Restart,
        "toggle_checks" => UpdateAction::ToggleChecks,
        "toggle_downloads" => UpdateAction::ToggleDownloads,
        _ => return None,
    })
}

/// `poll_updates`.
fn poll_updates(engine: &mut Engine) {
    if engine
        .shell
        .last_update_poll
        .is_some_and(|at| at.elapsed() < UPDATE_POLL_INTERVAL)
    {
        return;
    }
    engine.shell.last_update_poll = Some(Instant::now());
    let update = &mut engine.shell.update;
    let was_open = update.open;
    let mut snapshot = on_main_thread(&engine.app, updates::snapshot).unwrap_or_default();
    let attention = snapshot.attention;
    snapshot.attention = false;
    let mut changed = snapshot != update.snapshot;
    if update.error.as_deref() == Some("Wait for messages and uploads to finish before restarting.")
        && engine.models.update_restart_blocker().is_none()
        && !engine.router.has_inflight_sends()
    {
        update.error = None;
        changed = true;
    }
    if attention {
        update.open = true;
    }
    let now = chrono::Utc::now().timestamp();
    if matches!(
        snapshot.state.as_str(),
        "available" | "downloaded" | "ready"
    ) && engine.shell.reminder.due(&snapshot.version, now)
        && !engine.shell.splash_open
    {
        update.open = true;
        engine.shell.reminder.postpone(&snapshot.version, now);
        engine.shell.reminder.save();
    }
    update.has_reminder = snapshot.has_reminder();
    update.snapshot = snapshot;
    if changed || attention || was_open != update.open {
        engine.mark_dirty();
    }
}

/// `close_update_panel`.
fn close_update_panel(engine: &mut Engine) {
    engine.shell.update.open = false;
    on_main_thread(&engine.app, move || updates::action(UpdateAction::Later));
    let version = engine.shell.update.snapshot.version.clone();
    engine
        .shell
        .reminder
        .postpone(&version, chrono::Utc::now().timestamp());
    engine.shell.reminder.save();
    engine.mark_dirty();
}

/// `update_command`. Sparkle is not linked into this binary
/// (`services::updates` is the disabled stub), so Restart only checks the
/// blockers; the draft-restoring session is never needed.
fn update_command(engine: &mut Engine, action: UpdateAction) {
    engine.shell.update.error = None;
    if matches!(action, UpdateAction::Restart) {
        let blocker = if engine.router.has_inflight_sends() {
            Some("Wait for messages and uploads to finish before restarting.")
        } else {
            engine.models.update_restart_blocker()
        };
        if let Some(reason) = blocker {
            engine.shell.update.error = Some(reason.into());
            engine.mark_dirty();
            return;
        }
        engine.shell.native.flush_window_preferences();
    }
    on_main_thread(&engine.app, move || updates::action(action));
    if matches!(action, UpdateAction::Later) {
        let version = engine.shell.update.snapshot.version.clone();
        engine
            .shell
            .reminder
            .postpone(&version, chrono::Utc::now().timestamp());
        engine.shell.reminder.save();
    }
    if matches!(action, UpdateAction::Later | UpdateAction::Skip) {
        engine.shell.update.open = false;
    }
    engine.shell.last_update_poll = None;
    poll_updates(engine);
    engine.mark_dirty();
}

/// The Sparkle bridge (native/updater.m) must run on AppKit's main thread, as
/// gpui's callers did; the engine has its own thread, so hop and wait.
fn on_main_thread<T: Send + 'static>(
    app: &tauri::AppHandle,
    call: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(call());
    })
    .ok()?;
    receiver.recv().ok()
}
