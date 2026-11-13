//! Recent is a projection of inbox metadata, with no message-history or media reads.
use super::{
    quick_switcher_route_for_summary,
    sidebar_model::{SidebarRow, SidebarSection},
    sidebar_preview::SidebarPreview,
};
use crate::domain::{
    conversation::{ConversationKind, ConversationSummary},
    ids::WorkspaceId,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentSidebarRow {
    pub row: SidebarRow,
    pub secondary: String,
    pub is_channel: bool,
}

/// Choose a suffix unique across the entire inbox, so scrolling doesn't rename teams.
fn short_team_names(channels: &[ConversationSummary]) -> HashMap<String, String> {
    let teams = channels
        .iter()
        .filter_map(|summary| summary.group.as_ref())
        .map(|group| (group.id.clone(), group.display_name.clone()))
        .collect::<HashMap<_, _>>();
    let mut suffix_counts = HashMap::<String, usize>::new();
    for name in teams.values() {
        let parts = name.split('.').collect::<Vec<_>>();
        for start in 0..parts.len() {
            *suffix_counts
                .entry(parts[start..].join(".").to_ascii_lowercase())
                .or_default() += 1;
        }
    }
    teams
        .iter()
        .map(|(id, name)| {
            let parts = name.split('.').collect::<Vec<_>>();
            let short = (1..=parts.len())
                .map(|count| parts[parts.len() - count..].join("."))
                .find(|candidate| suffix_counts.get(&candidate.to_ascii_lowercase()) == Some(&1))
                .unwrap_or_else(|| name.clone());
            (id.clone(), short)
        })
        .collect()
}

impl RecentSidebarRow {
    /// Team name to print after a channel: only when another listed channel has
    /// the same name. Otherwise the team mark in front of the row identifies it.
    pub fn team_label<'a>(&'a self, ambiguous: &HashSet<String>) -> Option<&'a str> {
        (self.is_channel
            && !self.secondary.is_empty()
            && ambiguous.contains(&self.row.label.to_lowercase()))
        .then_some(self.secondary.as_str())
    }
}

/// Lowercased channel names shared by more than one of `rows` (the rows on screen,
/// after filtering), e.g. `general` when two teams each list a #general.
pub fn ambiguous_channel_labels<'a>(
    rows: impl IntoIterator<Item = &'a RecentSidebarRow>,
) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut ambiguous = HashSet::new();
    for recent in rows.into_iter().filter(|recent| recent.is_channel) {
        let label = recent.row.label.to_lowercase();
        if !seen.insert(label.clone()) {
            ambiguous.insert(label);
        }
    }
    ambiguous
}

