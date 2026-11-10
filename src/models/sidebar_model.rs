use crate::domain::{conversation::ConversationGroup, ids::SidebarSectionId, route::Route};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SidebarRow {
    pub label: String,
    pub team: Option<ConversationGroup>,
    pub unread_count: u32,
    pub mention_count: u32,
    pub route: Route,
    /// Newest message, for the Previews style.
    pub preview: Option<super::sidebar_preview::SidebarPreview>,
    /// Notifications are off: the row is drawn dimmed and plain unread stays quiet.
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SidebarSection {
    pub id: SidebarSectionId,
    pub title: String,
    pub rows: Vec<SidebarRow>,
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SidebarModel {
    pub sections: Vec<SidebarSection>,
    pub recent_rows: Vec<super::recent_sidebar::RecentSidebarRow>,
    pub mode: super::settings_model::SidebarMode,
    pub style: super::settings_model::SidebarStyle,
    pub filter: String,
    pub highlighted_route: Option<Route>,
    pub width_px: f32,
    /// What is happening in a conversation right now (typing, an unsent draft, a
    /// failed send), by conversation id. Previews show it instead of the newest message.
    pub live_previews: HashMap<String, super::sidebar_preview::PreviewLive>,
}

/// Conversation rows may represent people or a simple team's general channel.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum SidebarAvatar {
    Participants(Vec<super::conversation_model::AvatarParticipant>),
    Team(String),
}

impl SidebarAvatar {
    pub fn participants(&self) -> &[super::conversation_model::AvatarParticipant] {
        match self {
            Self::Participants(people) => people,
            Self::Team(_) => &[],
        }
    }
}

impl SidebarSection {
    /// Shortcuts to conversations with unread activity, listed above the full list.
    pub fn is_unread(&self) -> bool {
        self.id.0 == "unread"
    }
}

impl SidebarRow {
    pub fn full_label(&self) -> String {
        self.team.as_ref().map_or_else(
            || self.label.clone(),
            |team| format!("{} #{}", team.display_name, self.label),
        )
    }
}

/// Separate from conversation-route keys: complex teams use this image in group headers.
pub fn team_avatar_key(team_id: &str) -> String {
    format!("sidebar-team:{team_id}")
}

impl SidebarModel {
    pub fn visible_recent_rows(
        &self,
    ) -> impl Iterator<Item = &super::recent_sidebar::RecentSidebarRow> {
        let filter = self.filter.trim().to_lowercase();
        self.recent_rows.iter().filter(move |recent| {
            filter.is_empty()
                || recent.row.full_label().to_lowercase().contains(&filter)
                || recent.secondary.to_lowercase().contains(&filter)
                || recent.row.route.label().to_lowercase().contains(&filter)
        })
    }

    /// The section whose row for `route` carries the full selection. A
    /// conversation can be listed twice in Grouped mode: in Unread and in its
    /// team or Conversations. The canonical (non-Unread) row is the one selected,
    /// as long as its section is expanded; the Unread copy is selected only when
    /// it is the sole visible row. `None` when `route` isn't listed.
    pub fn selection_section(&self, route: &Route) -> Option<&SidebarSectionId> {
        let listed = |section: &&SidebarSection| section.rows.iter().any(|row| &row.route == route);
        self.sections
            .iter()
            .filter(|section| !section.is_unread() && !section.collapsed)
            .find(listed)
            .or_else(|| {
                self.sections
                    .iter()
                    .filter(|section| section.is_unread())
                    .find(listed)
            })
            .map(|section| &section.id)
    }

    /// Keep the flat navigation order identical to the grouped visual order. Stable
    /// sorting preserves the reducer's recency order within each team and for DMs.
    pub fn order_unread_groups(&mut self) {
        let team_order = self
            .sections
            .iter()
            .enumerate()
            .filter_map(|(index, section)| {
                section
                    .id
                    .0
                    .strip_prefix("group_")
                    .map(|id| (id.to_string(), index))
            })
            .collect::<HashMap<_, _>>();
        let Some(unread) = self
            .sections
            .iter_mut()
            .find(|section| section.id.0 == "unread")
        else {
            return;
        };
        unread.rows.sort_by_key(|row| match &row.team {
            Some(team) => (
                team_order.get(&team.id).copied().unwrap_or(usize::MAX - 1),
                team.id.clone(),
            ),
            None => (usize::MAX, String::new()),
        });
    }
}

pub const SIDEBAR_WIDTH_PX: f32 = 236.0;
/// Previews put a 32px avatar, a time and a message beside each name; they get
/// more room so common names still fit before anything truncates.
pub const PREVIEWS_SIDEBAR_WIDTH_PX: f32 = 280.0;

/// Default sidebar width for a row style.
pub fn sidebar_width_for_style(style: crate::models::settings_model::SidebarStyle) -> f32 {
    match style {
        crate::models::settings_model::SidebarStyle::Compact => SIDEBAR_WIDTH_PX,
        crate::models::settings_model::SidebarStyle::Previews => PREVIEWS_SIDEBAR_WIDTH_PX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        conversation::ConversationTeamType,
        ids::{ChannelId, WorkspaceId},
    };

    fn row(id: &str, team: Option<&str>) -> SidebarRow {
        SidebarRow {
            label: id.into(),
            team: team.map(|name| ConversationGroup {
                id: name.into(),
                display_name: name.into(),
                team_type: ConversationTeamType::Complex,
            }),
            unread_count: 1,
            mention_count: 0,
            route: Route::Channel {
                workspace_id: WorkspaceId::new("ws"),
                channel_id: ChannelId::new(id),
            },
            preview: None,
            muted: false,
        }
    }

    fn team_section(id: &str) -> SidebarSection {
        SidebarSection {
            id: SidebarSectionId::new(format!("group_{id}")),
            title: id.into(),
            rows: vec![],
            collapsed: true,
        }
    }

    #[test]
    fn unread_groups_follow_saved_team_order_and_preserve_recency_within_groups() {
        let mut retained = row("alpha-retained", Some("alpha"));
        retained.unread_count = 0;
        let mut sidebar = SidebarModel {
            mode: Default::default(),
            style: Default::default(),
            recent_rows: Vec::new(),
            sections: vec![
                SidebarSection {
                    id: SidebarSectionId::new("unread"),
                    title: "Unread".into(),
                    rows: vec![
                        row("dm-new", None),
                        row("alpha-new", Some("alpha")),
                        row("beta", Some("beta")),
                        retained,
                        row("simple-team", None),
                    ],
                    collapsed: false,
                },
                team_section("beta"),
                team_section("alpha"),
            ],
            filter: String::new(),
            highlighted_route: None,
            width_px: 200.,
            live_previews: HashMap::new(),
        };
        sidebar.order_unread_groups();
        assert_eq!(
            sidebar.sections[0]
                .rows
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            [
                "beta",
                "alpha-new",
                "alpha-retained",
                "dm-new",
                "simple-team"
            ]
        );
        assert_eq!(
            sidebar.sections[0].rows[2].unread_count, 0,
            "retaining a row must not restore its unread badge"
        );
        sidebar.sections.swap(1, 2);
        sidebar.order_unread_groups();
        assert_eq!(
            sidebar.sections[0]
                .rows
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            [
                "alpha-new",
                "alpha-retained",
                "beta",
                "dm-new",
                "simple-team"
            ]
        );
        assert!(
            sidebar.sections[1].collapsed,
            "Unread must not depend on normal team expansion"
        );
    }

    #[test]
    fn channel_identity_is_available_without_consuming_its_visible_label() {
        let channel = row("alerts-prod-mainnet", Some("chia_network.ent_wallet"));
        assert_eq!(channel.label, "alerts-prod-mainnet");
        assert_eq!(
            channel.full_label(),
            "chia_network.ent_wallet #alerts-prod-mainnet"
        );
        assert_eq!(row("alice", None).full_label(), "alice");
    }

    #[test]
    fn grouped_mode_selects_one_row_preferring_the_canonical_section() {
        let section = |id: &str, rows: Vec<SidebarRow>, collapsed: bool| SidebarSection {
            id: SidebarSectionId::new(id),
            title: id.into(),
            rows,
            collapsed,
        };
        let general = row("general", Some("lighthouse"));
        let mut sidebar = SidebarModel {
            mode: Default::default(),
            style: Default::default(),
            recent_rows: Vec::new(),
            sections: vec![
                section("unread", vec![general.clone()], false),
                section(
                    "group_lighthouse",
                    vec![general.clone(), row("random", Some("lighthouse"))],
                    false,
                ),
            ],
            filter: String::new(),
            highlighted_route: None,
            width_px: 236.,
            live_previews: HashMap::new(),
        };
        let selected = |sidebar: &SidebarModel| {
            sidebar
                .selection_section(&general.route)
                .map(|id| id.0.clone())
        };
        // Listed twice: the team's row is the selection, the Unread copy is not.
        assert_eq!(selected(&sidebar).as_deref(), Some("group_lighthouse"));
        // With the team collapsed, the Unread copy is the only visible row.
        sidebar.sections[1].collapsed = true;
        assert_eq!(selected(&sidebar).as_deref(), Some("unread"));
        // Once read and left, it leaves Unread; nothing visible lists it.
        sidebar.sections[0].rows.clear();
        assert_eq!(selected(&sidebar), None);
        sidebar.sections[1].collapsed = false;
        assert_eq!(selected(&sidebar).as_deref(), Some("group_lighthouse"));
        assert_eq!(sidebar.selection_section(&row("other", None).route), None);
    }
}
