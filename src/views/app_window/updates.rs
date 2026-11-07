use super::*;
use crate::services::updates::{
    self, UpdateAction, UpdateSnapshot,
    session::{self, SavedDraft, UpdateSession},
};
use crate::views::accent;
use gpui::FontWeight;

#[derive(Default)]
pub(super) struct UpdateUi {
    pub snapshot: UpdateSnapshot,
    pub open: bool,
    pub(super) previous_focus: Option<FocusHandle>,
    notes_open: bool,
    note_texts: HashMap<String, Entity<SelectableText>>,
    pub(super) reading_anchor: Option<(MessageId, f32)>,
    last_poll: Option<Instant>,
    reminder: session::Reminder,
    pub resume: Option<UpdateSession>,
    confirmed_account: Option<String>,
    error: Option<String>,
}
impl UpdateUi {
    pub(super) fn new() -> Self {
        Self {
            resume: session::load(&session::path()),
            reminder: session::Reminder::load(),
            ..Default::default()
        }
    }
}
impl AppWindow {
    pub(super) fn remember_update_focus(&mut self, window: &Window, cx: &Context<Self>) {
        if self.update_ui.open && self.update_ui.previous_focus.is_none() {
            self.update_ui.previous_focus = window.focused(cx);
        }
    }
    pub(super) fn close_update_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        restore: bool,
    ) {
        self.update_ui.open = false;
        updates::action(UpdateAction::Later);
        self.update_ui.reminder.postpone(
            &self.update_ui.snapshot.version,
            chrono::Utc::now().timestamp(),
        );
        self.update_ui.reminder.save();
        if let Some(focus) = self.update_ui.previous_focus.take()
            && restore
        {
            window.focus(&focus);
        }
        cx.notify();
    }
    pub(crate) fn check_for_updates_action(
        &mut self,
        _: &commands::CheckForUpdates,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_ui.open = true;
        updates::action(UpdateAction::Check);
        self.update_ui.last_poll = None;
        self.poll_updates(cx);
        self.refresh(cx);
    }
    pub(super) fn poll_updates(&mut self, cx: &mut Context<Self>) {
        if self
            .update_ui
            .last_poll
            .is_some_and(|time| time.elapsed() < Duration::from_millis(500))
        {
            return;
        }
        self.update_ui.last_poll = Some(Instant::now());
        let was_open = self.update_ui.open;
        let mut snapshot = updates::snapshot();
        let attention = snapshot.attention;
        snapshot.attention = false;
        let changed = snapshot != self.update_ui.snapshot;
        if snapshot.version != self.update_ui.snapshot.version {
            self.update_ui.note_texts.clear();
        }
        if attention {
            self.update_ui.open = true;
        }
        // Scheduled reminders never steal the composer focus while typing.
        let now = chrono::Utc::now().timestamp();
        if matches!(
            snapshot.state.as_str(),
            "available" | "downloaded" | "ready"
        ) && self.update_ui.reminder.due(&snapshot.version, now)
            && self
                .last_text_input_activity
                .is_none_or(|time| time.elapsed() > Duration::from_secs(30))
            && !self.splash_open
        {
            self.update_ui.open = true;
            self.update_ui.reminder.postpone(&snapshot.version, now);
            self.update_ui.reminder.save();
        }
        self.update_ui.snapshot = snapshot;
        if changed || attention || was_open != self.update_ui.open {
            self.sync_sidebar_view_state(cx);
            cx.notify();
        }
    }
    pub(crate) fn update_command(&mut self, action: UpdateAction, cx: &mut Context<Self>) {
        self.update_ui.error = None;
        if matches!(action, UpdateAction::Restart) {
            self.sync_drafts_from_inputs(cx);
            if let Some(reason) = self.update_restart_blocker() {
                self.update_ui.error = Some(reason.into());
                cx.notify();
                return;
            }
            if let Err(error) = self.save_update_session() {
                self.update_ui.error = Some(format!(
                    "Couldn’t save your drafts. Please try again. ({error})"
                ));
                cx.notify();
                return;
            }
            let flush = self.window_preferences_writer.flush();
            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let _ = flush.await;
                    let _ = this.update(&mut cx, |this, cx| {
                        this.sync_drafts_from_inputs(cx);
                        if let Some(reason) = this.update_restart_blocker() {
                            this.update_ui.error = Some(reason.into());
                            cx.notify();
                        } else if let Err(error) = this.save_update_session() {
                            this.update_ui.error =
                                Some(format!("Couldn’t save your drafts: {error}"));
                            cx.notify();
                        } else {
                            updates::action(UpdateAction::Restart);
                        }
                    });
                }
            })
            .detach();
            return;
        }
        updates::action(action);
        if matches!(action, UpdateAction::Later) {
            self.update_ui.reminder.postpone(
                &self.update_ui.snapshot.version,
                chrono::Utc::now().timestamp(),
            );
            self.update_ui.reminder.save();
        }
        if matches!(action, UpdateAction::Later | UpdateAction::Skip) {
            self.update_ui.open = false;
        }
        self.update_ui.last_poll = None;
        self.poll_updates(cx);
        cx.notify();
    }
    fn update_restart_blocker(&self) -> Option<&'static str> {
        if self.backend_router.has_inflight_sends() {
            Some("Wait for messages and uploads to finish before restarting.")
        } else {
            self.models.update_restart_blocker()
        }
    }
    fn save_update_session(&self) -> std::io::Result<()> {
        let mut drafts = self
            .models
            .composer_draft_texts()
            .map(|(id, text)| SavedDraft {
                id: id.0.clone(),
                thread: false,
                text: text.into(),
            })
            .collect::<Vec<_>>();
        // The live composer can be newer than its navigation cache after send/cancel.
        drafts.retain(|draft| draft.id != self.models.composer.conversation_id.0);
        drafts.push(SavedDraft {
            id: self.models.composer.conversation_id.0.clone(),
            thread: false,
            text: self.models.composer.draft_text.clone(),
        });
        drafts.extend(self.app_store.snapshot().drafts.iter().filter_map(
            |(key, draft)| match key {
                DraftKey::Thread(id) => Some(SavedDraft {
                    id: id.0.clone(),
                    thread: true,
                    text: draft.text.clone(),
                }),
                _ => None,
            },
        ));
        let route = &self.models.navigation.current;
        let top = self.timeline_list_state.logical_scroll_top();
        let anchor = (!self.timeline_is_near_bottom())
            .then(|| self.first_visible_timeline_message_id())
            .flatten();
        let anchor_offset = anchor
            .as_ref()
            .filter(|id| {
                timeline_row_index_for_message(&self.models.timeline.rows, id) == Some(top.item_ix)
            })
            .map(|_| f32::from(top.offset_in_item))
            .unwrap_or(0.);
        let anchor = anchor.map(|id| id.0);
        let pane = match &self.models.navigation.right_pane {
            RightPaneMode::Hidden => "hidden".into(),
            RightPaneMode::Thread => "thread".into(),
            RightPaneMode::Members => "members".into(),
            RightPaneMode::Details => "details".into(),
            RightPaneMode::Files => "files".into(),
            RightPaneMode::Profile(id) => format!("profile:{}", id.0),
            RightPaneMode::Search => "hidden".into(),
        };
        session::save(
            &UpdateSession {
                account: self
                    .models
                    .app
                    .current_user_id
                    .as_ref()
                    .map(|id| id.0.clone())
                    .unwrap_or_default(),
                version: self.update_ui.snapshot.version.clone(),
                workspace: self.models.app.active_workspace_id.0.clone(),
                conversation: route.conversation_key().map(str::to_string),
                channel: matches!(route, Route::Channel { .. }),
                anchor,
                anchor_offset,
                drafts,
                thread: self
                    .models
                    .thread_pane
                    .root_message_id
                    .as_ref()
                    .map(|id| id.0.clone()),
                pane,
            },
            &session::path(),
        )
    }
    pub(super) fn save_update_session_on_quit(&mut self, cx: &mut Context<Self>) {
        let snapshot = updates::snapshot();
        if matches!(snapshot.state.as_str(), "ready" | "installing") {
            self.update_ui.snapshot = snapshot;
            self.sync_drafts_from_inputs(cx);
            if let Err(error) = self.save_update_session() {
                tracing::warn!("could not save update session on quit: {error}");
            }
        }
    }
    pub(super) fn confirm_update_resume_account(&mut self, event: &BackendEvent) {
        if let BackendEvent::BootstrapLoaded {
            source: crate::state::event::BootstrapSource::Server,
            payload,
            ..
        } = event
        {
            self.update_ui.confirmed_account = payload.account_display_name.clone();
        }
    }
    pub(super) fn restore_update_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(saved) = self.update_ui.resume.as_ref() else {
            return;
        };
        if !saved.belongs_to(
            self.update_ui.confirmed_account.as_deref(),
            self.models
                .app
                .current_user_id
                .as_ref()
                .map(|id| id.0.as_str()),
        ) {
            return;
        }
        if self.models.workspace.channels.is_empty()
            && self.models.workspace.direct_messages.is_empty()
        {
            return;
        }
        let saved = self.update_ui.resume.take().unwrap();
        if let Some(id) = saved.conversation {
            let workspace_id = WorkspaceId::new(saved.workspace);
            let route = if saved.channel {
                Route::Channel {
                    workspace_id,
                    channel_id: crate::domain::ids::ChannelId::new(&id),
                }
            } else {
                Route::DirectMessage {
                    workspace_id,
                    dm_id: crate::domain::ids::DmId::new(&id),
                }
            };
            if let Some(anchor) = saved.anchor {
                let anchor = MessageId::new(anchor);
                self.update_ui.reading_anchor = Some((anchor.clone(), saved.anchor_offset));
                self.navigate_to_message(route, anchor, window, cx);
            } else {
                self.navigate_to(route, window, cx);
            }
        }
        match saved.pane.as_str() {
            "thread" => {
                if let Some(root) = saved.thread {
                    self.open_thread(MessageId::new(root), window, cx);
                }
            }
            "members" => self.toggle_pane(RightPaneMode::Members, window, cx),
            "details" => self.toggle_pane(RightPaneMode::Details, window, cx),
            "files" => self.open_files_pane(cx),
            value if value.starts_with("profile:") => {
                self.open_user_profile_panel(UserId::new(&value[8..]), window, cx)
            }
            _ => {}
        }
        for draft in &saved.drafts {
            if draft.thread {
                if self
                    .models
                    .thread_pane
                    .root_message_id
                    .as_ref()
                    .is_some_and(|id| id.0 == draft.id)
                {
                    self.models.update_thread_reply_draft(draft.text.clone());
                }
                self.dispatch_ui_action(UiAction::UpdateDraft {
                    key: DraftKey::Thread(MessageId::new(&draft.id)),
                    text: draft.text.clone(),
                });
            } else {
                self.models
                    .seed_composer_draft(ConversationId::new(&draft.id), &draft.text);
                if self.models.composer.conversation_id.0 == draft.id {
                    self.models.update_composer_draft_text(draft.text.clone());
                }
                self.dispatch_ui_action(UiAction::UpdateDraft {
                    key: DraftKey::Conversation(ConversationId::new(&draft.id)),
                    text: draft.text.clone(),
                });
            }
        }
        self.sync_inputs_from_models(cx);
        if saved.version == env!("CARGO_PKG_VERSION") {
            self.models
                .push_toast(format!("Updated to zBase {}", saved.version), None);
        }
        let _ = std::fs::remove_file(session::path());
    }
    pub(super) fn render_update_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.update_ui.snapshot.clone();
        let (title, description, primary, action) = match snapshot.state.as_str() {
            "available" => (
                "A new zBase is available",
                "Keep chatting while the update downloads.",
                "Download update",
                Some(UpdateAction::Download),
            ),
            "downloaded" => (
                "Ready to prepare the update",
                "macOS may ask your permission before preparing the update.",
                "Prepare update",
                Some(UpdateAction::Download),
            ),
            "downloading" => (
                "Downloading update",
                "You can keep using zBase.",
                "Cancel download",
                Some(UpdateAction::Cancel),
            ),
            "preparing" => (
                "Preparing update…",
                "macOS may ask permission to update this application.",
                "",
                None,
            ),
            "ready" => (
                "Ready when you are",
                "Restart now, or finish updating when you quit. Your drafts will be restored.",
                "Restart and update",
                Some(UpdateAction::Restart),
            ),
            "installing" => ("Installing update…", "zBase will reopen shortly.", "", None),
            "checking" => (
                "Checking for updates…",
                "",
                "Cancel",
                Some(UpdateAction::Cancel),
            ),
            "current" => (
                "No update available",
                snapshot.message.as_str(),
                "Check again",
                Some(UpdateAction::Check),
            ),
            "error" => (
                "Couldn’t finish the update",
                snapshot.message.as_str(),
                "Try again",
                Some(UpdateAction::Check),
            ),
            "disabled" => ("Updates", snapshot.message.as_str(), "View releases", None),
            _ => (
                "Updates",
                "Check for a new version of zBase.",
                "Check now",
                Some(UpdateAction::Check),
            ),
        };
        let notes_open = self.update_ui.notes_open;
        let mut panel = div()
            .id("software-update-panel")
            .w(px(360.))
            .max_w_full()
            .p_4()
            .rounded_lg()
            .bg(crate::views::content_surface())
            .border_1()
            .border_color(shell_border())
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .text_color(rgb(text_primary()))
            .on_mouse_move(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(text_secondary()))
                    .child(format!(
                        "Installed: {}{}",
                        env!("CARGO_PKG_VERSION"),
                        if snapshot.version.is_empty() {
                            String::new()
                        } else {
                            format!(" · Available: {}", snapshot.version)
                        }
                    )),
            )
            .child(div().child(description.to_string()));
        if snapshot.state == "downloading"
            && let Some(progress) = snapshot.progress()
        {
            panel = panel
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .rounded_full()
                        .bg(crate::views::subtle_surface())
                        .child(div().h_full().w(gpui::relative(progress)).bg(rgb(accent()))),
                )
                .child(format!(
                    "{:.1} / {:.1} MB",
                    snapshot.received as f64 / 1_000_000.,
                    snapshot.total as f64 / 1_000_000.
                ));
        }
        if !snapshot.notes.is_empty() {
            panel = panel.child(
                div()
                    .id("update-notes-toggle")
                    .cursor_pointer()
                    .text_color(rgb(accent()))
                    .child(if notes_open {
                        "▾ What’s new"
                    } else {
                        "▸ What’s new"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_ui.notes_open = !this.update_ui.notes_open;
                        cx.notify();
                    })),
            );
            if notes_open {
                panel = panel.child(
                    div()
                        .id("update-release-notes")
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .child(super::update_notes::render(
                            &snapshot.notes,
                            &mut self.update_ui.note_texts,
                            cx,
                        )),
                );
            }
        }
        if !snapshot.version.is_empty() {
            let url = format!(
                "https://github.com/cameroncooper/zbase/releases/tag/v{}",
                snapshot.version
            );
            panel = panel.child(
                div()
                    .id("update-view-release")
                    .cursor_pointer()
                    .text_color(rgb(accent()))
                    .child("View release on GitHub ↗")
                    .on_click(cx.listener(move |_, _, window, cx| {
                        window.dispatch_action(Box::new(commands::OpenUrl { url: url.clone() }), cx)
                    })),
            );
        }
        if notes_open
            && matches!(snapshot.state.as_str(), "available" | "downloaded")
            && !snapshot.informational
        {
            panel = panel.child(
                div()
                    .id("update-skip-version")
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(text_secondary()))
                    .child("Skip this version")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.update_command(UpdateAction::Skip, cx);
                        this.close_update_panel(window, cx, true);
                    })),
            );
        }
        if let Some(error) = &self.update_ui.error {
            panel = panel.child(
                div()
                    .text_color(rgb(crate::views::danger()))
                    .child(error.clone()),
            );
        }
        let mut actions = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap_2()
            .child(self.update_button(
                "update-later",
                if matches!(
                    snapshot.state.as_str(),
                    "downloading" | "preparing" | "checking" | "installing"
                ) {
                    "Hide"
                } else {
                    "Later"
                },
                UpdateAction::Later,
                false,
                cx,
            ));
        if let Some(action) = action {
            actions = actions.child(self.update_button(
                "update-primary",
                if snapshot.informational {
                    "View release"
                } else {
                    primary
                },
                action,
                true,
                cx,
            ));
        } else if snapshot.state == "disabled" {
            actions = actions.child(
                div()
                    .id("update-releases")
                    .cursor_pointer()
                    .child(primary)
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(
                            Box::new(commands::OpenUrl {
                                url: "https://github.com/cameroncooper/zbase/releases".into(),
                            }),
                            cx,
                        )
                    })),
            );
        }
        let width = self.models.sidebar.width_px;
        div()
            .id("update-click-away")
            .absolute()
            .inset_0()
            .on_click(cx.listener(|this, _, window, cx| this.close_update_panel(window, cx, false)))
            .child(
                div()
                    .absolute()
                    .top(px(48.))
                    .left(px(8.))
                    .max_w(px((width + 160.).max(280.)))
                    .child(panel.child(actions)),
            )
            .into_any_element()
    }
    fn update_button(
        &self,
        id: &'static str,
        label: &str,
        action: UpdateAction,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(if primary {
                rgb(accent()).into()
            } else {
                crate::views::subtle_surface()
            })
            .text_color(rgb(if primary { 0xffffff } else { text_primary() }))
            .child(label.to_string())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.update_command(action, cx);
                if matches!(action, UpdateAction::Later | UpdateAction::Skip) {
                    this.close_update_panel(window, cx, true);
                }
            }))
            .into_any_element()
    }
}
