use super::*;

impl AppWindow {
    pub(in crate::views::app_window) fn render_bot_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.bot_ui.form.as_ref() else {
            return div().into_any_element();
        };
        if form.conversation != self.models.conversation.summary.id {
            self.close_bot_form(window, cx);
            return div().into_any_element();
        }
        let state = self
            .app_store
            .snapshot()
            .backend
            .bots
            .get(&form.conversation)
            .cloned()
            .unwrap_or_default();
        if form.pending.is_some_and(|id| id == state.completed_request) && !state.loading {
            self.close_bot_form(window, cx);
            return div().into_any_element();
        }
        let mode = form.mode;
        let username = form
            .username
            .read(cx)
            .text()
            .trim()
            .trim_start_matches('@')
            .to_ascii_lowercase();
        let lookup = state.lookup.as_ref().filter(|(name, _)| name == &username);
        let review = mode != FormMode::Add || (form.show_lookup && lookup.is_some());
        let title = match mode {
            FormMode::Add if review => "Review bot access",
            FormMode::Add => "Add bot",
            FormMode::Edit => "Edit bot access",
            FormMode::Remove => "Remove bot",
        };
        let destination_snapshot = self.app_store.snapshot();
        let destination = if form.conversation == self.models.conversation.summary.id {
            self.models.conversation.summary.title.clone()
        } else {
            destination_snapshot
                .workspace
                .channels
                .iter()
                .chain(&destination_snapshot.workspace.direct_messages)
                .find(|c| c.id == form.conversation)
                .map(|c| c.title.clone())
                .unwrap_or_else(|| "Selected conversation".into())
        };
        let mut body = div()
            .id("bot-form-body")
            .flex_shrink_0()
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(note(format!("In {destination}")));
        if let Some(featured) = form.featured.as_ref().filter(|_| mode == FormMode::Add) {
            body = body.child(self.render_featured_intro(featured, !review));
        } else if mode == FormMode::Add {
            body = body.child(section_label("Keybase username")).child(
                crate::views::search_field_shell(
                    "bot-username-input",
                    form.username.clone().into_any_element(),
                    form.username.focus_handle(cx).is_focused(window),
                    36.,
                )
                .flex_shrink_0(),
            );
            if !review {
                body = body.child(note(
                    "Find a bot by its Keybase username to preview its commands.",
                ));
            }
        } else {
            body = body.child(
                div()
                    .text_size(typography::label())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(text_primary()))
                    .child(username.clone()),
            );
        }
        if mode == FormMode::Remove {
            let scope = self
                .models
                .conversation
                .summary
                .group
                .as_ref()
                .map(|g| g.display_name.clone());
            let description = if let Some(team) = scope {
                format!(
                    "Remove {username} from {team}? This removes its access to every channel in that team. Existing bot messages will remain."
                )
            } else {
                format!(
                    "Remove {username} from this conversation? Existing bot messages will remain."
                )
            };
            body = body.child(note(description));
        } else if review {
            if let Some((_, commands)) = lookup {
                body = body.child(section_label("Commands"));
                if commands.is_empty() {
                    body = body.child(note("This bot has no advertised commands."));
                }
                for command in commands {
                    body = body.child(note(format!(
                        "{} · {}",
                        command.text(),
                        command.description
                    )));
                }
            }
            let is_channel = self.models.conversation.summary.kind == ConversationKind::Channel;
            let scope_summary = if !is_channel {
                "Allowed in this conversation only.".to_string()
            } else if form.settings.conversations.is_empty() {
                "Allowed in all team channels.".to_string()
            } else if form.settings.conversations == vec![raw_id(&form.conversation)] {
                format!("Allowed in {destination} only.")
            } else {
                format!(
                    "Allowed in {} selected channels.",
                    form.settings.conversations.len()
                )
            };
            body = body
                .child(note(scope_summary))
                .child(section_label("Messages this bot can receive"))
                .child(option(
                    "bot-commands",
                    "Commands",
                    "Messages that invoke this bot’s commands",
                    form.settings.commands,
                    state.loading,
                    cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.bot_ui.form.as_mut() {
                            f.settings.commands = !f.settings.commands;
                        }
                        cx.notify();
                    }),
                ))
                .child(option(
                    "bot-mentions",
                    "Mentions",
                    "Messages that @mention this bot",
                    form.settings.mentions,
                    state.loading,
                    cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.bot_ui.form.as_mut() {
                            f.settings.mentions = !f.settings.mentions;
                        }
                        cx.notify();
                    }),
                ))
                .child(section_label("Message patterns"))
                .child(note("Optional regular expressions, one per line."))
                .child(bot_text_field(
                    "bot-triggers-input",
                    &form.triggers,
                    window,
                    cx,
                ));
            if is_channel {
                body = body.child(section_label("Channels"));
                let all = form.settings.conversations.is_empty();
                body = body.child(option(
                    "bot-all-channels",
                    "All channels",
                    "Allow access across this team",
                    all,
                    state.loading,
                    cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.bot_ui.form.as_mut() {
                            if f.settings.conversations.is_empty() {
                                f.settings.conversations = vec![raw_id(&f.conversation)];
                            } else {
                                f.settings.conversations.clear();
                            }
                        }
                        cx.notify();
                    }),
                ));
                if !all {
                    let snapshot = self.app_store.snapshot();
                    let channels: Vec<_> = snapshot
                        .workspace
                        .channels
                        .iter()
                        .filter(|s| {
                            s.group.as_ref().map(|g| &g.id)
                                == self
                                    .models
                                    .conversation
                                    .summary
                                    .group
                                    .as_ref()
                                    .map(|g| &g.id)
                        })
                        .cloned()
                        .collect();
                    let mut shown = Vec::new();
                    for channel in channels
                        .iter()
                        .chain(std::iter::once(&self.models.conversation.summary))
                    {
                        let id = raw_id(&channel.id);
                        if shown.contains(&id) {
                            continue;
                        }
                        shown.push(id.clone());
                        let selected = form.settings.conversations.contains(&id);
                        body = body.child(option(
                            SharedString::from(format!("bot-scope-{id}")),
                            &channel.title,
                            "",
                            selected,
                            state.loading,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(f) = this.bot_ui.form.as_mut() {
                                    if f.settings.conversations.contains(&id) {
                                        if f.settings.conversations.len() > 1 {
                                            f.settings.conversations.retain(|s| s != &id);
                                        }
                                    } else {
                                        f.settings.conversations.push(id.clone());
                                    }
                                }
                                cx.notify();
                            }),
                        ));
                    }
                    let others = form
                        .settings
                        .conversations
                        .iter()
                        .filter(|id| !shown.contains(id))
                        .count();
                    if others > 0 {
                        body = body.child(note(format!(
                            "Also includes {others} channels outside the loaded list."
                        )));
                    }
                }
            }
        }
        if let Some(error) = state.error {
            body = body.child(
                div()
                    .text_size(typography::label())
                    .text_color(rgb(crate::views::danger()))
                    .child(error),
            );
        }
        let label = if state.loading {
            "Working…"
        } else {
            match mode {
                FormMode::Add if !review && form.featured.is_some() => "Review",
                FormMode::Add if !review => "Find bot",
                FormMode::Add => "Add bot",
                FormMode::Edit => "Save changes",
                FormMode::Remove => "Remove bot",
            }
        };
        let label = if mode == FormMode::Add
            && review
            && !state.loading
            && state.snapshot.bots.iter().any(|b| b.username == username)
        {
            "View bot"
        } else {
            label
        };
        let disabled = state.loading
            || username.is_empty()
            || (mode == FormMode::Add && !state.snapshot.can_manage);
        if mode == FormMode::Add && !state.loading && !state.snapshot.can_manage {
            body = body.child(note(
                "Your role does not allow adding bots to this conversation.",
            ));
        }
        let form_focus = form.username.focus_handle(cx);
        let footer = div()
            .flex_shrink_0()
            .flex()
            .justify_between()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(shell_border())
            .child(
                secondary_button("bot-modal-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.close_bot_form(window, cx))),
            )
            .when(mode == FormMode::Add && review && !state.loading, |row| {
                row.child(
                    secondary_button("bot-modal-back", "Back").on_click(cx.listener(
                        move |this, _, window, cx| {
                            if let Some(form) = this.bot_ui.form.as_mut() {
                                form.show_lookup = false;
                            }
                            if let Some(form) = this.bot_ui.form.as_ref() {
                                if form.featured.is_some() {
                                    window.focus(&form.focus, cx);
                                } else {
                                    window.focus(&form_focus, cx);
                                }
                            }
                            cx.notify();
                        },
                    )),
                )
            })
            .child(
                bot_primary_button(label, mode == FormMode::Remove)
                    .when(disabled, |b| b.opacity(0.5))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            this.bot_form_primary(window, cx);
                        }
                    })),
            );
        modal_frame(
            &form.focus,
            &form.scroll,
            title,
            body.into_any_element(),
            footer.into_any_element(),
            window.viewport_size().height,
        )
    }
}