pub fn build_recent_rows(
    channels: &[ConversationSummary],
    direct_messages: &[ConversationSummary],
    sections: &[SidebarSection],
    workspace_id: &WorkspaceId,
    previews: &HashMap<String, SidebarPreview>,
) -> Vec<RecentSidebarRow> {
    let labels = sections
        .iter()
        .flat_map(|section| &section.rows)
        .map(|row| (row.route.label(), row.label.clone()))
        .collect::<HashMap<_, _>>();
    let team_names = short_team_names(channels);
    let mut seen = HashSet::new();
    let mut summaries = channels
        .iter()
        .chain(direct_messages)
        .filter(|summary| seen.insert(&summary.id))
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.compare_recency(right));
    summaries
        .into_iter()
        .map(|summary| {
            let route = quick_switcher_route_for_summary(summary, workspace_id);
            let is_channel = summary.kind == ConversationKind::Channel && !summary.is_simple_team();
            let label = labels.get(&route.label()).cloned().unwrap_or_else(|| {
                summary
                    .simple_team_label()
                    .unwrap_or_else(|| summary.title.clone())
            });
            let secondary = if is_channel {
                summary
                    .group
                    .as_ref()
                    .and_then(|group| team_names.get(&group.id))
                    .cloned()
                    .unwrap_or_default()
            } else if summary.is_simple_team() {
                String::new()
            } else {
                summary
                    .title
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(|name| format!("@{name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            RecentSidebarRow {
                row: SidebarRow {
                    label,
                    team: is_channel.then(|| summary.group.clone()).flatten(),
                    unread_count: summary.unread_count,
                    mention_count: summary.mention_count,
                    route,
                    preview: previews.get(&summary.id.0).cloned(),
                    muted: summary.muted,
                },
                secondary,
                is_channel,
            }
        })
        .collect()
}

/// The menu bar panel's conversations: the most recent ones, unread first,
/// each group keeping Recent's order.
pub fn menubar_rows(rows: &[RecentSidebarRow], limit: usize) -> Vec<RecentSidebarRow> {
    let (unread, read): (Vec<_>, Vec<_>) = rows.iter().partition(|recent| recent.row.is_unread());
    unread
        .into_iter()
        .chain(read)
        .take(limit)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menubar_lists_unread_first_then_recent_up_to_the_limit() {
        let row = |label: &str, unread_count: u32, muted: bool| RecentSidebarRow {
            row: SidebarRow {
                label: label.into(),
                team: None,
                unread_count,
                mention_count: 0,
                route: crate::domain::route::Route::Channel {
                    workspace_id: WorkspaceId::new("ws"),
                    channel_id: crate::domain::ids::ChannelId::new(label),
                },
                preview: None,
                muted,
            },
            secondary: String::new(),
            is_channel: true,
        };
        // Recent's order: newest activity first.
        let rows = [
            row("newest", 0, false),
            row("unread", 2, false),
            row("muted", 5, true),
            row("older", 0, false),
            row("older-unread", 1, false),
            row("oldest", 0, false),
        ];
        let labels = |rows: Vec<RecentSidebarRow>| {
            rows.into_iter()
                .map(|recent| recent.row.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(menubar_rows(&rows, 4)),
            ["unread", "older-unread", "newest", "muted"]
        );
        assert_eq!(labels(menubar_rows(&rows[..1], 4)), ["newest"]);
    }
    use crate::domain::{
        conversation::{ConversationGroup, ConversationTeamType},
        ids::ConversationId,
    };

    fn channel(id: &str, team: &str, time: i64) -> ConversationSummary {
        ConversationSummary {
            id: ConversationId::new(id),
            title: "alerts-prod-mainnet".into(),
            kind: ConversationKind::Channel,
            topic: String::new(),
            group: Some(ConversationGroup {
                id: team.into(),
                display_name: team.into(),
                team_type: ConversationTeamType::Complex,
            }),
            unread_count: 0,
            mention_count: 0,
            muted: false,
            last_activity_ms: time,
            last_message: None,
        }
    }

    #[test]
    fn recent_includes_read_and_muted_chats_once_and_tracks_activity() {
        let mut channels = vec![
            channel("older", "chia_network.ent_wallet", 10),
            channel("newer", "chia_network.ips", 20),
        ];
        channels[0].muted = true;
        channels[1].unread_count = 3;
        let workspace = WorkspaceId::new("ws");
        let rows = build_recent_rows(
            &channels,
            &[channels[0].clone()],
            &[],
            &workspace,
            &HashMap::new(),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].row.route,
            quick_switcher_route_for_summary(&channels[1], &workspace)
        );
        assert_eq!(rows[0].secondary, "ips");
        assert_eq!(rows[1].secondary, "ent_wallet");
        channels[1].unread_count = 0;
        assert_eq!(
            build_recent_rows(&channels, &[], &[], &workspace, &HashMap::new()).len(),
            2
        );
        channels[0].last_activity_ms = 30;
        assert_eq!(
            build_recent_rows(&channels, &[], &[], &workspace, &HashMap::new())[0]
                .row
                .route,
            rows[1].row.route
        );
    }

    #[test]
    fn team_label_shows_only_for_channel_names_listed_twice() {
        let mut general_a = channel("a", "alpha", 10);
        general_a.title = "general".into();
        let mut general_b = channel("b", "beta", 20);
        general_b.title = "General".into();
        let mut random = channel("c", "alpha", 30);
        random.title = "random".into();
        let mut dm = channel("d", "unused", 40);
        dm.kind = ConversationKind::DirectMessage;
        dm.title = "general".into();
        dm.group = None;
        let rows = build_recent_rows(
            &[general_a, general_b, random],
            &[dm],
            &[],
            &WorkspaceId::new("ws"),
            &HashMap::new(),
        );
        let ambiguous = ambiguous_channel_labels(&rows);
        let labels = rows
            .iter()
            .map(|recent| (recent.row.label.as_str(), recent.team_label(&ambiguous)))
            .collect::<Vec<_>>();
        // A DM named like a channel never counts; case doesn't hide a clash.
        assert_eq!(
            labels,
            [
                ("general", None),
                ("random", None),
                ("General", Some("beta")),
                ("general", Some("alpha")),
            ]
        );
        // Filtering down to one #general makes it unique again.
        let visible = rows.iter().filter(|recent| recent.secondary != "beta");
        let ambiguous = ambiguous_channel_labels(visible);
        assert!(
            rows.iter()
                .all(|recent| recent.team_label(&ambiguous).is_none())
        );
    }

    #[test]
    fn team_suffixes_are_unambiguous_and_do_not_depend_on_visible_rows() {
        let channels = vec![
            channel("a", "chia_network.wallet", 10),
            channel("b", "friends.wallet", 20),
            channel("c", "chia_network", 30),
            channel("d", "friends_of_chia", 0),
        ];
        let names = short_team_names(&channels);
        assert_eq!(names["chia_network.wallet"], "chia_network.wallet");
        assert_eq!(names["friends.wallet"], "friends.wallet");
        assert_eq!(names["chia_network"], "chia_network");
        assert_eq!(names["friends_of_chia"], "friends_of_chia");
        let tied = vec![channel("b", "same", 1), channel("a", "same", 1)];
        assert_eq!(
            build_recent_rows(&tied, &[], &[], &WorkspaceId::new("ws"), &HashMap::new())[0]
                .row
                .route,
            quick_switcher_route_for_summary(&tied[1], &WorkspaceId::new("ws"))
        );
    }

    #[test]
    fn sidebar_mode_is_restored_independently_for_each_account() {
        use crate::{
            domain::ids::UserId,
            models::{
                AppModels,
                settings_model::{SettingsModel, SidebarMode},
            },
        };
        let mut models = AppModels::empty_with_settings(SettingsModel::default());
        models.app.current_user_id = Some(UserId::new("alice"));
        assert_eq!(models.sidebar.mode, SidebarMode::Recent);
        models.set_sidebar_mode(SidebarMode::Grouped);
        models.app.current_user_id = Some(UserId::new("bob"));
        models.restore_sidebar_mode();
        assert_eq!(models.sidebar.mode, SidebarMode::Recent);
        models.app.current_user_id = Some(UserId::new("ALICE"));
        models.restore_sidebar_mode();
        assert_eq!(models.sidebar.mode, SidebarMode::Grouped);
    }
    #[test]
    fn recent_navigation_and_filtering_ignore_group_collapse_and_reads_keep_rows() {
        use crate::models::{
            AppModels,
            settings_model::{SettingsModel, SidebarMode},
        };
        let mut models = AppModels::empty_with_settings(SettingsModel::default());
        let mut summary = channel("channel", "chia_network.ent_wallet", 10);
        summary.unread_count = 2;
        models.workspace.channels = vec![summary.clone()];
        models.sidebar.recent_rows = build_recent_rows(
            &models.workspace.channels,
            &[],
            &[],
            &models.app.active_workspace_id,
            &HashMap::new(),
        );
        models.set_sidebar_mode(SidebarMode::Recent);
        let routes = models.visible_sidebar_routes();
        assert_eq!(routes.len(), 1);
        models.sidebar.filter = "ENT_WALLET".into();
        assert_eq!(models.visible_sidebar_routes(), routes);
        models.clear_workspace_counts(&summary.id);
        assert_eq!(models.visible_sidebar_routes(), routes);
        assert_eq!(models.sidebar.recent_rows[0].row.unread_count, 0);
        models.sidebar.filter = "missing-team".into();
        assert!(models.visible_sidebar_routes().is_empty());
        models.set_sidebar_mode(SidebarMode::Grouped);
        assert!(models.visible_sidebar_routes().is_empty());
    }
    #[test]
    fn channels_dms_and_simple_teams_share_one_activity_order() {
        let normal = channel("channel", "chia_network.ent_wallet", 20);
        let mut simple = channel("simple", "example_team", 10);
        simple.title = "general".into();
        simple.group.as_mut().unwrap().team_type = ConversationTeamType::Simple;
        let mut dm = channel("dm", "unused", 30);
        dm.kind = ConversationKind::DirectMessage;
        dm.title = "alice".into();
        dm.group = None;
        let rows = build_recent_rows(
            &[simple, normal],
            &[dm],
            &[],
            &WorkspaceId::new("ws"),
            &HashMap::new(),
        );
        assert_eq!(
            rows.iter()
                .map(|recent| recent.row.label.as_str())
                .collect::<Vec<_>>(),
            ["alice", "alerts-prod-mainnet", "example_team"]
        );
        assert!(!rows[0].is_channel);
        assert!(rows[1].is_channel);
        assert!(!rows[2].is_channel);
        assert_eq!(rows[1].secondary, "ent_wallet");
        assert!(rows[2].row.team.is_none());
    }
}
