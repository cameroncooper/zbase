//! The second line of a sidebar row in the Previews style: who wrote the newest
//! message and what it said, as one line of plain text.
use crate::{
    domain::{
        conversation::{ConversationKind, ConversationSummary},
        message_preview::{MessagePreview, PreviewBody},
    },
    util::search_snippet::clean_search_snippet,
};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SidebarPreview {
    /// Leads the text: "You: ", "Lena: ", or "Lena " before an action.
    pub prefix: Option<String>,
    /// Plain text: Markdown markers removed, `:shortcode:` emoji rendered.
    pub text: String,
    /// When the conversation last changed, for the trailing time label.
    pub timestamp_ms: Option<i64>,
}

/// What a conversation's preview line shows instead of its newest message,
/// because it is happening now or needs you.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PreviewLive {
    /// "Lena is typing…"
    Typing(String),
    /// Your unsent draft, as one line of plain text.
    Draft(String),
    /// Your newest message could not be sent.
    NotSent,
}

/// The live state to show, most pressing first: a failed send needs action,
/// typing is happening now, and a draft is shown only for conversations you
/// aren't looking at (the open one shows it in its composer).
pub fn live_preview(
    typing: Option<String>,
    draft: Option<&str>,
    not_sent: bool,
    is_open: bool,
) -> Option<PreviewLive> {
    if not_sent {
        return Some(PreviewLive::NotSent);
    }
    if let Some(typing) = typing {
        return Some(PreviewLive::Typing(typing));
    }
    if is_open {
        return None;
    }
    draft
        .map(clean)
        .filter(|draft| !draft.is_empty())
        .map(PreviewLive::Draft)
}

/// Whose names previews use: the signed-in user (shown as "You") and display
/// names by lowercase username. People without a display name show their username.
pub struct PreviewNames<'a> {
    pub me: Option<&'a str>,
    pub display_names: &'a HashMap<String, String>,
}

impl PreviewNames<'_> {
    fn is_me(&self, username: &str) -> bool {
        self.me
            .is_some_and(|me| me.eq_ignore_ascii_case(username.trim()))
    }

    /// "Lena is typing…" for the people typing, by username; you never count.
    pub fn typing_line<S: AsRef<str>>(&self, usernames: &[S]) -> Option<String> {
        let names = usernames
            .iter()
            .map(AsRef::as_ref)
            .filter(|username| !self.is_me(username))
            .map(|username| self.short_name(username))
            .collect::<Vec<_>>();
        crate::util::formatting::typing_indicator_label(&names)
    }

    /// First name, as chat lists name people: "Lena" for "Lena Fischer".
    fn short_name(&self, username: &str) -> String {
        self.display_names
            .get(&username.to_ascii_lowercase())
            .and_then(|name| name.split_whitespace().next())
            .map(str::to_string)
            .unwrap_or_else(|| username.to_string())
    }
}

/// Second line for every conversation, keyed by conversation id.
pub fn sidebar_previews<'a>(
    summaries: impl IntoIterator<Item = &'a ConversationSummary>,
    names: &PreviewNames<'_>,
) -> HashMap<String, SidebarPreview> {
    summaries
        .into_iter()
        .map(|summary| (summary.id.0.clone(), sidebar_preview(summary, names)))
        .collect()
}

/// The row's second line. Without a known message it names who is in the
/// conversation (or its team), so the row never shows an empty line.
pub fn sidebar_preview(summary: &ConversationSummary, names: &PreviewNames<'_>) -> SidebarPreview {
    let timestamp_ms = summary
        .last_message
        .as_ref()
        .map(|message| message.timestamp_ms)
        .filter(|at| *at > 0)
        .or((summary.last_activity_ms > 0).then_some(summary.last_activity_ms));
    let line = summary
        .last_message
        .as_ref()
        .and_then(|message| preview_line(message, &summary.kind, names));
    let (prefix, text) = line.unwrap_or_else(|| (None, placeholder(summary)));
    SidebarPreview {
        prefix,
        text,
        timestamp_ms,
    }
}

