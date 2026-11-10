//! The Tauri counterpart of `AppWindow`'s non-rendering half.
//!
//! One thread owns the store, the backends and the same `AppModels` the gpui
//! app renders from. The web frontend sends [`Intent`]s, the engine applies
//! them the way `AppWindow` would, and every model section that changed is
//! emitted as a `model:<section>` event for the frontend to render.
//!
//! Feature areas live in submodules, each with its own intent enum and handler.

mod composer;
#[cfg(feature = "demo-fixtures")]
mod demo;
mod overlays;
mod right_pane;
mod shell;
mod timeline;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::{
    domain::{
        backend::{AccountId, BackendCapabilities, BackendId, ProviderWorkspaceRef},
        conversation::ConversationSummary,
        github_pr::GithubPrRef,
        ids::{ConversationId, MessageId, WorkspaceId},
        message::MessageRecord,
        route::Route,
    },
    models::{
        AppModels,
        link_previews::{
            apply_github_pr_previews, apply_og_previews, extract_link_urls_from_fragments,
        },
        store_sync::{StoreSync, SyncInputs, ViewHints},
        timeline_model::TimelineRow,
    },
    services::{
        backends::{keybase::KeybaseBackend, router::BackendRouter},
        github_pr_service::{GithubPrFetchResult, GithubPrService},
        local_store::{LocalStore, paths},
        og_service::{OgFetchResult, OgService},
        settings_store::SettingsStore,
    },
    state::{
        AccountState, AppStore, ConnectionState, UiAction, UiState, WorkspaceBinding,
        effect::Effect, event::BackendEvent, state::BootPhase,
    },
};

const POLL_BOOT_INTERVAL: Duration = Duration::from_millis(16);
const POLL_READY_INTERVAL: Duration = Duration::from_millis(50);
const MAX_BACKEND_EVENTS_PER_DRAIN: usize = 48;
const MARK_READ_THROTTLE: Duration = Duration::from_secs(2);
const GITHUB_PR_POLL_INTERVAL: Duration = Duration::from_secs(1);
const WORKSPACE_ID: &str = "ws_primary";
const ACCOUNT_ID: &str = "account_demo_keybase";
const BACKEND_ID: &str = "keybase";

/// Everything the frontend can ask for, grouped by the area that handles it.
#[derive(Debug, Deserialize)]
#[serde(tag = "area", content = "intent", rename_all = "snake_case")]
pub enum Intent {
    /// Ask for every section again, e.g. after a reload of the web view.
    Republish,
    /// Layout facts only the web view knows, reported as they change.
    View(ViewReport),
    Shell(shell::Intent),
    Timeline(timeline::Intent),
    Composer(composer::Intent),
    RightPane(right_pane::Intent),
    Overlays(overlays::Intent),
}

/// What `AppWindow` reads from gpui's layout and window state.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ViewReport {
    pub window_focused: bool,
    pub timeline_near_bottom: bool,
    pub first_visible_message_id: Option<String>,
}

/// Directories the web view may load files from: zBase's own cache (avatars,
/// emoji, downloads) and the Keybase service's cache (attachment previews).
pub fn asset_directories() -> Vec<PathBuf> {
    let root = paths::data_root();
    let _ = std::fs::create_dir_all(&root);
    let mut directories = vec![root];
    if let Some(keybase_cache) = crate::services::backends::keybase::paths::socket_path()
        .and_then(|socket| socket.parent().map(PathBuf::from))
    {
        directories.push(keybase_cache);
    }
    directories
}

pub fn spawn(app: AppHandle) -> Sender<Intent> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("zbase-engine".into())
        .spawn(move || Engine::new(app).run(receiver))
        .expect("failed to start the engine thread");
    sender
}

