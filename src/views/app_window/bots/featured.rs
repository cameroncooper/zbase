use super::*;
use crate::domain::bots::FeaturedBot;

impl AppWindow {
    pub(super) fn load_featured_bots(&mut self, cx: &mut Context<Self>) {
        let catalog = &self.app_store.snapshot().backend.featured_bots;
        if catalog.loading || catalog.all_loaded {
            return;
        }
        let offset = catalog.next_offset;
        self.request_bots(
            self.models.conversation.summary.id.clone(),
            BotOperation::Featured { offset },
        );
        cx.notify();
    }
    pub(super) fn begin_featured_bot(
        &mut self,
        bot: FeaturedBot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Featured bots use the same verified lookup and explicit access review as username installs.
        self.begin_bot_form(None, false, window, cx);
        if let Some(form) = self.bot_ui.form.as_mut() {
            form.username
                .update(cx, |input, cx| input.set_text(bot.username.clone(), cx));
            form.featured = Some(bot);
            window.focus(&form.focus, cx);
        }
        cx.notify();
    }
    pub(super) fn render_featured_intro(&self, bot: &FeaturedBot, expanded: bool) -> AnyElement {
        let mut intro = div().flex().flex_col().gap_3().flex_shrink_0().child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(self.bot_avatar(&bot.username, 56.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(typography::heading())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(text_primary()))
                                .child(bot.name.clone()),
                        )
                        .child(note(format!("@{}", bot.username)))
                        .child(note(bot.description.clone())),
                ),
        );
        if expanded {
            if !bot.extended_description.is_empty() {
                let parsed = crate::views::inline_markdown::apply_inline_markdown(
                    &bot.extended_description,
                    &[],
                    &[],
                    Default::default(),
                );
                intro = intro.child(
                    div()
                        .text_size(typography::label())
                        .line_height(typography::font_px(19.))
                        .text_color(rgb(text_primary()))
                        .child(parsed.text),
                );
            }
            intro = intro.child(section_label("Install as")).child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(shell_border())
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(typography::label())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(text_primary()))
                            .child("Restricted bot (recommended)"),
                    )
                    .child(note(
                        "Choose which messages this bot can receive on the next screen.",
                    )),
            );
        }
        intro.into_any_element()
    }
    pub(super) fn render_featured_bots(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.app_store.snapshot();
        let catalog = snapshot.backend.featured_bots.clone();
        let installed = self.current_bots().snapshot.bots;
        let bots: Vec<_> = catalog
            .bots
            .iter()
            .filter(|bot| !installed.iter().any(|b| b.username == bot.username))
            .cloned()
            .collect();
        let mut section = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(section_label("Featured").px_2().pb_1());
        for bot in bots {
            let owner = if bot.owner.is_empty() {
                String::new()
            } else {
                format!("· by {}", bot.owner)
            };
            let avatar = self.bot_avatar(&bot.username, 28.);
            let identity = div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_1()
                        .child(
                            div()
                                .text_size(typography::label())
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(text_primary()))
                                .truncate()
                                .child(bot.name.clone()),
                        )
                        .when(!owner.is_empty(), |row| {
                            row.child(
                                div()
                                    .text_size(typography::caption())
                                    .text_color(rgb(text_secondary()))
                                    .truncate()
                                    .child(owner),
                            )
                        }),
                )
                .child(
                    div()
                        .text_size(typography::caption())
                        .text_color(rgb(text_secondary()))
                        .line_clamp(2)
                        .child(bot.description.clone()),
                );
            section = section.child(
                super::panel::bot_list_row(
                    SharedString::from(format!("featured-bot-{}", bot.username)),
                    avatar,
                    identity.into_any_element(),
                )
                .child(
                    div()
                        .size(px(28.))
                        .flex_shrink_0()
                        .rounded_md()
                        .border_1()
                        .border_color(shell_border())
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::views::icon_asset(
                            "assets/icons/plus.svg",
                            14.,
                            text_secondary(),
                        )),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_featured_bot(bot.clone(), window, cx)
                })),
            );
        }
        if let Some(error) = catalog.error {
            section = section
                .child(note(format!("Could not load featured bots: {error}")))
                .child(
                    secondary_button("featured-bots-retry", "Retry")
                        .mx_2()
                        .mt_2()
                        .on_click(cx.listener(|this, _, _, cx| this.load_featured_bots(cx))),
                );
        } else if catalog.loading {
            section = section.child(note("Loading featured bots…"));
        } else if !catalog.all_loaded {
            section = section.child(
                secondary_button("featured-bots-more", "Load more")
                    .mx_2()
                    .mt_2()
                    .on_click(cx.listener(|this, _, _, cx| this.load_featured_bots(cx))),
            );
        } else if catalog.bots.is_empty() {
            section = section.child(note("Keybase has no featured bots to show."));
        }
        section.into_any_element()
    }
}
