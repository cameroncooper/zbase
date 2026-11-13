mod featured;
mod modal;
mod panel;
use super::*;
use crate::domain::bots::{BotCommand, BotInfo, BotOperation, BotSettings, ConversationBots};
use crate::models::composer_model::AutocompleteCandidate;
use crate::views::{accent_strong, typography};
use crate::views::{primary_button, secondary_button, section_label};
use gpui_kit::{FontWeight, SharedString, prelude::FluentBuilder};

#[derive(Default)]
pub(super) struct BotUi {
    selected: Option<String>,
    pub(super) form: Option<BotForm>,
    sequence: u64,
    target_thread: bool,
}
pub(super) struct BotForm {
    conversation: ConversationId,
    username: Entity<TextField>,
    triggers: Entity<TextField>,
    settings: BotSettings,
    restricted: bool,
    mode: FormMode,
    featured: Option<crate::domain::bots::FeaturedBot>,
    pending: Option<u64>,
    show_lookup: bool,
    previous_focus: Option<FocusHandle>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FormMode {
    Add,
    Edit,
    Remove,
}

impl AppWindow {
    fn current_bots(&self) -> ConversationBots {
        self.app_store
            .snapshot()
            .backend
            .bots
            .get(&self.models.conversation.summary.id)
            .cloned()
            .unwrap_or_default()
    }
    fn request_bots(&mut self, conversation_id: ConversationId, operation: BotOperation) -> u64 {
        let current = self
            .app_store
            .snapshot()
            .backend
            .bots
            .get(&conversation_id)
            .map_or(0, |s| s.request);
        self.bot_ui.sequence = self
            .bot_ui
            .sequence
            .max(current)
            .max(self.app_store.snapshot().backend.featured_bots.request)
            + 1;
        let request = self.bot_ui.sequence;
        self.dispatch_ui_action(UiAction::Bots {
            conversation_id,
            request,
            operation,
        });
        request
    }
    pub(crate) fn open_bots(
        &mut self,
        username: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_input.focus_handle(cx).is_focused(window) {
            self.bot_ui.target_thread = true;
        } else if self.composer_input.focus_handle(cx).is_focused(window) {
            self.bot_ui.target_thread = false;
        }
        self.bot_ui.selected = username;
        self.models.set_right_pane(RightPaneMode::Bots);
        if !self.current_bots().loading {
            self.request_bots(
                self.models.conversation.summary.id.clone(),
                BotOperation::Load,
            );
        }
        let catalog = &self.app_store.snapshot().backend.featured_bots;
        if catalog.bots.is_empty() && !catalog.loading {
            self.load_featured_bots(cx);
        }
        self.refresh(cx);
    }
    pub(super) fn bot_command_candidates(&self, query: &str) -> Vec<AutocompleteCandidate> {
        let query = query.to_ascii_lowercase();
        let snapshot = self.current_bots().snapshot;
        let conversation = raw_id(&self.models.conversation.summary.id);
        let mut commands: Vec<_> = snapshot
            .commands
            .iter()
            .filter(|c| snapshot.command_enabled(c, &conversation))
            .cloned()
            .collect();
        commands.sort_by(|a, b| a.name.cmp(&b.name).then(a.username.cmp(&b.username)));
        commands
            .into_iter()
            .filter(|c| {
                c.name
                    .trim_start_matches('!')
                    .to_ascii_lowercase()
                    .starts_with(&query)
            })
            .take(INLINE_AUTOCOMPLETE_MAX_RESULTS)
            .map(|c| AutocompleteCandidate::BotCommand {
                text: c.text(),
                username: c.username,
                description: c.description,
            })
            .collect()
    }
    fn insert_bot_command(
        &mut self,
        command: &BotCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Preserve existing text as command arguments rather than sending immediately.
        let input =
            if self.bot_ui.target_thread && self.models.thread_pane.root_message_id.is_some() {
                self.models.set_right_pane(RightPaneMode::Thread);
                self.thread_input.clone()
            } else {
                self.composer_input.clone()
            };
        input.update(cx, |input, cx| {
            input.replace_range(0..0, &format!("{} ", command.text()), cx);
        });
        window.focus(&input.focus_handle(cx), cx);
        self.capture_live_inputs(cx);
        self.refresh(cx);
    }
    fn begin_bot_form(
        &mut self,
        bot: Option<BotInfo>,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conversation = self.models.conversation.summary.id.clone();
        let settings = bot
            .as_ref()
            .and_then(|b| b.settings.clone())
            .unwrap_or_else(|| BotSettings {
                commands: true,
                mentions: true,
                triggers: Vec::new(),
                conversations: vec![raw_id(&conversation)],
            });
        let name = bot.as_ref().map(|b| b.username.clone()).unwrap_or_default();
        let username = cx.new(|cx| TextField::new(cx.focus_handle(), "Keybase username", name));
        let triggers = cx.new(|cx| {
            TextField::new_multiline(
                cx.focus_handle(),
                "One regular expression per line",
                settings.triggers.join("\n"),
                3,
            )
        });
        // Changes to form inputs redraw the parent without doing RPC work per keystroke.
        let subscriptions = vec![
            cx.observe(&username, |_, _, cx| cx.notify()),
            cx.observe(&triggers, |_, _, cx| cx.notify()),
        ];
        let previous_focus = window.focused(cx);
        let focus = cx.focus_handle();
        if remove {
            window.focus(&focus, cx);
        } else if bot.is_some() {
            window.focus(&triggers.focus_handle(cx), cx);
        } else {
            window.focus(&username.focus_handle(cx), cx);
        }
        self.bot_ui.form = Some(BotForm {
            conversation,
            username,
            triggers,
            settings,
            restricted: bot.as_ref().is_none_or(|b| b.restricted),
            mode: if remove {
                FormMode::Remove
            } else if bot.is_some() {
                FormMode::Edit
            } else {
                FormMode::Add
            },
            featured: None,
            pending: None,
            show_lookup: false,
            previous_focus,
            focus,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        });
        cx.notify();
    }
    pub(super) fn close_bot_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = self.bot_ui.form.take()
            && let Some(focus) = form.previous_focus
        {
            window.focus(&focus, cx);
        }
        cx.notify();
    }
    pub(super) fn bot_form_primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.bot_ui.form.as_ref() else {
            return;
        };
        let state = self
            .app_store
            .snapshot()
            .backend
            .bots
            .get(&form.conversation)
            .cloned()
            .unwrap_or_default();
        if state.loading || form.pending.is_some_and(|id| id == state.completed_request) {
            return;
        }
        let username = form
            .username
            .read(cx)
            .text()
            .trim()
            .trim_start_matches('@')
            .to_ascii_lowercase();
        if username.is_empty() {
            return;
        }
        let conversation = form.conversation.clone();
        let mode = form.mode;
        let mut settings = form.settings.clone();
        settings.triggers = form
            .triggers
            .read(cx)
            .text()
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let verified = form.show_lookup
            && state
                .lookup
                .as_ref()
                .is_some_and(|(name, _)| name == &username);
        if mode == FormMode::Add
            && verified
            && state.snapshot.bots.iter().any(|b| b.username == username)
        {
            self.close_bot_form(window, cx);
            self.bot_ui.selected = Some(username);
            self.refresh(cx);
            return;
        }
        let operation = match mode {
            FormMode::Add if !verified => BotOperation::Lookup { username },
            FormMode::Add => BotOperation::Add {
                username,
                settings,
                restricted: form.restricted,
            },
            FormMode::Edit => BotOperation::Save {
                username,
                settings,
                restricted: form.restricted,
            },
            FormMode::Remove => BotOperation::Remove { username },
        };
        let mutation = !matches!(operation, BotOperation::Lookup { .. });
        if mutation && !state.snapshot.can_manage {
            return;
        }
        if let Some(form) = self.bot_ui.form.as_mut() {
            form.show_lookup = true;
        }
        let request = self.request_bots(conversation, operation);
        if mutation && let Some(form) = self.bot_ui.form.as_mut() {
            form.pending = Some(request);
        }
        self.refresh(cx);
    }
}
fn raw_id(id: &ConversationId) -> String {
    id.0.strip_prefix("kb_conv:").unwrap_or(&id.0).to_string()
}
fn note(text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(typography::label())
        .text_color(rgb(text_secondary()))
        .child(text.into())
        .into_any_element()
}