fn modal_frame(
    focus: &FocusHandle,
    scroll: &ScrollHandle,
    title: &str,
    body: AnyElement,
    footer: AnyElement,
    viewport_height: gpui_kit::Pixels,
) -> AnyElement {
    let top = px(crate::views::overlays::OVERLAY_TOP_PX).min(viewport_height / 4.);
    let max_height = (viewport_height - top - px(16.)).max(px(0.)).min(px(620.));
    div()
        .absolute()
        .inset_0()
        .bg(tint(0x000000, 0.45))
        .flex()
        .justify_center()
        .items_start()
        .id("bot-modal-backdrop")
        .occlude()
        .on_scroll(|_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child(
            div()
                .id("bot-modal")
                .track_focus(focus)
                .mt(top)
                .w(px(480.))
                .max_h(max_height)
                .rounded_xl()
                .border_1()
                .border_color(shell_border_strong())
                .bg(crate::views::modal_surface())
                .shadow(crate::views::card_shadow())
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex_shrink_0()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(shell_border())
                        .text_size(typography::label())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(text_primary()))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .id("bot-form-scroll")
                        .min_h(px(0.))
                        .flex_1()
                        .overflow_y_scroll()
                        .scrollbar_width(px(8.))
                        .track_scroll(scroll)
                        .child(body),
                )
                .child(footer),
        )
        .into_any_element()
}

