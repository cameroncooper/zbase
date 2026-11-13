use super::*;

impl AppWindow {
    pub(super) fn bot_avatar(&self, username: &str, size: f32) -> AnyElement {
        let snapshot = self.app_store.snapshot();
        let profile = snapshot.backend.user_profiles.get(&UserId::new(username));
        if username == "zoombot" && profile.and_then(|p| p.avatar_asset.as_ref()).is_none() {
            return div()
                .size(px(size))
                .flex_shrink_0()
                .rounded_full()
                .bg(rgb(0x2d8cff))
                .flex()
                .items_center()
                .justify_center()
                .child(crate::views::icon_asset(
                    "assets/icons/video.svg",
                    size * 0.55,
                    0xffffff,
                ))
                .into_any_element();
        }
        crate::views::avatar::Avatar::render(
            username,
            profile.and_then(|p| p.avatar_asset.as_deref()),
            size,
            crate::views::avatar::default_avatar_background(username),
            text_primary(),
        )
    }
    pub(in crate::views::app_window) fn render_bots_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.current_bots();
        let selected = self
            .bot_ui
            .selected
            .as_ref()
            .and_then(|name| state.snapshot.bots.iter().find(|bot| &bot.username == name))
            .cloned();
        let location = match self.models.conversation.summary.kind {
            ConversationKind::Channel => format!("#{}", self.models.conversation.summary.title),
            _ => self.models.conversation.summary.title.clone(),
        };
        let detail = selected.is_some();
        let content = if let Some(bot) = selected {
            self.render_bot_detail(bot, &state, cx)
        } else {
            let mut list = div().flex().flex_col().gap(px(12.));
            if let Some(error) = &state.error {
                list = list.child(
                    div()
                        .px_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(note(error.clone()))
                        .child(secondary_button("bots-retry", "Retry").on_click(
                            cx.listener(|this, _, window, cx| this.open_bots(None, window, cx)),
                        )),
                );
            }
            if state.loading {
                list = list.child(div().px_2().child(note("Loading bots…")));
            }
            let mut installed = div().flex().flex_col().child(
                section_label(format!("Installed · {}", state.snapshot.bots.len()))
                    .px_2()
                    .pb_1(),
            );
            if state.snapshot.bots.is_empty() && !state.loading && state.error.is_none() {
                installed = installed.child(
                    div()
                        .px_2()
                        .py_1()
                        .child(note("No bots installed in this conversation yet.")),
                );
            }
            for bot in &state.snapshot.bots {
                let username = bot.username.clone();
                let identity = div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(typography::label())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(text_primary()))
                            .truncate()
                            .child(username.clone()),
                    )
                    .child(
                        div()
                            .text_size(typography::caption())
                            .text_color(rgb(text_secondary()))
                            .line_clamp(2)
                            .child(bot.access_summary()),
                    );
                installed = installed.child(
                    bot_list_row(
                        SharedString::from(format!("installed-bot-{username}")),
                        self.bot_avatar(&username, 28.),
                        identity.into_any_element(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.bot_ui.selected = Some(username.clone());
                        cx.notify();
                    })),
                );
            }
            list = list.child(installed).child(self.render_featured_bots(cx));
            let mut body = div().flex_1().min_h(px(0.)).flex().flex_col();
            if state.snapshot.can_manage && !state.loading {
                body = body.child(
                    div().px_4().pt_3().pb_2().flex_shrink_0().child(
                        secondary_button("bots-add", "Add bot")
                            .w_full()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.begin_bot_form(None, false, window, cx)
                            })),
                    ),
                );
            }
            body.child(
                div()
                    .id("bots-pane-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .scrollbar_width(px(8.))
                    .px_2()
                    .pt_1()
                    .pb_4()
                    .child(list),
            )
            .into_any_element()
        };
        crate::views::right_pane::pane_with_header(
            crate::views::right_pane::pane_header(
                if detail { "Bot details" } else { "Bots" },
                Some(location),
                "bots-pane-close",
                cx,
            ),
            content,
        )
    }
    fn render_bot_detail(
        &mut self,
        bot: BotInfo,
        state: &ConversationBots,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let username = bot.username.clone();
        let mut body = div()
            .id("bot-detail-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .scrollbar_width(px(8.))
            .pb_4()
            .flex()
            .flex_col()
            .child(div().px_4().pt_3().pb_2().flex().child(
                secondary_button("bots-back", "All bots").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.bot_ui.selected = None;
                        cx.notify();
                    },
                )),
            ))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(self.bot_avatar(&username, 56.))
                    .child(
                        div()
                            .text_size(typography::heading())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(text_primary()))
                            .child(username.clone()),
                    )
                    .child(crate::views::badge("Bot", panel_alt_bg(), text_secondary())),
            );
        if let Some(error) = &state.error {
            body =
                body.child(
                    div()
                        .px_4()
                        .pb_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(note(error.clone()))
                        .child(secondary_button("bot-detail-retry", "Retry").on_click(
                            cx.listener(|this, _, window, cx| {
                                let selected = this.bot_ui.selected.clone();
                                this.open_bots(selected, window, cx);
                            }),
                        )),
                );
        }
        let commands: Vec<_> = state
            .snapshot
            .commands
            .iter()
            .filter(|command| command.username == username)
            .cloned()
            .collect();
        let mut command_list = div().flex().flex_col().gap_1();
        if commands.is_empty() {
            command_list = command_list.child(note("No commands are available here."));
        }
        for (index, command) in commands.into_iter().enumerate() {
            let enabled = state
                .snapshot
                .command_enabled(&command, &raw_id(&self.models.conversation.summary.id));
            command_list = command_list.child(command_row(index, command, enabled, cx));
        }
        body = body.child(bot_section("Commands", command_list.into_any_element()));
        let mut access = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(note(bot.access_summary()));
        if let Some(settings) = &bot.settings {
            let scope = if self.models.conversation.summary.kind != ConversationKind::Channel
                || settings.conversations == vec![raw_id(&self.models.conversation.summary.id)]
            {
                "This conversation only".into()
            } else if settings.conversations.is_empty() {
                "All channels".to_string()
            } else {
                format!("{} selected channels", settings.conversations.len())
            };
            access = access.child(note(scope));
            if !settings.triggers.is_empty() {
                access = access.child(note(format!(
                    "Message patterns: {}",
                    settings.triggers.join(", ")
                )));
            }
        }
        body = body.child(bot_section("Access", access.into_any_element()));
        if state.snapshot.can_manage && !state.loading {
            let mut actions = div().flex().flex_col().gap_2();
            if !bot.restricted || bot.settings.is_some() {
                let edit = bot.clone();
                actions =
                    actions.child(secondary_button("bot-edit-access", "Edit access").on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.begin_bot_form(Some(edit.clone()), false, window, cx)
                        }),
                    ));
            }
            actions = actions.child(
                secondary_button("bot-remove", "Remove bot…")
                    .text_color(rgb(crate::views::danger()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.begin_bot_form(Some(bot.clone()), true, window, cx)
                    })),
            );
            body = body.child(bot_section("Manage", actions.into_any_element()));
        }
        body.into_any_element()
    }
}