/// Prefix and text for a message. Your own messages read "You: …"; a 1:1
/// conversation never names the other person, who is already the row's title.
fn preview_line(
    message: &MessagePreview,
    kind: &ConversationKind,
    names: &PreviewNames<'_>,
) -> Option<(Option<String>, String)> {
    let author = message.author_id.as_ref().map(|author| author.0.as_str());
    let speaker = match author {
        Some(author) if names.is_me(author) => Some("You".to_string()),
        Some(_) if *kind == ConversationKind::DirectMessage => None,
        Some(author) => Some(names.short_name(author)),
        None => None,
    };
    let quoted = |text: String| {
        let text = clean(&text);
        (!text.is_empty()).then(|| (speaker.as_ref().map(|name| format!("{name}: ")), text))
    };
    // Actions follow the name ("Lena joined…"), or stand alone capitalized.
    let action = |phrase: &str| match &speaker {
        Some(name) => (Some(format!("{name} ")), phrase.to_string()),
        None => (None, capitalized(phrase)),
    };
    match &message.body {
        PreviewBody::Text(text) => quoted(text.clone()),
        PreviewBody::Attachment { caption, name, .. } => {
            let label = caption
                .as_deref()
                .map(clean)
                .filter(|caption| !caption.is_empty())
                .or_else(|| (!name.trim().is_empty()).then(|| name.trim().to_string()));
            Some(match label {
                Some(label) => (
                    speaker.as_ref().map(|name| format!("{name}: ")),
                    format!("📎 {label}"),
                ),
                None => action("sent an attachment"),
            })
        }
        PreviewBody::Event(phrase) => Some(action(phrase)),
        PreviewBody::Deleted => Some((None, "Message deleted".to_string())),
        PreviewBody::Snippet(text) => {
            let text = clean(text);
            (!text.is_empty()).then_some((None, text))
        }
    }
}

fn clean(text: &str) -> String {
    clean_search_snippet(text, &[]).text
}