pub struct Engine {
    /// Showing the fictional demo workspace instead of Keybase (`engine::demo`).
    pub is_demo: bool,
    pub app: AppHandle,
    pub store: AppStore,
    pub router: BackendRouter,
    pub models: AppModels,
    pub store_sync: StoreSync,
    pub local_store: Arc<LocalStore>,
    pub view: ViewReport,
    /// Channel previews opened from search or a mention before joining.
    pub preview_summaries: HashMap<ConversationId, (ConversationSummary, bool)>,
    pub og_service: OgService,
    og_results: (Sender<OgFetchResult>, Receiver<OgFetchResult>),
    pub github_pr_service: GithubPrService,
    github_pr_results: (Sender<GithubPrFetchResult>, Receiver<GithubPrFetchResult>),
    github_pr_last_poll: Option<Instant>,
    github_pr_attention_key: Option<(ConversationId, bool)>,
    last_mark_read_attempt: HashMap<String, (String, Instant)>,
    pending_backend_events: std::collections::VecDeque<BackendEvent>,
    /// Engine state the frontend renders that `AppModels` has no place for.
    pub shell: shell::ShellState,
    pub timeline: timeline::TimelineState,
    pub composer: composer::ComposerState,
    pub right_pane: right_pane::RightPaneState,
    pub overlays: overlays::OverlaysState,
    published: HashMap<&'static str, Value>,
    dirty: bool,
}

impl Engine {
    fn new(app: AppHandle) -> Self {
        let local_store = Arc::new(
            LocalStore::open().unwrap_or_else(|error| panic!("failed to open RocksDB: {error}")),
        );
        let account_id = AccountId::new(ACCOUNT_ID);
        let backend_id = BackendId::new(BACKEND_ID);
        let workspace = WorkspaceBinding {
            workspace_id: WorkspaceId::new(WORKSPACE_ID),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_workspace_ref: ProviderWorkspaceRef::new(WORKSPACE_ID),
        };

        let settings = SettingsStore::load_from_disk().unwrap_or_default();
        let demo = demo_parts(&settings, &account_id, &backend_id);
        let is_demo = demo.is_some();
        let (models, store, router) = demo.unwrap_or_else(|| {
            let models = AppModels::empty_with_settings(settings);
            let mut store = AppStore::new(UiState::with_route(models.navigation.current.clone()));
            store.register_account(AccountState {
                account_id: account_id.clone(),
                backend_id: backend_id.clone(),
                display_name: "Keybase".into(),
                avatar: None,
                connection_state: ConnectionState::Disconnected,
                capabilities: BackendCapabilities::keybase_defaults(),
            });
            store.register_workspace_binding(workspace.clone());

            let mut router = BackendRouter::default();
            router.register_backend(Box::new(KeybaseBackend::new(Arc::clone(&local_store))));
            router.register_account(account_id, backend_id);
            router.register_workspace_binding(workspace);
            (models, store, router)
        });

        Self {
            is_demo,
            app,
            store,
            router,
            models,
            store_sync: StoreSync::default(),
            og_service: OgService::new(Arc::clone(&local_store)),
            local_store,
            view: ViewReport {
                window_focused: true,
                timeline_near_bottom: true,
                first_visible_message_id: None,
            },
            preview_summaries: HashMap::new(),
            og_results: mpsc::channel(),
            github_pr_service: GithubPrService::new(),
            github_pr_results: mpsc::channel(),
            github_pr_last_poll: None,
            github_pr_attention_key: None,
            last_mark_read_attempt: HashMap::new(),
            pending_backend_events: Default::default(),
            shell: Default::default(),
            timeline: Default::default(),
            composer: Default::default(),
            right_pane: Default::default(),
            overlays: Default::default(),
            published: HashMap::new(),
            dirty: true,
        }
    }

