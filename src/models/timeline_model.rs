use crate::domain::{
    affinity::Affinity,
    ids::{ConversationId, MessageId, UserId},
    message::{EmojiSourceRef, MessageRecord},
    user::UserSummary,
};
use std::collections::HashMap;

#[derive(Clone, Debug, serde::Serialize)]
pub struct InlineEmojiRender {
    pub alias: String,
    pub unicode: Option<String>,
    pub asset_path: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ReactionActorRender {
    pub user_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MessageReactionRender {
    pub emoji: String,
    pub source_ref: Option<EmojiSourceRef>,
    pub count: usize,
    pub actors: Vec<ReactionActorRender>,
    pub reacted_by_me: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MessageRow {
    pub author: UserSummary,
    pub message: MessageRecord,
    pub show_header: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum SystemEventIcon {
    Join,
    Leave,
    Add,
    Remove,
    Pin,
    Description,
    Settings,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum EventSpan {
    Actor(String),
    Text(String),
    ChannelLink {
        channel_name: String,
        team_name: Option<String>,
        conv_id: Option<ConversationId>,
    },
    UserLink(UserId),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SystemEventRow {
    pub icon: SystemEventIcon,
    pub spans: Vec<EventSpan>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "Message rows dominate the timeline; keep them inline without a per-row allocation."
)]
pub enum TimelineRow {
    DateDivider(String),
    UnreadDivider(String),
    SystemEvent(SystemEventRow),
    Message(MessageRow),
    LoadingIndicator(String),
}

pub const QUICK_REACT_EMOJI: &[(&str, &str)] = &[
    ("+1", "\u{1F44D}"),
    ("heart", "\u{2764}\u{FE0F}"),
    ("joy", "\u{1F602}"),
    ("tada", "\u{1F389}"),
    ("eyes", "\u{1F440}"),
];

/// Window content size in pixels; kept free of UI framework types.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct ViewportSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct TimelineModel {
    pub conversation_id: ConversationId,
    pub current_user_id: Option<UserId>,
    pub rows: Vec<TimelineRow>,
    pub highlighted_message_id: Option<MessageId>,
    pub editing_message_id: Option<MessageId>,
    pub unread_marker: Option<MessageId>,
    pub affinity_index: HashMap<UserId, Affinity>,
    pub emoji_index: HashMap<String, InlineEmojiRender>,
    pub emoji_source_index: HashMap<String, InlineEmojiRender>,
    pub reaction_index: HashMap<MessageId, Vec<MessageReactionRender>>,
    /// Who replied to each thread root among the loaded rows; see [`thread_summaries`].
    pub thread_summaries: HashMap<MessageId, ThreadSummary>,
    pub pending_scroll_target: Option<MessageId>,
    pub older_cursor: Option<String>,
    pub newer_cursor: Option<String>,
    pub loading_older: bool,
    pub hovered_message_id: Option<MessageId>,
    pub hovered_message_is_thread: Option<bool>,
    /// Cursor X position (window coords) captured on hover.
    pub hovered_message_anchor_x: Option<f32>,
    /// Cursor Y position (window coords) captured when the hover settled.
    pub hovered_message_anchor_y: Option<f32>,
    /// Hovered message container left edge in window coords.
    pub hovered_message_window_left: Option<f32>,
    /// Hovered message container top edge in window coords.
    pub hovered_message_window_top: Option<f32>,
    /// Hovered message container width in window coords.
    pub hovered_message_window_width: Option<f32>,
    pub hovered_message_viewport_size: Option<ViewportSize>,
    /// Whether the hover toolbar is settled (mouse was still long enough).
    pub hover_toolbar_settled: bool,
    pub typing_text: Option<String>,
    pub quick_react_recent: Option<QuickReactRecent>,
    /// Upload progress of pending attachment messages.
    pub upload_progress: HashMap<MessageId, crate::state::state::UploadProgress>,
    /// Root of the thread open in the right pane; its row is marked while open.
    pub open_thread_root_id: Option<MessageId>,
    /// A thread root whose header is scrolled above the viewport while the rest
    /// of it still shows, so its reply summary names whose message it follows.
    pub clipped_thread_root_id: Option<MessageId>,
}

/// The loaded replies to one thread root: distinct repliers in the order they
/// first replied (at most [`ThreadSummary::MAX_REPLIERS`]) and the newest reply's time.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ThreadSummary {
    pub repliers: Vec<UserSummary>,
    pub last_reply_ms: Option<i64>,
}

impl ThreadSummary {
    pub const MAX_REPLIERS: usize = 3;
}

/// Thread summaries for every root that has loaded replies. Keybase shows
/// replies inline, so a root's replies are the rows whose thread root it is.
pub fn thread_summaries(rows: &[TimelineRow]) -> HashMap<MessageId, ThreadSummary> {
    let mut summaries: HashMap<MessageId, ThreadSummary> = HashMap::new();
    for row in rows {
        let TimelineRow::Message(row) = row else {
            continue;
        };
        let Some(root_id) = row.message.thread_root_id.as_ref() else {
            continue;
        };
        if *root_id == row.message.id {
            continue;
        }
        let summary = summaries.entry(root_id.clone()).or_default();
        if let Some(timestamp_ms) = row.message.timestamp_ms {
            summary.last_reply_ms = Some(
                summary
                    .last_reply_ms
                    .map_or(timestamp_ms, |last| last.max(timestamp_ms)),
            );
        }
        let known = summary
            .repliers
            .iter()
            .any(|replier| replier.id.0.eq_ignore_ascii_case(&row.author.id.0));
        if !known && summary.repliers.len() < ThreadSummary::MAX_REPLIERS {
            summary.repliers.push(row.author.clone());
        }
    }
    summaries
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct QuickReactRecent {
    pub alias: String,
    pub unicode: Option<String>,
    pub asset_path: Option<String>,
}

impl TimelineModel {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// Places day dividers while timeline rows are built oldest first: one before the
/// first message of each local calendar day.
pub struct DayDividers {
    today: chrono::NaiveDate,
    current: Option<chrono::NaiveDate>,
}

impl DayDividers {
    pub fn new(today: chrono::NaiveDate) -> Self {
        Self {
            today,
            current: None,
        }
    }

    /// Label of the divider to insert before a message sent at `timestamp_ms`, if
    /// it opens a new day. Messages without a timestamp never open one.
    pub fn before(&mut self, timestamp_ms: Option<i64>) -> Option<String> {
        let day = timestamp_ms.and_then(crate::util::formatting::local_day)?;
        if self.current == Some(day) {
            return None;
        }
        self.current = Some(day);
        Some(crate::util::formatting::day_divider_label(day, self.today))
    }
}

#[cfg(test)]
mod tests {
    use super::DayDividers;
    use chrono::{Local, NaiveDate, TimeZone};

    fn at(day: u32, hour: u32) -> Option<i64> {
        Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 11, day)
                    .unwrap()
                    .and_hms_opt(hour, 0, 0)
                    .unwrap(),
            )
            .earliest()
            .map(|time| time.timestamp_millis())
    }

    #[test]
    fn one_divider_opens_each_local_day() {
        let mut dividers = DayDividers::new(NaiveDate::from_ymd_opt(2026, 11, 6).unwrap());
        let labels = [
            at(3, 9),
            at(3, 23),
            at(5, 0),
            None,
            at(5, 18),
            at(6, 7),
            at(6, 8),
        ]
        .into_iter()
        .map(|timestamp| dividers.before(timestamp))
        .collect::<Vec<_>>();
        assert_eq!(
            labels,
            [
                Some("Tuesday, Nov 3".to_string()),
                None,
                Some("Yesterday".to_string()),
                None,
                None,
                Some("Today".to_string()),
                None,
            ]
        );
    }
}

#[cfg(test)]
mod thread_summary_tests {
    use super::*;
    use crate::domain::{
        message::MessageSendState,
        presence::{Availability, Presence},
    };

    fn row(id: &str, author: &str, root: Option<&str>, timestamp_ms: i64) -> TimelineRow {
        TimelineRow::Message(MessageRow {
            author: UserSummary {
                id: UserId::new(author),
                display_name: author.to_string(),
                title: String::new(),
                avatar_asset: None,
                presence: Presence {
                    availability: Availability::Unknown,
                    status_text: None,
                },
                affinity: Affinity::None,
            },
            message: MessageRecord {
                id: MessageId::new(id),
                conversation_id: ConversationId::new("c"),
                author_id: UserId::new(author),
                reply_to: root.map(MessageId::new),
                thread_root_id: root.map(MessageId::new),
                timestamp_ms: Some(timestamp_ms),
                event: None,
                link_previews: Vec::new(),
                permalink: String::new(),
                fragments: Vec::new(),
                source_text: None,
                attachments: Vec::new(),
                reactions: Vec::new(),
                thread_reply_count: 0,
                send_state: MessageSendState::Sent,
                edited: None,
            },
            show_header: true,
        })
    }

    #[test]
    fn collects_distinct_repliers_in_reply_order_and_the_latest_time() {
        let rows = vec![
            row("1", "alice", None, 100),
            row("2", "lena", Some("1"), 200),
            row("3", "Robin", Some("1"), 400),
            row("4", "LENA", Some("1"), 300),
            row("5", "sam", None, 500),
        ];
        let summaries = thread_summaries(&rows);
        assert_eq!(summaries.len(), 1);
        let summary = &summaries[&MessageId::new("1")];
        let names: Vec<_> = summary
            .repliers
            .iter()
            .map(|user| user.id.0.as_str())
            .collect();
        assert_eq!(names, ["lena", "Robin"]);
        assert_eq!(summary.last_reply_ms, Some(400));
    }

    #[test]
    fn caps_the_replier_faces() {
        let mut rows = vec![row("1", "alice", None, 0)];
        for (index, author) in ["a", "b", "c", "d"].into_iter().enumerate() {
            rows.push(row(&format!("r{index}"), author, Some("1"), index as i64));
        }
        let summary = &thread_summaries(&rows)[&MessageId::new("1")];
        assert_eq!(summary.repliers.len(), ThreadSummary::MAX_REPLIERS);
        assert_eq!(summary.last_reply_ms, Some(3));
    }
}
