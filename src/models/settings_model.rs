#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarMode {
    #[default]
    Recent,
    Grouped,
}

/// How much a sidebar row shows: one line with the name, or two with the
/// newest message under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarStyle {
    #[default]
    Compact,
    Previews,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Density {
    Comfortable,
    Compact,
}

use std::collections::{HashMap, HashSet};

use crate::domain::{channel_details::ConversationNotificationSettings, user::NameStyle};

#[derive(Clone, Debug)]
pub struct SettingsModel {
    pub theme_mode: ThemeMode,
    pub text_size: TextSize,
    pub density: Density,
    pub reduced_motion: bool,
    pub show_right_pane: bool,
    /// The menu bar icon with the unread count and recent conversations;
    /// macOS only, off until turned on in Settings.
    pub show_menu_bar_icon: bool,
    pub sidebar_style: SidebarStyle,
    /// Whether people appear by profile name or by username, everywhere.
    pub name_style: NameStyle,
    pub emoji_skin_tone: Option<String>,
    pub emoji_recents: Vec<String>,
    pub sidebar_modes: HashMap<String, SidebarMode>,
    pub sidebar_section_order: HashMap<String, Vec<String>>,
    pub sidebar_collapsed_sections: HashMap<String, HashSet<String>>,
    /// The share of Grouped mode's list the conversations take, once dragged;
    /// `None` sizes them automatically.
    pub sidebar_conversations_share: Option<f32>,
    pub dismissed_pinned_items: HashMap<String, String>,
    pub quick_switcher_affinity: HashMap<String, (f32, i64)>,
    /// Notification choices for individual conversations, keyed by conversation id.
    pub conversation_notifications: HashMap<String, ConversationNotificationSettings>,
}

impl Default for SettingsModel {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::System,
            text_size: TextSize::default(),
            density: Density::Comfortable,
            reduced_motion: false,
            show_right_pane: true,
            show_menu_bar_icon: false,
            sidebar_style: SidebarStyle::Compact,
            name_style: NameStyle::FullName,
            emoji_skin_tone: None,
            emoji_recents: Vec::new(),
            sidebar_modes: HashMap::new(),
            sidebar_section_order: HashMap::new(),
            sidebar_collapsed_sections: HashMap::new(),
            sidebar_conversations_share: None,
            dismissed_pinned_items: HashMap::new(),
            quick_switcher_affinity: HashMap::new(),
            conversation_notifications: HashMap::new(),
        }
    }
}

/// A global type scale, stored as a percentage in five-percent steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSize(u16);

impl Default for TextSize {
    fn default() -> Self {
        Self::BASE
    }
}

impl TextSize {
    pub const BASE: Self = Self(100);
    pub const MIN_PERCENT: u16 = 100;
    pub const MAX_PERCENT: u16 = 200;
    pub const STEP: u16 = 5;

    pub fn percent(self) -> u16 {
        self.0
    }

    pub fn from_percent(value: u16) -> Option<Self> {
        ((Self::MIN_PERCENT..=Self::MAX_PERCENT).contains(&value)
            && value.is_multiple_of(Self::STEP))
        .then_some(Self(value))
    }

    pub fn smaller(self) -> Self {
        Self(self.0.saturating_sub(Self::STEP).max(Self::MIN_PERCENT))
    }

    pub fn larger(self) -> Self {
        Self((self.0 + Self::STEP).min(Self::MAX_PERCENT))
    }

    pub fn scale(self) -> f32 {
        self.0 as f32 / 100.
    }
}

#[cfg(test)]
mod text_size_tests {
    use super::TextSize;

    #[test]
    fn stepping_is_bounded_and_reversible() {
        let mut size = TextSize::default();
        assert_eq!(size.smaller(), size);
        for percent in (105..=200).step_by(5) {
            let next = size.larger();
            assert_eq!(next.percent(), percent);
            assert_eq!(next.smaller(), size);
            size = next;
        }
        assert_eq!(size.larger(), size);
        assert_eq!(size.scale(), 2.);
    }
}
