//! Display data the gpui overlay views derive at render time (label splits,
//! cleaned snippets, hit contexts, the emoji catalog). The web view cannot run
//! this Rust, so the engine derives it and publishes it in `overlays_extra`.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::{
    domain::{ids::UserId, route::Route, search::SearchResult},
    models::{
        AppModels,
        conversation_model::AvatarParticipant,
        emoji_picker_model::{
            EmojiPickerItem, EmojiPickerModel, SKIN_TONE_OPTIONS, search_emoji_items,
            selected_emoji_for_tone,
        },
        quick_switcher_model::{QuickSwitcherResult, QuickSwitcherResultKind},
        sidebar_model::{SidebarAvatar, team_avatar_key},
    },
    state::{ConnectionState, UiState as AppState},
    util::{
        formatting::{compact_date_label, message_full_timestamp},
        search_snippet::{MentionKind, clean_search_snippet},
    },
};

const EMOJI_SEARCH_MAX_RESULTS: usize = 50;

/// A run of label text; `matched` runs are the query's letters.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Segment {
    pub text: String,
    pub matched: bool,
}

/// A run of a cleaned snippet: query match, mention kind ("user", "broadcast",
/// "channel"), or plain.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SnippetRun {
    pub text: String,
    pub matched: bool,
    pub mention: Option<&'static str>,
}

/// Who wrote a message hit, where, and when (`SearchResultContext` plus time labels).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HitView {
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub location: Option<String>,
    pub time_label: String,
    pub full_time: Option<String>,
    pub snippet: Vec<SnippetRun>,
}

/// One quick switcher result, as `quick_switcher_row` lays it out.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SwitcherRowView {
    pub is_dm: bool,
    /// Channel name, DM title or whole label.
    pub primary: Vec<Segment>,
    /// The team after a channel ("lighthouse" in "lighthouse #general").
    pub team: Option<Vec<Segment>>,
    /// A single-channel team named by its team; shows "  #general" after it.
    pub simple_team: bool,
    /// Name the team mark is drawn from, if the row has one.
    pub team_name: Option<String>,
    pub team_avatar: Option<String>,
    pub participants: Vec<AvatarParticipant>,
    /// Message hits are drawn with the Search anatomy.
    pub hit: Option<HitView>,
}

pub fn switcher_rows(
    models: &AppModels,
    snapshot: &AppState,
    avatars: &HashMap<String, SidebarAvatar>,
) -> Vec<SwitcherRowView> {
    models
        .quick_switcher
        .results
        .iter()
        .map(|result| switcher_row(result, snapshot, avatars))
        .collect()
}