#[cfg(any(test, feature = "demo-fixtures"))]
impl AppWindow {
    pub(super) fn show_demo_bots(
        &mut self,
        scene: crate::app::demo::DemoScene,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::{app::demo::DemoScene, domain::bots::BotResult};
        let conversation_id = self.models.conversation.summary.id.clone();
        let mut snapshot = crate::models::demo::demo_bot_snapshot(&conversation_id);
        let bot = snapshot.bots[0].clone();
        if matches!(
            scene,
            DemoScene::BotsDm | DemoScene::FeaturedZoom | DemoScene::FeaturedZoomReview
        ) {
            snapshot.bots.clear();
            snapshot.commands.clear();
        }
        let request = self.app_store.snapshot().backend.featured_bots.request;
        self.apply_backend_events_inline(vec![BackendEvent::FeaturedBotsResult {
            request,
            offset: 0,
            result: Ok(crate::models::demo::demo_featured_bots()),
        }]);
        let request = self.current_bots().request;
        self.apply_backend_events_inline(vec![BackendEvent::BotsResult {
            conversation_id,
            request,
            result: Ok(BotResult::Loaded(snapshot)),
        }]);
        self.models.set_right_pane(RightPaneMode::Bots);
        self.bot_ui.selected = matches!(
            scene,
            DemoScene::BotDetails | DemoScene::BotAccess | DemoScene::BotUnrestricted
        )
        .then(|| "zoombot".into());
        if matches!(
            scene,
            DemoScene::BotAccess | DemoScene::BotUnrestricted | DemoScene::BotRemove
        ) {
            self.begin_bot_form(Some(bot), scene == DemoScene::BotRemove, window, cx);
        }
        if scene == DemoScene::BotUnrestricted
            && let Some(form) = self.bot_ui.form.as_mut()
        {
            form.restricted = false;
        }
        if matches!(scene, DemoScene::BotAdd | DemoScene::BotAddReview) {
            self.begin_bot_form(None, false, window, cx);
            if scene == DemoScene::BotAddReview {
                if let Some(form) = self.bot_ui.form.as_mut() {
                    form.username
                        .update(cx, |input, cx| input.set_text("reminderbot", cx));
                    form.show_lookup = true;
                }
                let request = self.current_bots().request;
                self.apply_backend_events_inline(vec![BackendEvent::BotsResult {
                    conversation_id: self.models.conversation.summary.id.clone(),
                    request,
                    result: Ok(BotResult::Lookup {
                        username: "reminderbot".into(),
                        commands: vec![BotCommand {
                            name: "remind".into(),
                            username: "reminderbot".into(),
                            description: "Set a reminder for your team".into(),
                            usage: "!remind [time] [message]".into(),
                        }],
                    }),
                }]);
            }
        }
        if matches!(
            scene,
            DemoScene::FeaturedZoom | DemoScene::FeaturedZoomReview
        ) {
            let zoom = crate::models::demo::demo_featured_bots().bots[0].clone();
            self.begin_featured_bot(zoom, window, cx);
            if scene == DemoScene::FeaturedZoomReview {
                self.bot_form_primary(window, cx);
            }
        }
        if scene == DemoScene::BotCommands {
            self.models.set_right_pane(RightPaneMode::Hidden);
            self.composer_input
                .update(cx, |input, cx| input.set_text("!", cx));
            window.focus(&self.composer_input.focus_handle(cx), cx);
            self.sync_inline_autocomplete_state(cx);
        }
        self.refresh(cx);
    }
}