fn option(
    id: impl Into<gpui_kit::ElementId>,
    label: &str,
    description: &str,
    checked: bool,
    disabled: bool,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .rounded_md()
        .px_2()
        .py_2()
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .when(disabled, |s| s.opacity(0.5))
        .hover(|s| s.bg(tint(text_primary(), 0.05)))
        .on_click(move |event, window, cx| {
            if !disabled {
                on_click(event, window, cx);
            }
        })
        .child(
            div()
                .w(px(32.))
                .h(px(18.))
                .p(px(2.))
                .rounded_full()
                .flex()
                .items_center()
                .when(checked, |s| s.justify_end().bg(rgb(accent_strong())))
                .when(!checked, |s| s.bg(tint(text_primary(), 0.20)))
                .child(div().size(px(14.)).rounded_full().bg(rgb(0xffffff))),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(typography::label())
                        .text_color(rgb(text_primary()))
                        .child(label.to_string()),
                )
                .when(!description.is_empty(), |row| {
                    row.child(note(description.to_string()))
                }),
        )
        .into_any_element()
}

fn bot_text_field(
    id: &'static str,
    input: &Entity<TextField>,
    window: &Window,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let focus = input.focus_handle(cx);
    let focused = focus.is_focused(window);
    div()
        .id(id)
        .w_full()
        .h(px(78.))
        .flex_shrink_0()
        .px_2()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(if focused {
            crate::views::accent()
        } else {
            crate::views::field_border()
        }))
        .bg(rgb(crate::views::field_bg()))
        .text_size(typography::label())
        .line_height(typography::font_px(18.))
        .text_color(rgb(text_primary()))
        .when(focused, |field| {
            field.shadow(vec![crate::views::focus_ring()])
        })
        .on_click(move |_, window, cx| window.focus(&focus, cx))
        .child(input.clone())
        .into_any_element()
}

fn bot_primary_button(label: &'static str, destructive: bool) -> gpui_kit::Stateful<gpui_kit::Div> {
    if !destructive {
        return primary_button("bot-modal-primary", label);
    }
    div()
        .id("bot-modal-primary")
        .h(px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(rgb(crate::views::danger()))
        .hover(|s| s.opacity(0.85))
        .cursor_pointer()
        .text_size(typography::label())
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(0xffffff))
        .child(label)
}

#[cfg(test)]
mod scroll_tests;