fn capitalized(phrase: &str) -> String {
    let mut chars = phrase.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn placeholder(summary: &ConversationSummary) -> String {
    match summary.kind {
        ConversationKind::Channel => summary
            .group
            .as_ref()
            .map(|group| group.display_name.clone())
            .unwrap_or_default(),
        ConversationKind::DirectMessage | ConversationKind::GroupDirectMessage => summary
            .title
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| format!("@{name}"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        attachment::AttachmentKind,
        conversation::{ConversationGroup, ConversationTeamType},
        ids::{ConversationId, MessageId, UserId},
    };

    fn summary(kind: ConversationKind, body: PreviewBody, author: &str) -> ConversationSummary {
        ConversationSummary {
            id: ConversationId::new("c"),
            title: "lena_f".into(),
            kind,
            topic: String::new(),
            group: Some(ConversationGroup {
                id: "lighthouse".into(),
                display_name: "lighthouse".into(),
                team_type: ConversationTeamType::Complex,
            }),
            unread_count: 0,
            mention_count: 0,
            muted: false,
            last_activity_ms: 10,
            last_message: Some(MessagePreview {
                message_id: Some(MessageId::new("7")),
                author_id: (!author.is_empty()).then(|| UserId::new(author)),
                timestamp_ms: 20,
                body,
            }),
        }
    }

    fn line(kind: ConversationKind, body: PreviewBody, author: &str) -> String {
        let names = HashMap::from([
            ("lena_f".to_string(), "Lena Fischer".to_string()),
            ("me".to_string(), "Me Myself".to_string()),
        ]);
        let preview = sidebar_preview(
            &summary(kind, body, author),
            &PreviewNames {
                me: Some("Me"),
                display_names: &names,
            },
        );
        format!("{}{}", preview.prefix.unwrap_or_default(), preview.text)
    }

    fn text(value: &str) -> PreviewBody {
        PreviewBody::Text(value.to_string())
    }

    #[test]
    fn author_prefix_names_others_in_groups_and_you_everywhere() {
        use ConversationKind::*;
        assert_eq!(line(Channel, text("ship it"), "lena_f"), "Lena: ship it");
        assert_eq!(
            line(GroupDirectMessage, text("ship it"), "lena_f"),
            "Lena: ship it"
        );
        // The other person in a 1:1 is the row's title already.
        assert_eq!(line(DirectMessage, text("ship it"), "lena_f"), "ship it");
        assert_eq!(line(DirectMessage, text("ship it"), "me"), "You: ship it");
        assert_eq!(line(Channel, text("ship it"), "me"), "You: ship it");
        // No display name: the username stands in.
        assert_eq!(line(Channel, text("hi"), "devon"), "devon: hi");
    }

    #[test]
    fn markdown_is_stripped_and_emoji_rendered_on_one_line() {
        let body = text("Release banner is final :tada:\n`assets/banner.png`, *please* keep it");
        assert_eq!(
            line(ConversationKind::DirectMessage, body, "lena_f"),
            "Release banner is final 🎉 assets/\u{200A}banner.png, please keep it"
        );
        // A message that is only Markdown markers has nothing to show.
        assert_eq!(
            line(ConversationKind::DirectMessage, text("``` ```"), "lena_f"),
            "@lena_f"
        );
    }

    #[test]
    fn attachments_events_and_deletions_read_as_sentences() {
        use ConversationKind::*;
        let file = |name: &str, caption: Option<&str>| PreviewBody::Attachment {
            kind: AttachmentKind::Image,
            name: name.into(),
            caption: caption.map(str::to_string),
        };
        assert_eq!(
            line(Channel, file("screenshot.png", None), "lena_f"),
            "Lena: 📎 screenshot.png"
        );
        assert_eq!(
            line(
                DirectMessage,
                file("screenshot.png", Some("New *icon*")),
                "lena_f"
            ),
            "📎 New icon"
        );
        assert_eq!(
            line(Channel, file("", None), "me"),
            "You sent an attachment"
        );
        assert_eq!(
            line(DirectMessage, file("", None), "lena_f"),
            "Sent an attachment"
        );
        assert_eq!(
            line(
                Channel,
                PreviewBody::Event("joined the conversation".into()),
                "lena_f"
            ),
            "Lena joined the conversation"
        );
        assert_eq!(
            line(Channel, PreviewBody::Deleted, "lena_f"),
            "Message deleted"
        );
        // Keybase's own snippet is shown as is, without a second name.
        assert_eq!(
            line(
                Channel,
                PreviewBody::Snippet("lena_f: hi :wave:".into()),
                ""
            ),
            "lena_f: hi 👋"
        );
    }

    #[test]
    fn rows_without_a_message_name_the_people_or_team_and_keep_their_time() {
        let names = HashMap::new();
        let names = PreviewNames {
            me: None,
            display_names: &names,
        };
        let mut channel = summary(ConversationKind::Channel, text(""), "lena_f");
        channel.last_message = None;
        let preview = sidebar_preview(&channel, &names);
        assert_eq!(
            (preview.prefix, preview.text.as_str()),
            (None, "lighthouse")
        );
        assert_eq!(preview.timestamp_ms, Some(10));
        let mut group = channel.clone();
        group.kind = ConversationKind::GroupDirectMessage;
        group.title = "devon,lena_f".into();
        group.last_activity_ms = 0;
        let preview = sidebar_preview(&group, &names);
        assert_eq!(preview.text, "@devon, @lena_f");
        assert_eq!(preview.timestamp_ms, None);
    }

    #[test]
    fn live_states_replace_the_preview_most_pressing_first() {
        let names = HashMap::from([
            ("lena_f".to_string(), "Lena Fischer".to_string()),
            ("sam".to_string(), "Sam Rivera".to_string()),
        ]);
        let names = PreviewNames {
            me: Some("me"),
            display_names: &names,
        };
        assert_eq!(
            names.typing_line(&["lena_f"]).as_deref(),
            Some("Lena is typing…")
        );
        assert_eq!(
            names.typing_line(&["lena_f", "me", "sam"]).as_deref(),
            Some("Lena and Sam are typing…")
        );
        assert_eq!(names.typing_line(&["ME"]), None);
        let typing = || names.typing_line(&["lena_f"]);
        assert_eq!(
            live_preview(typing(), Some("half a thought"), true, false),
            Some(PreviewLive::NotSent)
        );
        assert_eq!(
            live_preview(typing(), Some("half a thought"), false, false),
            Some(PreviewLive::Typing("Lena is typing…".into()))
        );
        assert_eq!(
            live_preview(None, Some("Ship **rc2**\nafter lunch"), false, false),
            Some(PreviewLive::Draft("Ship rc2 after lunch".into()))
        );
        // The open conversation shows its draft in the composer, not here.
        assert_eq!(live_preview(None, Some("Ship rc2"), false, true), None);
        assert_eq!(live_preview(None, Some("  "), false, false), None);
    }
}