// Match the profile pane's section dividers and Members' list spacing.
fn bot_section(title: &str, content: AnyElement) -> AnyElement {
    div()
        .mx_4()
        .py(px(14.))
        .border_t_1()
        .border_color(shell_border())
        .flex()
        .flex_col()
        .gap_2()
        .child(section_label(title.to_string()))
        .child(content)
        .into_any_element()
}
pub(super) fn bot_list_row(
    id: impl Into<gpui_kit::ElementId>,
    avatar: AnyElement,
    identity: AnyElement,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .min_h(typography::font_px(56.))
        .px_2()
        .py_2()
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        .hover(|style| style.bg(tint(text_primary(), 0.05)))
        .child(avatar)
        .child(identity)
}

fn command_row(
    index: usize,
    command: BotCommand,
    enabled: bool,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    div()
        .id(("bot-command", index))
        .rounded_md()
        .px_2()
        .py_2()
        .cursor_pointer()
        .hover(|s| s.bg(tint(text_primary(), 0.05)))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(typography::label())
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(text_primary()))
                .child(command.text()),
        )
        .child(note(command.description.clone()))
        .when(
            !command.usage.is_empty() && command.usage != command.text(),
            |row| row.child(note(command.usage.clone())),
        )
        .when(!enabled, |row| {
            row.opacity(0.55)
                .child(note("Commands are disabled in this conversation."))
        })
        .when(enabled, |row| {
            row.on_click(
                cx.listener(move |this, _, window, cx| {
                    this.insert_bot_command(&command, window, cx)
                }),
            )
        })
        .into_any_element()
}
