//! Members pane: a filter field, then owners and admins, members and bots,
//! each person with presence, display name, username and role.
use super::{conversation_location, member_avatar, pane_header, pane_with_header};
use crate::views::typography;
use crate::{
    domain::channel_details::ChannelMemberPreview,
    models::conversation_model::ConversationModel,
    views::{
        app_window::AppWindow, empty_state, input::TextField, search_field_shell, section_label,
        text_primary, text_secondary, tint,
    },
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, CursorStyle, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, div, px, rgb,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MemberSection {
    Admins,
    Members,
    Bots,
}

impl MemberSection {
    fn title(self) -> &'static str {
        match self {
            Self::Admins => "Owners & admins",
            Self::Members => "Members",
            Self::Bots => "Bots",
        }
    }
}

/// Members matching `query` (display name or username, any case), split into
/// sections in display order. Owners lead the admins; everyone else is sorted
/// by name. Empty sections are left out.
pub(super) fn member_sections<'a>(
    members: &'a [ChannelMemberPreview],
    query: &str,
) -> Vec<(MemberSection, Vec<&'a ChannelMemberPreview>)> {
    let query = query.trim().to_lowercase();
    let mut sections: Vec<(MemberSection, Vec<&ChannelMemberPreview>)> = vec![
        (MemberSection::Admins, Vec::new()),
        (MemberSection::Members, Vec::new()),
        (MemberSection::Bots, Vec::new()),
    ];
    for member in members.iter().filter(|member| {
        query.is_empty()
            || member.display_name.to_lowercase().contains(&query)
            || member.user_id.0.to_lowercase().contains(&query)
    }) {
        let section = if member.is_bot {
            2
        } else if member.is_team_admin_or_owner {
            0
        } else {
            1
        };
        sections[section].1.push(member);
    }
    for (_, people) in &mut sections {
        people.sort_by_cached_key(|member| {
            (
                !(member.is_team_admin_or_owner && member.is_team_owner),
                member.shown_name().to_lowercase(),
            )
        });
    }
    sections.retain(|(_, people)| !people.is_empty());
    sections
}

/// "No one in #general matches “quinn”." Outside a channel, "this conversation".
fn no_member_matches_title(channel: Option<&str>, query: &str) -> String {
    format!(
        "No one in {} matches “{}”.",
        channel.unwrap_or("this conversation"),
        query.trim()
    )
}

pub(super) fn render_members_panel(
    conversation: &ConversationModel,
    filter_input: &Entity<TextField>,
    filter_focused: bool,
    cx: &mut Context<AppWindow>,
) -> AnyElement {
    let details = conversation.details.as_ref();
    let members = details
        .map(|details| details.members.as_slice())
        .unwrap_or(&[]);
    let total = details
        .map(|details| details.member_count as usize)
        .filter(|count| *count > 0)
        .unwrap_or(members.len());
    let query = filter_input.read(cx).text();
    let sections = member_sections(members, &query);
    let subtitle = match (conversation_location(conversation), total) {
        (Some(location), 0) => Some(location),
        (Some(location), total) => Some(format!("{location} · {}", people_count(total))),
        (None, 0) => None,
        (None, total) => Some(people_count(total)),
    };

    let list = if details.is_none() {
        empty_state("assets/icons/members.svg", "Loading members…", None).into_any_element()
    } else if members.is_empty() {
        empty_state(
            "assets/icons/members.svg",
            "No members to show",
            Some("Keybase hasn't shared this conversation's member list yet.".to_string()),
        )
        .into_any_element()
    } else if sections.is_empty() {
        let input = filter_input.clone();
        // The filter only narrows this list; nothing here adds people, so the
        // copy says where it looked instead of suggesting another search.
        empty_state(
            "assets/icons/search.svg",
            no_member_matches_title(
                (conversation.summary.kind
                    == crate::domain::conversation::ConversationKind::Channel)
                    .then(|| conversation_location(conversation))
                    .flatten()
                    .as_deref(),
                &query,
            ),
            None,
        )
        .child(div().pt(px(12.)).child(
            crate::views::secondary_button("members-filter-clear", "Clear filter").on_click(
                move |_, window, cx| {
                    input.update(cx, |input, cx| input.set_text("", cx));
                    window.focus(&gpui_kit::Focusable::focus_handle(&input, cx), cx);
                },
            ),
        ))
        .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(sections.into_iter().map(|(section, people)| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        section_label(format!("{} · {}", section.title(), people.len()))
                            .px(px(8.))
                            .pb(px(4.)),
                    )
                    .children(people.into_iter().map(|member| member_row(member, cx)))
                    .into_any_element()
            }))
            .into_any_element()
    };

    let body = div()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .child(
            div()
                .flex_shrink_0()
                .px(px(16.))
                .pt(px(12.))
                .pb(px(8.))
                .child(
                    search_field_shell(
                        "members-filter-input",
                        filter_input.clone().into_any_element(),
                        filter_focused,
                        32.,
                    )
                    .on_click(cx.listener(AppWindow::focus_members_filter_input)),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .id("right-pane-members-scroll")
                .overflow_y_scroll()
                .scrollbar_width(px(8.))
                .px(px(8.))
                .pt(px(4.))
                .pb(px(16.))
                .child(list),
        )
        .into_any_element();
    pane_with_header(
        pane_header("Members", subtitle, "members-pane-close", cx),
        body,
    )
}