    fn run(mut self, intents: Receiver<Intent>) {
        shell::start(&mut self);
        loop {
            match intents.recv_timeout(self.poll_interval()) {
                Ok(intent) => {
                    self.handle(intent);
                    while let Ok(intent) = intents.try_recv() {
                        self.handle(intent);
                    }
                    self.dirty = true;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.drain_backend_events();
            self.drain_link_previews();
            shell::tick(&mut self);
            timeline::tick(&mut self);
            composer::tick(&mut self);
            right_pane::tick(&mut self);
            overlays::tick(&mut self);
            // Serializing the models walks the whole timeline, so skip idle ticks.
            if std::mem::take(&mut self.dirty) {
                self.publish();
            }
        }
    }

    fn poll_interval(&self) -> Duration {
        if !self.pending_backend_events.is_empty() {
            return POLL_BOOT_INTERVAL;
        }
        match self.store.snapshot().app.boot_phase {
            BootPhase::Ready | BootPhase::Degraded | BootPhase::FatalError => POLL_READY_INTERVAL,
            _ => POLL_BOOT_INTERVAL,
        }
    }

    fn handle(&mut self, intent: Intent) {
        match intent {
            Intent::Republish => self.published.clear(),
            Intent::View(report) => {
                let focus_gained = report.window_focused && !self.view.window_focused;
                self.view = report;
                if focus_gained {
                    self.github_pr_service.note_attention();
                }
                self.request_mark_conversation_read_if_needed();
            }
            Intent::Shell(intent) => shell::handle(self, intent),
            Intent::Timeline(intent) => timeline::handle(self, intent),
            Intent::Composer(intent) => composer::handle(self, intent),
            Intent::RightPane(intent) => right_pane::handle(self, intent),
            Intent::Overlays(intent) => overlays::handle(self, intent),
        }
    }

    /// Marks the frontend for a republish after state changed outside a handler.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// `AppWindow::dispatch_ui_action`: reduce, run backend effects to a fixed
    /// point, then project the store into the models.
    pub fn dispatch(&mut self, action: UiAction) {
        let is_update_draft = matches!(&action, UiAction::UpdateDraft { .. });
        let is_open_thread = matches!(&action, UiAction::OpenThread { .. });
        let should_sync_models = !matches!(
            &action,
            UiAction::SetSearchQuery(_)
                | UiAction::SetSidebarFilter(_)
                | UiAction::UpdateDraft { .. }
                | UiAction::QuickSwitcherSearch { .. }
                | UiAction::FindInChatSearch { .. }
        );
        let effects = self.store.dispatch_ui(action);
        if is_update_draft {
            return;
        }
        self.run_effects(effects);
        if should_sync_models || is_open_thread {
            self.sync_models();
        }
        self.dirty = true;
    }

    /// Runs backend commands until they stop producing follow-up events.
    pub fn run_effects(&mut self, mut effects: Vec<Effect>) {
        while !effects.is_empty() {
            self.run_local_effects(&effects);
            let events = self.router.apply_effects(&effects);
            effects.clear();
            for event in events {
                if self.take_search_results(&event) {
                    continue;
                }
                self.register_bindings(&event);
                effects.extend(self.store.dispatch_backend(event));
            }
        }
    }

    /// Settings and drafts are persisted by the shell, not the backends.
    fn run_local_effects(&mut self, effects: &[Effect]) {
        for effect in effects {
            if let Effect::PersistSettings = effect {
                self.persist_settings();
            }
        }
    }

    pub fn persist_settings(&self) {
        if let Err(error) = SettingsStore::save_to_disk(&self.models.settings) {
            tracing::warn!("could not save settings: {error}");
        }
    }

    /// Quick switcher and find-in-chat results bypass the store, as in `AppWindow`.
    fn take_search_results(&mut self, event: &BackendEvent) -> bool {
        let BackendEvent::SearchResults {
            query_id,
            results,
            is_complete,
        } = event
        else {
            return false;
        };
        if let Some(seq) = query_id.0.strip_prefix("quick-switcher-") {
            if seq == self.overlays.quick_switcher_query_seq.to_string() {
                self.models.apply_quick_switcher_message_results(results);
                self.models.quick_switcher.loading_messages = !*is_complete;
            }
            return true;
        }
        if let Some(seq) = query_id.0.strip_prefix("find-in-chat-") {
            if seq == self.models.find_in_chat.query_seq.to_string() {
                self.models
                    .find_in_chat
                    .apply_results(results.clone(), *is_complete);
            }
            return true;
        }
        false
    }

    fn drain_backend_events(&mut self) {
        let polled = self.router.poll_backends();
        self.pending_backend_events.extend(polled);
        if self.pending_backend_events.is_empty() {
            return;
        }
        let count = self
            .pending_backend_events
            .len()
            .min(MAX_BACKEND_EVENTS_PER_DRAIN);
        let events = self
            .pending_backend_events
            .drain(..count)
            .collect::<Vec<_>>();
        let mut effects = Vec::new();
        for event in events {
            if self.take_search_results(&event) {
                continue;
            }
            self.register_bindings(&event);
            shell::observe_backend_event(self, &event);
            overlays::observe_backend_event(self, &event);
            effects.extend(self.store.dispatch_backend(event));
        }
        self.run_effects(effects);
        self.sync_models();
        self.request_mark_conversation_read_if_needed();
        self.dirty = true;
    }

    /// The router routes by binding; mirrors `AppWindow::register_bindings_from_backend_event`.
    pub fn register_bindings(&mut self, event: &BackendEvent) {
        match event {
            BackendEvent::BootstrapLoaded { payload, .. } => {
                for binding in &payload.workspace_bindings {
                    self.router.register_workspace_binding(binding.clone());
                }
                for binding in &payload.conversation_bindings {
                    self.router.register_conversation_binding(binding.clone());
                }
                for binding in &payload.message_bindings {
                    self.router.register_message_binding(binding.clone());
                }
            }
            BackendEvent::WorkspaceConversationsExtended {
                conversation_bindings,
                ..
            } => {
                for binding in conversation_bindings {
                    self.router.register_conversation_binding(binding.clone());
                }
            }
            BackendEvent::ConversationCreated {
                conversation_binding,
                ..
            }
            | BackendEvent::ChannelResolved {
                conversation_binding,
                ..
            } => self
                .router
                .register_conversation_binding(conversation_binding.clone()),
            _ => {}
        }
    }

    /// `AppWindow::apply_backend_events_inline`: applies events a handler got
    /// back directly, then reprojects.
    pub fn apply_backend_events_inline(&mut self, events: Vec<BackendEvent>) {
        if events.is_empty() {
            return;
        }
        let mut effects = Vec::new();
        for event in events {
            self.register_bindings(&event);
            effects.extend(self.store.dispatch_backend(event));
        }
        self.run_effects(effects);
        self.sync_models();
    }

    /// Projects the store into the models with the shared `StoreSync`.
    pub fn sync_models(&mut self) {
        let hints = ViewHints {
            timeline_near_bottom: self.view.timeline_near_bottom,
            jump_to_message_active: self.timeline.jump_to_message_active,
            first_visible_message_id: self
                .view
                .first_visible_message_id
                .clone()
                .map(MessageId::new),
        };
        let outcome = self.store_sync.sync(
            &mut self.models,
            self.store.snapshot(),
            SyncInputs {
                preview_summaries: &self.preview_summaries,
                og_service: &self.og_service,
                github_pr_service: &self.github_pr_service,
            },
            hints,
        );
        if outcome.conversation_changed {
            self.timeline.jump_to_message_active = false;
        }
        timeline::after_sync(self, &outcome);
        overlays::after_sync(self, &outcome);
        let messages = self.store.snapshot().timeline.messages.clone();
        self.schedule_og_fetches(&messages);
        let replies = self.models.thread_pane.replies.clone();
        self.schedule_og_fetches(&replies);
        self.dirty = true;
    }

    fn schedule_og_fetches(&mut self, messages: &[MessageRecord]) {
        let now = Instant::now();
        for message in messages {
            for url in extract_link_urls_from_fragments(&message.fragments) {
                if self.github_pr_service.handles_url(&url, now) {
                    continue;
                }
                self.og_service.schedule_fetch(&url, &self.og_results.0);
            }
        }
    }

    fn timeline_messages_mut(models: &mut AppModels) -> Vec<&mut MessageRecord> {
        models
            .timeline
            .rows
            .iter_mut()
            .filter_map(|row| match row {
                TimelineRow::Message(row) => Some(&mut row.message),
                _ => None,
            })
            .chain(models.thread_pane.replies.iter_mut())
            .collect()
    }

    /// `drain_og_fetch_results`, `poll_github_prs` and `drain_github_pr_results`.
    fn drain_link_previews(&mut self) {
        let mut og_changed = false;
        while let Ok(result) = self.og_results.1.try_recv() {
            og_changed |= result.preview.is_some();
            self.og_service.apply_result(result);
        }
        if og_changed {
            apply_og_previews(&mut self.models, &self.og_service, &self.github_pr_service);
            self.dirty = true;
        }

        let now = Instant::now();
        let mut pr_changed = false;
        if self
            .github_pr_last_poll
            .is_none_or(|at| now.duration_since(at) >= GITHUB_PR_POLL_INTERVAL)
        {
            self.github_pr_last_poll = Some(now);
            let attention_key = (
                self.models.timeline.conversation_id.clone(),
                self.view.window_focused,
            );
            if self.github_pr_attention_key.as_ref() != Some(&attention_key) {
                if self.view.window_focused {
                    self.github_pr_service.note_attention();
                }
                self.github_pr_attention_key = Some(attention_key);
            }
            let refs = self.visible_github_pr_refs();
            if !refs.is_empty() {
                pr_changed |= self.github_pr_service.poll(
                    &refs,
                    self.view.window_focused,
                    now,
                    &self.github_pr_results.0,
                );
            }
        }
        let mut fallback = HashSet::new();
        while let Ok(result) = self.github_pr_results.1.try_recv() {
            fallback.extend(self.github_pr_service.apply_result(result, now));
            pr_changed = true;
        }
        if !fallback.is_empty() {
            // These PRs now use the ordinary link preview, which was skipped so far.
            let urls = Self::timeline_messages_mut(&mut self.models)
                .into_iter()
                .flat_map(|message| extract_link_urls_from_fragments(&message.fragments))
                .filter(|url| GithubPrRef::parse(url).is_some_and(|pr| fallback.contains(&pr)))
                .collect::<Vec<_>>();
            for url in &urls {
                self.og_service.schedule_fetch(url, &self.og_results.0);
            }
        }
        if pr_changed && apply_github_pr_previews(&mut self.models, &self.github_pr_service, now) {
            self.dirty = true;
        }
    }

    /// PR links in the open conversation and thread, most recent first.
    fn visible_github_pr_refs(&self) -> Vec<GithubPrRef> {
        let timeline = self
            .models
            .timeline
            .rows
            .iter()
            .rev()
            .filter_map(|row| match row {
                TimelineRow::Message(row) => Some(&row.message),
                _ => None,
            });
        let mut seen = HashSet::new();
        let mut refs = Vec::new();
        for message in self.models.thread_pane.replies.iter().rev().chain(timeline) {
            for url in extract_link_urls_from_fragments(&message.fragments) {
                if let Some(pr) = GithubPrRef::parse(&url)
                    && seen.insert(pr.clone())
                {
                    refs.push(pr);
                }
            }
        }
        refs
    }

    /// Mirrors `AppWindow::request_mark_conversation_read_if_needed`.
    pub fn request_mark_conversation_read_if_needed(&mut self) -> bool {
        if !self.view.window_focused {
            return false;
        }
        let has_visible_message_rows = self
            .models
            .timeline
            .rows
            .iter()
            .any(|row| matches!(row, TimelineRow::Message(_)));
        if !self.view.timeline_near_bottom && has_visible_message_rows {
            return false;
        }
        let conversation_id = self.models.timeline.conversation_id.clone();
        if conversation_id.0.is_empty() {
            return false;
        }
        let latest_message_id = self
            .models
            .timeline
            .newer_cursor
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| MessageId::new(value.to_string()))
            .or_else(|| {
                latest_snapshot_message_id(&self.store.snapshot().timeline.messages).or_else(|| {
                    self.models
                        .timeline
                        .rows
                        .iter()
                        .rev()
                        .find_map(|row| match row {
                            TimelineRow::Message(row) => Some(row.message.id.clone()),
                            _ => None,
                        })
                })
            });
        let has_unread_marker = self
            .models
            .timeline
            .unread_marker
            .as_ref()
            .is_some_and(|marker| {
                latest_message_id
                    .as_ref()
                    .is_some_and(|latest| message_id_is_after(latest, marker))
            });
        let current_route = &self.models.navigation.current;
        let has_unread_sidebar_dot = self
            .models
            .sidebar
            .sections
            .iter()
            .flat_map(|section| section.rows.iter())
            .find(|row| &row.route == current_route)
            .is_some_and(|row| row.unread_count > 0 || row.mention_count > 0);
        if !has_unread_marker && !has_unread_sidebar_dot {
            return false;
        }
        let current_message_key = latest_message_id
            .as_ref()
            .map(|value| value.0.clone())
            .unwrap_or_default();
        if let Some((last_message_key, last_attempt_at)) =
            self.last_mark_read_attempt.get(&conversation_id.0)
            && last_message_key == &current_message_key
            && last_attempt_at.elapsed() < MARK_READ_THROTTLE
        {
            return false;
        }
        self.dispatch(UiAction::MarkConversationRead {
            conversation_id: conversation_id.clone(),
            message_id: None,
        });
        self.last_mark_read_attempt.insert(
            conversation_id.0.clone(),
            (current_message_key, Instant::now()),
        );
        true
    }

    /// Emits each model section whose serialized form changed since last time.
    fn publish(&mut self) {
        let models = &self.models;
        let sections: [(&'static str, Result<Value, serde_json::Error>); 22] = [
            ("app", serde_json::to_value(&models.app)),
            ("workspace", serde_json::to_value(&models.workspace)),
            ("navigation", serde_json::to_value(&models.navigation)),
            ("sidebar", serde_json::to_value(&models.sidebar)),
            ("conversation", serde_json::to_value(&models.conversation)),
            ("timeline", serde_json::to_value(&models.timeline)),
            ("composer", serde_json::to_value(&models.composer)),
            (
                "quick_switcher",
                serde_json::to_value(&models.quick_switcher),
            ),
            ("emoji_picker", serde_json::to_value(&models.emoji_picker)),
            ("find_in_chat", serde_json::to_value(&models.find_in_chat)),
            ("search", serde_json::to_value(&models.search)),
            ("new_chat", serde_json::to_value(&models.new_chat)),
            ("thread_pane", serde_json::to_value(&models.thread_pane)),
            ("profile_panel", serde_json::to_value(&models.profile_panel)),
            ("overlay", serde_json::to_value(&models.overlay)),
            ("call", serde_json::to_value(&models.call)),
            ("notifications", serde_json::to_value(&models.notifications)),
            ("settings", serde_json::to_value(&models.settings)),
            ("shell", serde_json::to_value(&self.shell)),
            ("timeline_extra", serde_json::to_value(&self.timeline)),
            ("composer_extra", serde_json::to_value(&self.composer)),
            ("right_pane_extra", serde_json::to_value(&self.right_pane)),
        ];
        let overlays = ("overlays_extra", serde_json::to_value(&self.overlays));
        let sidebar_avatars = (
            "sidebar_avatars",
            serde_json::to_value(&self.store_sync.sidebar_conversation_avatars),
        );
        for (name, value) in sections.into_iter().chain([overlays, sidebar_avatars]) {
            let value = match value {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!("could not serialize model section {name}: {error}");
                    continue;
                }
            };
            if self.published.get(name) == Some(&value) {
                continue;
            }
            if let Err(error) = self.app.emit(&format!("model:{name}"), &value) {
                tracing::warn!("failed to emit model:{name}: {error}");
                continue;
            }
            self.published.insert(name, value);
        }
    }

    pub fn home_route(&self) -> Route {
        Route::WorkspaceHome {
            workspace_id: self.models.app.active_workspace_id.clone(),
        }
    }
}

/// The demo workspace's models, store and backend, when this run shows it.
#[cfg(feature = "demo-fixtures")]
fn demo_parts(
    settings: &crate::models::settings_model::SettingsModel,
    account_id: &AccountId,
    backend_id: &BackendId,
) -> Option<(AppModels, AppStore, BackendRouter)> {
    demo::enabled().then(|| demo::build(settings.clone(), account_id, backend_id))
}

#[cfg(not(feature = "demo-fixtures"))]
fn demo_parts(
    _: &crate::models::settings_model::SettingsModel,
    _: &AccountId,
    _: &BackendId,
) -> Option<(AppModels, AppStore, BackendRouter)> {
    None
}

/// Keybase message ids are numeric; compare numerically when both are.
pub fn message_id_is_after(candidate: &MessageId, baseline: &MessageId) -> bool {
    match (
        candidate.0.parse::<u64>().ok(),
        baseline.0.parse::<u64>().ok(),
    ) {
        (Some(candidate), Some(baseline)) => candidate > baseline,
        _ => candidate.0 > baseline.0,
    }
}

fn latest_snapshot_message_id(messages: &[MessageRecord]) -> Option<MessageId> {
    let mut latest: Option<MessageId> = None;
    for message in messages {
        if latest
            .as_ref()
            .is_none_or(|current| message_id_is_after(&message.id, current))
        {
            latest = Some(message.id.clone());
        }
    }
    latest
}
