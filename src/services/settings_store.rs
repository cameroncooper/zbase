use crate::{
    domain::{
        channel_details::{ConversationNotificationSettings, NotificationLevel},
        user::NameStyle,
    },
    models::settings_model::{
        Density, SettingsModel, SidebarMode, SidebarStyle, TextSize, ThemeMode,
    },
};
use std::{
    fs,
    io::{self, ErrorKind},
    path::PathBuf,
};

#[derive(Default)]
pub struct SettingsStore;

impl SettingsStore {
    pub fn load_from_disk() -> io::Result<SettingsModel> {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(raw) => Ok(parse_settings(&raw)),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                // One-time migration from source builds' working-directory storage.
                let legacy = std::env::current_dir()
                    .unwrap_or_default()
                    .join(".zbase/settings.conf");
                let isolated = std::env::var_os("ZBASE_DATA_DIR").is_some();
                if !isolated && let Ok(raw) = fs::read_to_string(legacy) {
                    let settings = parse_settings(&raw);
                    if let Err(error) = Self::save_to_disk(&settings) {
                        tracing::warn!("could not migrate settings: {error}");
                    }
                    return Ok(settings);
                }
                Ok(SettingsModel::default())
            }
            Err(error) => Err(error),
        }
    }

    pub fn save_to_disk(settings: &SettingsModel) -> io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serialize_settings(settings))
    }

    fn path() -> PathBuf {
        super::local_store::paths::data_root().join("settings.conf")
    }
}

fn parse_settings(raw: &str) -> SettingsModel {
    let mut settings = SettingsModel::default();

    for line in raw.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        if let Some(account_id) = key.strip_prefix("sidebar_mode.") {
            let mode = if value == "recent" {
                SidebarMode::Recent
            } else {
                SidebarMode::Grouped
            };
            settings.sidebar_modes.insert(account_id.to_string(), mode);
            continue;
        }
        if let Some(account_id) = key.strip_prefix("sidebar_section_order.") {
            let order = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            settings
                .sidebar_section_order
                .insert(account_id.to_string(), order);
            continue;
        }

        if let Some(account_id) = key.strip_prefix("sidebar_collapsed.") {
            let collapsed = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            settings
                .sidebar_collapsed_sections
                .insert(account_id.to_string(), collapsed);
            continue;
        }

        if let Some(conversation_id) = key.strip_prefix("dismissed_pinned.") {
            if !value.is_empty() {
                settings
                    .dismissed_pinned_items
                    .insert(conversation_id.to_string(), value.to_string());
            }
            continue;
        }

        if let Some(conversation_id) = key.strip_prefix("quick_switcher_affinity.") {
            if conversation_id.is_empty() {
                continue;
            }
            let Some((affinity_raw, last_updated_raw)) = value.split_once(',') else {
                continue;
            };
            let Some(affinity) = affinity_raw
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && *value >= 0.0)
            else {
                continue;
            };
            let Some(last_updated_ms) = last_updated_raw.trim().parse::<i64>().ok() else {
                continue;
            };
            settings
                .quick_switcher_affinity
                .insert(conversation_id.to_string(), (affinity, last_updated_ms));
            continue;
        }

        if let Some(conversation_id) = key.strip_prefix("conversation_notifications.") {
            if conversation_id.is_empty() {
                continue;
            }
            let mut parts = value.split(',');
            let Some(muted) = parts.next().map(str::trim).and_then(parse_bool) else {
                continue;
            };
            let Some(ignore_mentions) = parts.next().map(str::trim).and_then(parse_bool) else {
                continue;
            };
            let Some(desktop) = parts
                .next()
                .map(str::trim)
                .and_then(NotificationLevel::from_str)
            else {
                continue;
            };
            let Some(mobile) = parts
                .next()
                .map(str::trim)
                .and_then(NotificationLevel::from_str)
            else {
                continue;
            };
            if parts.next().is_some() {
                continue;
            }
            settings.conversation_notifications.insert(
                conversation_id.to_string(),
                ConversationNotificationSettings {
                    muted,
                    ignore_mentions,
                    desktop,
                    mobile,
                },
            );
            continue;
        }

        match key {
            "text_size" => {
                settings.text_size = value
                    .parse()
                    .ok()
                    .and_then(TextSize::from_percent)
                    .unwrap_or_default();
            }
            "theme" => {
                settings.theme_mode = match value {
                    "light" => ThemeMode::Light,
                    "dark" => ThemeMode::Dark,
                    _ => ThemeMode::System,
                };
            }
            "density" => {
                settings.density = match value {
                    "compact" => Density::Compact,
                    _ => Density::Comfortable,
                };
            }
            "reduced_motion" => {
                settings.reduced_motion = value == "true";
            }
            "show_right_pane" => {
                settings.show_right_pane = value != "false";
            }
            "show_menu_bar_icon" => {
                settings.show_menu_bar_icon = value == "true";
            }
            "sidebar_style" => {
                settings.sidebar_style = match value {
                    "previews" => SidebarStyle::Previews,
                    _ => SidebarStyle::Compact,
                };
            }
            "sidebar_conversations_share" => {
                settings.sidebar_conversations_share = value
                    .parse::<f32>()
                    .ok()
                    .filter(|share| (0.0..=1.0).contains(share));
            }
            "name_style" => {
                settings.name_style = match value {
                    "username" => NameStyle::Username,
                    _ => NameStyle::FullName,
                };
            }
            "emoji_skin_tone" => {
                settings.emoji_skin_tone = (!value.is_empty()).then(|| value.to_string());
            }
            "emoji_recents" => {
                settings.emoji_recents = value
                    .split(',')
                    .map(|entry| entry.trim().to_ascii_lowercase())
                    .filter(|entry| !entry.is_empty())
                    .collect();
            }
            _ => {}
        }
    }

    settings
}