fn people_count(count: usize) -> String {
    format!("{count} {}", if count == 1 { "member" } else { "members" })
}

/// 40px row: avatar with presence, the shown name and then the other one,
/// identity marker, role.
fn member_row(member: &ChannelMemberPreview, cx: &mut Context<AppWindow>) -> AnyElement {
    let user_id = member.user_id.clone();
    let is_bot = member.is_bot;
    let username = member.user_id.0.as_str();
    let other_name = member.other_name();
    let role = if member.is_bot {
        Some("Bot")
    } else {
        member
            .is_team_admin_or_owner
            .then_some(if member.is_team_owner {
                "Owner"
            } else {
                "Admin"
            })
    };
    div()
        .id(SharedString::from(format!("members-pane-row-{username}")))
        .h(px(40.))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(tint(text_primary(), 0.05)))
        .on_click(cx.listener(move |this, _, window, cx| {
            if is_bot {
                this.open_bots(Some(user_id.0.clone()), window, cx);
            } else {
                this.open_user_profile_card(user_id.clone(), cx);
            }
        }))
        .child(member_avatar(
            "members-pane",
            member,
            28.,
            crate::views::right_pane_surface(),
        ))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(180.))
                        .truncate()
                        .text_size(typography::label())
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(text_primary()))
                        .child(member.shown_name().to_string()),
                )
                .children(crate::views::identity_marker(
                    SharedString::from(format!("members-pane-identity-{username}")),
                    member.affinity,
                    member.shown_name(),
                    true,
                ))
                .when_some(other_name, |row, other_name| {
                    row.child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(typography::font_px(12.))
                            .text_color(rgb(text_secondary()))
                            .child(other_name),
                    )
                }),
        )
        .when_some(role, |row, role| row.child(role_badge(role)))
        .into_any_element()
}

/// Team role spelled out, so the list needs no legend.
fn role_badge(role: &'static str) -> AnyElement {
    div()
        .flex_shrink_0()
        .min_h(typography::font_px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(tint(text_primary(), 0.07))
        .text_size(typography::font_px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(text_secondary()))
        .child(role)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{affinity::Affinity, ids::UserId};

    #[test]
    fn no_match_copy_names_where_it_looked() {
        assert_eq!(
            no_member_matches_title(Some("#general"), " quinn "),
            "No one in #general matches “quinn”."
        );
        assert_eq!(
            no_member_matches_title(None, "quinn"),
            "No one in this conversation matches “quinn”."
        );
    }

    fn member(username: &str, name: &str, admin: bool, owner: bool) -> ChannelMemberPreview {
        ChannelMemberPreview {
            user_id: UserId::new(username),
            display_name: name.to_string(),
            avatar_asset: None,
            affinity: Affinity::None,
            is_team_admin_or_owner: admin,
            is_team_owner: owner,
            is_bot: username == "buildbot",
            availability: None,
        }
    }

    fn names(
        sections: &[(MemberSection, Vec<&ChannelMemberPreview>)],
    ) -> Vec<(MemberSection, Vec<String>)> {
        sections
            .iter()
            .map(|(section, people)| {
                (
                    *section,
                    people
                        .iter()
                        .map(|member| member.display_name.clone())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn bot_grouping_uses_metadata_instead_of_username_suffix() {
        let human = member("humanbot", "Human", false, false);
        let mut bot = member("meetings", "Meetings", false, false);
        bot.is_bot = true;
        let members = [human, bot];
        let groups = names(&member_sections(&members, ""));
        assert_eq!(groups[0], (MemberSection::Members, vec!["Human".into()]));
        assert_eq!(groups[1], (MemberSection::Bots, vec!["Meetings".into()]));
    }
    #[test]
    fn groups_admins_people_and_bots_with_owners_first() {
        let members = [
            member("zed", "Zed Admin", true, false),
            member("sam", "Sam Rivera", false, false),
            member("buildbot", "Build Bot", false, false),
            member("alicej", "Alice Johnson", true, true),
            member("devon", "Devon Brooks", false, false),
        ];
        assert_eq!(
            names(&member_sections(&members, "")),
            [
                (
                    MemberSection::Admins,
                    vec!["Alice Johnson".to_string(), "Zed Admin".to_string()]
                ),
                (
                    MemberSection::Members,
                    vec!["Devon Brooks".to_string(), "Sam Rivera".to_string()]
                ),
                (MemberSection::Bots, vec!["Build Bot".to_string()]),
            ]
        );
    }

    #[test]
    fn filter_matches_name_or_username_and_drops_empty_sections() {
        let members = [
            member("alicej", "Alice Johnson", true, true),
            member("samr", "Sam Rivera", false, false),
        ];
        assert_eq!(
            names(&member_sections(&members, "  SAMR ")),
            [(MemberSection::Members, vec!["Sam Rivera".to_string()])]
        );
        assert_eq!(
            names(&member_sections(&members, "john")),
            [(MemberSection::Admins, vec!["Alice Johnson".to_string()])]
        );
        assert!(member_sections(&members, "nobody").is_empty());
    }
}