fn switcher_row(
    result: &QuickSwitcherResult,
    snapshot: &AppState,
    avatars: &HashMap<String, SidebarAvatar>,
) -> SwitcherRowView {
    let is_message = result.kind == QuickSwitcherResultKind::Message;
    let is_dm = matches!(result.route, Route::DirectMessage { .. });
    let simple_team = !is_message && !is_dm && is_simple_team_label(&result.label);
    let (primary, team) = if is_message || is_dm || simple_team {
        let ranges = if is_message {
            Vec::new()
        } else {
            result.match_ranges.clone()
        };
        ((result.label.clone(), ranges), None)
    } else {
        split_channel_label(&result.label, &result.match_ranges)
    };
    let team_name = team
        .as_ref()
        .map(|(name, _)| name.clone())
        .or(simple_team.then(|| result.label.clone()));
    let team_avatar = team_name.as_ref().and_then(|_| {
        result
            .sublabel
            .as_deref()
            .and_then(|team_id| avatars.get(&team_avatar_key(team_id)))
            .and_then(|avatar| match avatar {
                SidebarAvatar::Team(asset) => Some(asset.clone()),
                SidebarAvatar::Participants(_) => None,
            })
    });
    let participants = if is_dm {
        avatars
            .get(&result.route.label())
            .map(|avatar| avatar.participants().to_vec())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let hit = result.author_id.as_ref().map(|author| {
        let (author_name, author_avatar) = author_profile(snapshot, author);
        HitView {
            author_name,
            author_avatar,
            location: Some(result.label.clone()),
            time_label: compact_date_label(result.timestamp_ms),
            full_time: result.timestamp_ms.and_then(message_full_timestamp),
            snippet: snippet_runs(
                result.sublabel.as_deref().unwrap_or_default(),
                &result.match_ranges,
            ),
        }
    });
    SwitcherRowView {
        is_dm,
        primary: segments(&primary.0, &primary.1),
        team: team.map(|(name, ranges)| segments(&name, &ranges)),
        simple_team,
        team_name,
        team_avatar,
        participants,
        hit,
    }
}

/// `AppWindow::search_result_contexts` plus the row's time and snippet.
pub fn search_rows(models: &AppModels, snapshot: &AppState) -> Vec<HitView> {
    models
        .search
        .results
        .iter()
        .map(|result| search_row(models, snapshot, result))
        .collect()
}

fn search_row(models: &AppModels, snapshot: &AppState, result: &SearchResult) -> HitView {
    let (author_name, author_avatar) = author_profile(snapshot, &result.message.author_id);
    HitView {
        author_name,
        author_avatar,
        location: models.conversation_location_label(&result.conversation_id),
        time_label: compact_date_label(result.message.timestamp_ms),
        full_time: result.message.timestamp_ms.and_then(message_full_timestamp),
        snippet: snippet_runs(&result.snippet, &result.snippet_highlight_ranges),
    }
}

/// `AppWindow::message_hit_context`: the author's display name and avatar.
fn author_profile(snapshot: &AppState, author: &UserId) -> (String, Option<String>) {
    let profiles = &snapshot.backend.user_profiles;
    let profile = profiles
        .get(author)
        .or_else(|| profiles.get(&UserId::new(author.0.to_ascii_lowercase())));
    (
        profile
            .map(|profile| profile.display_name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(author.0.as_str())
            .to_string(),
        profile.and_then(|profile| profile.avatar_asset.clone()),
    )
}

fn snippet_runs(raw: &str, ranges: &[(usize, usize)]) -> Vec<SnippetRun> {
    let snippet = clean_search_snippet(raw, ranges);
    snippet
        .runs()
        .into_iter()
        .map(|run| SnippetRun {
            text: snippet.text[run.range.0..run.range.1].to_string(),
            matched: run.matched,
            mention: run.mention.map(|kind| match kind {
                MentionKind::User(_) => "user",
                MentionKind::Broadcast => "broadcast",
                MentionKind::Channel => "channel",
            }),
        })
        .collect()
}

/// A channel label without "#": a single-channel team named by its team.
fn is_simple_team_label(label: &str) -> bool {
    !label.starts_with('#') && !label.contains(" #")
}

type LabelPart = (String, Vec<(usize, usize)>);

/// Splits "lighthouse #general" into "#general" and "lighthouse", rebasing ranges.
fn split_channel_label(label: &str, ranges: &[(usize, usize)]) -> (LabelPart, Option<LabelPart>) {
    let Some(split) = label.find(" #") else {
        return ((label.to_string(), ranges.to_vec()), None);
    };
    let channel_start = split + 1;
    let channel = label[channel_start..].to_string();
    let channel_ranges = ranges
        .iter()
        .filter_map(|&(start, end)| {
            let start = start.max(channel_start);
            (start < end).then(|| (start - channel_start, end - channel_start))
        })
        .collect();
    let team_ranges = ranges
        .iter()
        .filter_map(|&(start, end)| {
            let end = end.min(split);
            (start < end).then_some((start, end))
        })
        .collect();
    (
        (channel, channel_ranges),
        Some((label[..split].to_string(), team_ranges)),
    )
}

/// `highlight_segments`: plain and matched runs; ranges off char boundaries are ignored.
fn segments(text: &str, ranges: &[(usize, usize)]) -> Vec<Segment> {
    let mut ranges: Vec<(usize, usize)> = ranges
        .iter()
        .copied()
        .filter(|&(start, end)| {
            start < end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end)
        })
        .collect();
    ranges.sort_unstable();
    let mut out = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        let start = start.max(cursor);
        if start >= end {
            continue;
        }
        if start > cursor {
            out.push(Segment {
                text: text[cursor..start].to_string(),
                matched: false,
            });
        }
        out.push(Segment {
            text: text[start..end].to_string(),
            matched: true,
        });
        cursor = end;
    }
    if cursor < text.len() {
        out.push(Segment {
            text: text[cursor..].to_string(),
            matched: false,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Emoji picker

/// One emoji cell: a glyph (skin tone applied), or a custom emoji image.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmojiCell {
    pub key: String,
    pub glyph: Option<String>,
    pub asset: Option<String>,
    pub alias: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmojiSection {
    pub title: String,
    pub items: Vec<EmojiCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmojiBookmark {
    pub label: String,
    pub selected: bool,
    /// The group it jumps to (`None` for Frequent and Custom).
    pub group: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmojiTone {
    pub tone: String,
    pub color: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmojiFooter {
    pub cell: EmojiCell,
    pub title: String,
    pub subtitle: String,
}

/// Everything `render_emoji_picker` draws.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmojiPickerView {
    pub tones: Vec<EmojiTone>,
    pub bookmarks: Vec<EmojiBookmark>,
    /// Frequent, the groups (active one first) and custom; when the query is empty.
    pub sections: Vec<EmojiSection>,
    /// Search results when the query is not empty.
    pub search_results: Option<Vec<EmojiCell>>,
    pub footer: Option<EmojiFooter>,
}

/// Cache key: the inputs the catalog depends on, except the hovered item.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EmojiCatalogKey {
    query: String,
    tone: Option<String>,
    group: Option<String>,
    recents: Vec<String>,
    custom: Vec<String>,
}

/// The built catalog and the items behind its keys, for hover and pick.
#[derive(Default)]
pub struct EmojiCatalog {
    key: Option<EmojiCatalogKey>,
    view: Option<EmojiPickerView>,
    custom_items: Vec<EmojiPickerItem>,
    frequent_items: Vec<EmojiPickerItem>,
}

impl EmojiCatalog {
    /// The picker view for the current models, rebuilt when its inputs change.
    pub fn view(&mut self, picker: &EmojiPickerModel, snapshot: &AppState) -> EmojiPickerView {
        let custom_items = custom_items(snapshot);
        let key = EmojiCatalogKey {
            query: picker.query.trim().to_string(),
            tone: debug_name(picker.selected_skin_tone),
            group: debug_name(picker.active_group),
            recents: picker.recent_aliases.clone(),
            custom: custom_items.iter().map(EmojiPickerItem::key).collect(),
        };
        if self.key.as_ref() != Some(&key) || self.view.is_none() {
            let frequent_items = resolve_frequent_items(&picker.recent_aliases, &custom_items);
            self.view = Some(build_view(picker, &frequent_items, &custom_items));
            self.custom_items = custom_items;
            self.frequent_items = frequent_items;
            self.key = Some(key);
        }
        let mut view = self.view.clone().unwrap_or_else(|| unreachable!());
        let hovered = picker
            .hovered
            .clone()
            .or_else(|| self.frequent_items.first().cloned());
        view.footer = hovered.map(|item| {
            let (title, subtitle) = hover_text(&item);
            EmojiFooter {
                cell: cell(&item, picker.selected_skin_tone),
                title,
                subtitle,
            }
        });
        view
    }

    /// The item behind a cell key: custom emoji first, then stock.
    pub fn item(&self, key: &str) -> Option<EmojiPickerItem> {
        if let Some(item) = self.custom_items.iter().find(|item| item.key() == key) {
            return Some(item.clone());
        }
        stock_item(key)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn stock_item(key: &str) -> Option<EmojiPickerItem> {
    if let Some(emoji) = emojis::get_by_shortcode(key) {
        return Some(EmojiPickerItem::Stock(emoji));
    }
    emojis::iter()
        .map(EmojiPickerItem::Stock)
        .find(|item| item.key() == key)
}

pub fn debug_name<T: std::fmt::Debug>(value: Option<T>) -> Option<String> {
    value.map(|value| format!("{value:?}"))
}

pub fn group_from_name(name: &str) -> Option<emojis::Group> {
    emojis::Group::iter().find(|group| format!("{group:?}") == name)
}

pub fn tone_from_name(name: &str) -> Option<emojis::SkinTone> {
    SKIN_TONE_OPTIONS
        .into_iter()
        .find(|tone| format!("{tone:?}") == name)
}

/// `AppWindow::current_conversation_custom_emoji_items` (custom emoji of the open chat).
pub fn custom_items(snapshot: &AppState) -> Vec<EmojiPickerItem> {
    let supports_custom_emoji = snapshot
        .backend
        .accounts
        .values()
        .find(|account| matches!(account.connection_state, ConnectionState::Connected))
        .or_else(|| snapshot.backend.accounts.values().next())
        .map(|account| account.capabilities.supports_custom_emoji)
        .unwrap_or(false);
    if !supports_custom_emoji {
        return Vec::new();
    }
    let Some(conversation_id) = snapshot.timeline.conversation_id.as_ref() else {
        return Vec::new();
    };
    let mut items = snapshot
        .backend
        .conversation_emojis
        .get(conversation_id)
        .into_iter()
        .flat_map(|index| index.values())
        .map(|emoji| EmojiPickerItem::Custom {
            alias: emoji.alias.clone(),
            unicode: emoji.unicode.clone(),
            asset_path: emoji.asset_path.clone(),
        })
        .collect::<Vec<_>>();
    items.sort_by_key(EmojiPickerItem::key);
    items
}

fn build_view(
    picker: &EmojiPickerModel,
    frequent_items: &[EmojiPickerItem],
    custom_items: &[EmojiPickerItem],
) -> EmojiPickerView {
    let tone = picker.selected_skin_tone;
    let tones = SKIN_TONE_OPTIONS
        .into_iter()
        .map(|option| EmojiTone {
            tone: format!("{option:?}"),
            color: format!("#{:06x}", skin_tone_color(option)),
            selected: Some(option) == tone,
        })
        .collect();
    let query = picker.query.trim();
    if !query.is_empty() {
        let results = search_emoji_items(query, custom_items, EMOJI_SEARCH_MAX_RESULTS);
        return EmojiPickerView {
            tones,
            bookmarks: Vec::new(),
            sections: Vec::new(),
            search_results: Some(cells(&results, tone)),
            footer: None,
        };
    }

    let active_group = picker.active_group;
    let mut bookmarks = Vec::new();
    let default_active = emojis::Group::iter().next();
    let effective_active = active_group.or(default_active);
    if !frequent_items.is_empty() {
        bookmarks.push(EmojiBookmark {
            label: "🕐".to_string(),
            selected: effective_active.is_none(),
            group: None,
        });
    }
    for group in emojis::Group::iter() {
        bookmarks.push(EmojiBookmark {
            label: group_bookmark_emoji(group).to_string(),
            selected: Some(group) == effective_active,
            group: Some(format!("{group:?}")),
        });
    }
    if !custom_items.is_empty() {
        bookmarks.push(EmojiBookmark {
            label: "⭐".to_string(),
            selected: false,
            group: None,
        });
    }

    let mut groups = emojis::Group::iter().collect::<Vec<_>>();
    if let Some(group) = active_group
        && let Some(index) = groups.iter().position(|candidate| *candidate == group)
    {
        let selected = groups.remove(index);
        groups.insert(0, selected);
    }
    let mut sections = Vec::new();
    if !frequent_items.is_empty() {
        sections.push(EmojiSection {
            title: "Frequently used".to_string(),
            items: cells(frequent_items, tone),
        });
    }
    for group in groups {
        let items = group
            .emojis()
            .map(EmojiPickerItem::Stock)
            .collect::<Vec<_>>();
        sections.push(EmojiSection {
            title: group_label(group).to_string(),
            items: cells(&items, tone),
        });
    }
    if !custom_items.is_empty() {
        sections.push(EmojiSection {
            title: "Custom emoji".to_string(),
            items: cells(custom_items, tone),
        });
    }
    EmojiPickerView {
        tones,
        bookmarks,
        sections,
        search_results: None,
        footer: None,
    }
}

/// `render_grid` drops repeated keys within a grid.
fn cells(items: &[EmojiPickerItem], tone: Option<emojis::SkinTone>) -> Vec<EmojiCell> {
    let mut seen = HashSet::new();
    items
        .iter()
        .filter(|item| seen.insert(item.key()))
        .map(|item| cell(item, tone))
        .collect()
}

fn cell(item: &EmojiPickerItem, tone: Option<emojis::SkinTone>) -> EmojiCell {
    match item {
        EmojiPickerItem::Stock(emoji) => EmojiCell {
            key: item.key(),
            glyph: Some(selected_emoji_for_tone(emoji, tone).as_str().to_string()),
            asset: None,
            alias: item.key(),
        },
        EmojiPickerItem::Custom {
            alias,
            unicode,
            asset_path,
        } => EmojiCell {
            key: item.key(),
            glyph: if asset_path.is_some() {
                None
            } else {
                unicode.clone()
            },
            asset: asset_path.clone(),
            alias: alias.clone(),
        },
    }
}

fn hover_text(item: &EmojiPickerItem) -> (String, String) {
    match item {
        EmojiPickerItem::Stock(emoji) => {
            let title = emoji.name().to_string();
            let shortcodes = emoji
                .shortcodes()
                .map(|shortcode| format!(":{shortcode}:"))
                .collect::<Vec<_>>()
                .join("  ");
            (title, shortcodes)
        }
        EmojiPickerItem::Custom { alias, .. } => (format!(":{alias}:"), "Custom emoji".to_string()),
    }
}

fn resolve_frequent_items(
    recent_aliases: &[String],
    custom_items: &[EmojiPickerItem],
) -> Vec<EmojiPickerItem> {
    let custom_lookup = custom_items
        .iter()
        .filter_map(|item| match item {
            EmojiPickerItem::Custom { alias, .. } => {
                Some((alias.to_ascii_lowercase(), item.clone()))
            }
            EmojiPickerItem::Stock(_) => None,
        })
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for alias in recent_aliases {
        let key = alias.trim().to_ascii_lowercase();
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if let Some(custom) = custom_lookup.get(&key) {
            items.push(custom.clone());
            continue;
        }
        if let Some(stock) =
            crate::util::emoji_aliases::standard_emoji_for_alias(&key).and_then(emojis::get)
        {
            items.push(EmojiPickerItem::Stock(stock));
        }
    }
    items
}

fn group_label(group: emojis::Group) -> &'static str {
    match group {
        emojis::Group::SmileysAndEmotion => "Smileys & Emotion",
        emojis::Group::PeopleAndBody => "People & Body",
        emojis::Group::AnimalsAndNature => "Animals & Nature",
        emojis::Group::FoodAndDrink => "Food & Drink",
        emojis::Group::TravelAndPlaces => "Travel & Places",
        emojis::Group::Activities => "Activities",
        emojis::Group::Objects => "Objects",
        emojis::Group::Symbols => "Symbols",
        emojis::Group::Flags => "Flags",
    }
}

fn group_bookmark_emoji(group: emojis::Group) -> &'static str {
    match group {
        emojis::Group::SmileysAndEmotion => "😀",
        emojis::Group::PeopleAndBody => "🖐",
        emojis::Group::AnimalsAndNature => "🐻",
        emojis::Group::FoodAndDrink => "🍎",
        emojis::Group::TravelAndPlaces => "✈️",
        emojis::Group::Activities => "⚽",
        emojis::Group::Objects => "💡",
        emojis::Group::Symbols => "🔣",
        emojis::Group::Flags => "🏁",
    }
}

fn skin_tone_color(tone: emojis::SkinTone) -> u32 {
    match tone {
        emojis::SkinTone::Light => 0xf6d6b7,
        emojis::SkinTone::MediumLight => 0xe6bc98,
        emojis::SkinTone::Medium => 0xd19a73,
        emojis::SkinTone::MediumDark => 0xad7a52,
        emojis::SkinTone::Dark => 0x7f5539,
        _ => 0xf4cc87,
    }
}