fn serialize_settings(settings: &SettingsModel) -> String {
    let mut content = format!(
        "theme={}\ndensity={}\nreduced_motion={}\nshow_right_pane={}\nshow_menu_bar_icon={}\nsidebar_style={}\nname_style={}\n",
        theme_label(&settings.theme_mode),
        density_label(&settings.density),
        settings.reduced_motion,
        settings.show_right_pane,
        settings.show_menu_bar_icon,
        match settings.sidebar_style {
            SidebarStyle::Compact => "compact",
            SidebarStyle::Previews => "previews",
        },
        match settings.name_style {
            NameStyle::FullName => "full_name",
            NameStyle::Username => "username",
        }
    );
    content.push_str(&format!("text_size={}\n", settings.text_size.percent()));
    if let Some(share) = settings.sidebar_conversations_share {
        content.push_str(&format!("sidebar_conversations_share={share:.4}\n"));
    }
    if let Some(skin_tone) = settings
        .emoji_skin_tone
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        content.push_str(&format!("emoji_skin_tone={skin_tone}\n"));
    }
    if !settings.emoji_recents.is_empty() {
        content.push_str(&format!(
            "emoji_recents={}\n",
            settings.emoji_recents.join(",")
        ));
    }

    for (account_id, mode) in &settings.sidebar_modes {
        let label = if *mode == SidebarMode::Recent {
            "recent"
        } else {
            "grouped"
        };
        content.push_str(&format!("sidebar_mode.{account_id}={label}\n"));
    }
    for (account_id, order) in &settings.sidebar_section_order {
        content.push_str(&format!(
            "sidebar_section_order.{}={}\n",
            account_id,
            order.join(",")
        ));
    }

    for (account_id, collapsed) in &settings.sidebar_collapsed_sections {
        if !collapsed.is_empty() {
            let mut ids: Vec<&str> = collapsed.iter().map(String::as_str).collect();
            ids.sort();
            content.push_str(&format!(
                "sidebar_collapsed.{}={}\n",
                account_id,
                ids.join(",")
            ));
        }
    }

    for (conversation_id, pinned_item_id) in &settings.dismissed_pinned_items {
        if !pinned_item_id.trim().is_empty() {
            content.push_str(&format!(
                "dismissed_pinned.{}={}\n",
                conversation_id, pinned_item_id
            ));
        }
    }

    let mut notification_entries = settings
        .conversation_notifications
        .iter()
        .collect::<Vec<(&String, &ConversationNotificationSettings)>>();
    notification_entries.sort_by(|left, right| left.0.cmp(right.0));
    for (conversation_id, notification) in notification_entries {
        if conversation_id.trim().is_empty() {
            continue;
        }
        content.push_str(&format!(
            "conversation_notifications.{}={},{},{},{}\n",
            conversation_id,
            notification.muted,
            notification.ignore_mentions,
            notification.desktop.as_str(),
            notification.mobile.as_str(),
        ));
    }

    let mut affinity_entries = settings
        .quick_switcher_affinity
        .iter()
        .collect::<Vec<(&String, &(f32, i64))>>();
    affinity_entries.sort_by(|left, right| left.0.cmp(right.0));
    for (conversation_id, (affinity, last_updated_ms)) in affinity_entries {
        if conversation_id.trim().is_empty() || !affinity.is_finite() {
            continue;
        }
        content.push_str(&format!(
            "quick_switcher_affinity.{}={},{}\n",
            conversation_id,
            affinity.max(0.0),
            last_updated_ms
        ));
    }

    content
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn theme_label(theme: &ThemeMode) -> &'static str {
    match theme {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
        ThemeMode::System => "system",
    }
}

fn density_label(density: &Density) -> &'static str {
    match density {
        Density::Comfortable => "comfortable",
        Density::Compact => "compact",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_modes_round_trip_and_missing_preferences_use_default() {
        let mut settings = SettingsModel::default();
        settings
            .sidebar_modes
            .insert("workspace:alice".into(), SidebarMode::Recent);
        settings
            .sidebar_modes
            .insert("workspace:bob".into(), SidebarMode::Grouped);
        let parsed = parse_settings(&serialize_settings(&settings));
        assert_eq!(parsed.sidebar_modes, settings.sidebar_modes);
        assert!(parse_settings("theme=dark\n").sidebar_modes.is_empty());
        assert_eq!(
            parse_settings("sidebar_mode.workspace:alice=unknown").sidebar_modes["workspace:alice"],
            SidebarMode::Grouped
        );
    }

    #[test]
    fn conversations_share_round_trips_and_ignores_nonsense() {
        let mut settings = SettingsModel::default();
        assert_eq!(settings.sidebar_conversations_share, None);
        settings.sidebar_conversations_share = Some(0.42);
        let parsed = parse_settings(&serialize_settings(&settings));
        assert_eq!(parsed.sidebar_conversations_share, Some(0.42));
        assert_eq!(
            parse_settings("sidebar_conversations_share=7\n").sidebar_conversations_share,
            None
        );
    }

    #[test]
    fn sidebar_style_round_trips_and_defaults_to_compact() {
        let mut settings = SettingsModel::default();
        assert_eq!(settings.sidebar_style, SidebarStyle::Compact);
        settings.sidebar_style = SidebarStyle::Previews;
        let serialized = serialize_settings(&settings);
        assert!(serialized.contains("sidebar_style=previews\n"));
        assert_eq!(
            parse_settings(&serialized).sidebar_style,
            SidebarStyle::Previews
        );
        // Settings saved before the option existed keep the compact rows.
        assert_eq!(
            parse_settings("theme=dark\n").sidebar_style,
            SidebarStyle::Compact
        );
    }

    #[test]
    fn menu_bar_icon_round_trips_and_is_off_until_turned_on() {
        let mut settings = SettingsModel::default();
        assert!(!settings.show_menu_bar_icon);
        settings.show_menu_bar_icon = true;
        let serialized = serialize_settings(&settings);
        assert!(serialized.contains("show_menu_bar_icon=true\n"));
        assert!(parse_settings(&serialized).show_menu_bar_icon);
        // Settings saved before the option existed keep the icon off.
        assert!(!parse_settings("theme=dark\n").show_menu_bar_icon);
    }

    #[test]
    fn name_style_round_trips_and_defaults_to_full_names() {
        let mut settings = SettingsModel::default();
        assert_eq!(settings.name_style, NameStyle::FullName);
        settings.name_style = NameStyle::Username;
        let serialized = serialize_settings(&settings);
        assert!(serialized.contains("name_style=username\n"));
        assert_eq!(parse_settings(&serialized).name_style, NameStyle::Username);
        assert_eq!(
            parse_settings("theme=dark\n").name_style,
            NameStyle::FullName
        );
    }

    #[test]
    fn conversation_notification_settings_round_trip_and_ignore_malformed_entries() {
        let mut settings = SettingsModel::default();
        settings.conversation_notifications.insert(
            "kb_conv:aa".into(),
            ConversationNotificationSettings {
                muted: true,
                ignore_mentions: true,
                desktop: NotificationLevel::Nothing,
                mobile: NotificationLevel::MentionsOnly,
            },
        );
        settings.conversation_notifications.insert(
            "kb_conv:bb".into(),
            ConversationNotificationSettings::default(),
        );

        let serialized = serialize_settings(&settings);
        assert!(
            serialized
                .contains("conversation_notifications.kb_conv:aa=true,true,nothing,mentions\n")
        );
        assert!(serialized.contains("conversation_notifications.kb_conv:bb=false,false,all,all\n"));
        let parsed = parse_settings(&serialized);
        assert_eq!(
            parsed.conversation_notifications,
            settings.conversation_notifications
        );

        let malformed = parse_settings(
            "conversation_notifications.good=false,true,all,nothing\n\
             conversation_notifications.missing=false,true,all\n\
             conversation_notifications.bad=false,maybe,all,all\n\
             conversation_notifications.extra=false,false,all,all,extra\n",
        );
        assert_eq!(malformed.conversation_notifications.len(), 1);
        assert_eq!(
            malformed.conversation_notifications["good"],
            ConversationNotificationSettings {
                muted: false,
                ignore_mentions: true,
                desktop: NotificationLevel::All,
                mobile: NotificationLevel::Nothing,
            }
        );
    }

    #[test]
    fn quick_switcher_affinity_round_trips_through_settings_content() {
        let mut settings = SettingsModel::default();
        settings
            .quick_switcher_affinity
            .insert("conv_sample_a".to_string(), (3.25, 1_710_000_000_000));
        settings
            .quick_switcher_affinity
            .insert("conv_sample_b".to_string(), (0.5, 1_709_000_000_000));

        let serialized = serialize_settings(&settings);
        assert!(serialized.contains("quick_switcher_affinity.conv_sample_a=3.25,1710000000000"));
        assert!(serialized.contains("quick_switcher_affinity.conv_sample_b=0.5,1709000000000"));

        let parsed = parse_settings(&serialized);
        assert_eq!(
            parsed.quick_switcher_affinity.get("conv_sample_a"),
            Some(&(3.25, 1_710_000_000_000))
        );
        assert_eq!(
            parsed.quick_switcher_affinity.get("conv_sample_b"),
            Some(&(0.5, 1_709_000_000_000))
        );
    }

    #[test]
    fn quick_switcher_affinity_parse_ignores_malformed_entries() {
        let raw = r#"
quick_switcher_affinity.conv_ok=1.5,1710000000000
quick_switcher_affinity.conv_missing_timestamp=2.5
quick_switcher_affinity.conv_bad_affinity=abc,1710000000001
quick_switcher_affinity.conv_bad_timestamp=2.5,not_ms
quick_switcher_affinity.=2.5,1710000000002
"#;
        let parsed = parse_settings(raw);
        assert_eq!(parsed.quick_switcher_affinity.len(), 1);
        assert_eq!(
            parsed.quick_switcher_affinity.get("conv_ok"),
            Some(&(1.5, 1_710_000_000_000))
        );
    }
}

#[cfg(test)]
mod text_size_tests {
    use super::*;

    #[test]
    fn text_sizes_round_trip_and_invalid_values_use_default() {
        for percent in
            (TextSize::MIN_PERCENT..=TextSize::MAX_PERCENT).step_by(TextSize::STEP as usize)
        {
            let text_size = TextSize::from_percent(percent).unwrap();
            let settings = SettingsModel {
                text_size,
                ..Default::default()
            };
            assert_eq!(
                parse_settings(&serialize_settings(&settings)).text_size,
                text_size
            );
        }
        for raw in [
            "",
            "text_size=NaN",
            "text_size=0",
            "text_size=65535",
            "text_size=101",
            "text_size=205",
        ] {
            assert_eq!(parse_settings(raw).text_size, TextSize::default());
        }
    }
}
