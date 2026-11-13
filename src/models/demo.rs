// Explicit test/benchmark fixtures. This module is excluded from normal builds.
//
// Everything here is made up: a fictional product team ("lighthouse") building a
// desktop app, plus a few friends and DMs. The data is shaped like what the Keybase
// backend produces (usernames as user ids, numeric message ids, replies inline in
// the main timeline) so the demo exercises the same rendering paths as real use.
use super::*;
use super::{
    notifications_model::{ActivityItem, ActivityKind},
    sidebar_model::{CONVERSATIONS_SECTION_ID, SidebarRow, SidebarSection},
    timeline_model::{DayDividers, MessageRow},
};
use crate::domain::{
    call::CallSessionSummary,
    conversation::{ConversationGroup, ConversationTeamType},
    ids::CallId,
    message::MessageReaction,
    message_formatting::push_text_fragment,
    message_preview::{MessagePreview, PreviewBody},
    presence::{Availability, Presence},
    profile::{
        IdentityProof, ProfileSection, ProofState, SocialGraph, SocialGraphEntry,
        SocialGraphListType, TeamShowcaseEntry, UserProfile,
    },
    user::UserSummary,
};
use crate::util::formatting::typing_indicator_label;
use chrono::{Local, NaiveTime, TimeZone};

pub(crate) const DEMO_WORKSPACE_ID: &str = "ws_lighthouse";
pub(crate) const DEMO_ME: &str = "robin";
pub(crate) const DEMO_GENERAL_ID: &str = "lighthouse.general";
/// Someone typing in #general, so the typing indicator shows in captures.
pub(crate) const DEMO_TYPING_USER: &str = "lena_f";
pub(crate) const DEMO_DESIGN_ID: &str = "lighthouse.design";
pub(crate) const DEMO_RELEASES_ID: &str = "lighthouse.releases";
pub(crate) const DEMO_RANDOM_ID: &str = "lighthouse.random";
pub(crate) const DEMO_BOOKCLUB_ID: &str = "bookclub.general";
pub(crate) const DEMO_DM_ID: &str = "dm.alicej";
pub(crate) const DEMO_DM_SAM_ID: &str = "dm.samr";
pub(crate) const DEMO_GROUP_DM_ID: &str = "gdm.devon-lena_f";
/// The #general message with the most thread replies.
pub(crate) const DEMO_THREAD_ROOT_ID: &str = "1010";
/// The person shown by the `profile` demo scene.
pub(crate) const DEMO_PROFILE_USER: &str = "alicej";
pub(crate) const DEMO_TEAM_ID: &str = "lighthouse";
/// A message nobody has replied to, for the empty thread state.
pub(crate) const DEMO_UNTHREADED_MESSAGE_ID: &str = "1017";
pub(crate) const DEMO_SEARCH_QUERY: &str = "release";
/// A query nothing in the demo matches, for the empty search state.
pub(crate) const DEMO_SEARCH_EMPTY_QUERY: &str = "quarterly roadmap";

pub(crate) struct DemoPerson {
    pub username: &'static str,
    pub display_name: &'static str,
    pub title: &'static str,
    pub avatar_asset: Option<&'static str>,
    pub availability: Availability,
    pub status: Option<&'static str>,
    pub bio: &'static str,
    pub location: &'static str,
    pub affinity: Affinity,
}

pub(crate) fn demo_people() -> Vec<DemoPerson> {
    vec![
        DemoPerson {
            username: DEMO_ME,
            display_name: "Robin Ellis",
            title: "Desktop Engineer",
            avatar_asset: Some("assets/avatars/me.svg"),
            availability: Availability::Active,
            status: Some("Shipping 2.4"),
            bio: "Desktop app, build tooling, and too many keyboard shortcuts.",
            location: "Lisbon",
            affinity: Affinity::None,
        },
        DemoPerson {
            username: "alicej",
            display_name: "Alice Johnson",
            title: "Product Designer",
            avatar_asset: Some("assets/avatars/alice.svg"),
            availability: Availability::Active,
            status: Some("Onboarding redesign"),
            bio: "Designing the Lighthouse desktop app. Sketchbook always within reach.",
            location: "Toronto",
            affinity: Affinity::Positive,
        },
        DemoPerson {
            username: "samr",
            display_name: "Sam Rivera",
            title: "Backend Engineer",
            avatar_asset: Some("assets/avatars/sam.svg"),
            availability: Availability::Active,
            status: Some("Profiling cold start"),
            bio: "Sync engine and storage. Ask me about indexes.",
            location: "Mexico City",
            affinity: Affinity::Positive,
        },
        DemoPerson {
            username: "priya",
            display_name: "Priya Natarajan",
            title: "Engineering Manager",
            avatar_asset: Some("assets/avatars/priya.svg"),
            availability: Availability::Away,
            status: Some("Out from 2pm"),
            bio: "Herding releases.",
            location: "Bengaluru",
            affinity: Affinity::None,
        },
        DemoPerson {
            username: "lena_f",
            display_name: "Lena Fischer",
            title: "Frontend Engineer",
            avatar_asset: Some("assets/avatars/lena.svg"),
            availability: Availability::Active,
            status: None,
            bio: "UI components and accessibility.",
            location: "Hamburg",
            affinity: Affinity::None,
        },
        DemoPerson {
            username: "devon",
            display_name: "Devon Brooks",
            title: "QA Lead",
            avatar_asset: None,
            availability: Availability::DoNotDisturb,
            status: Some("Regression pass"),
            bio: "If it can break, I will find it.",
            location: "Chicago",
            affinity: Affinity::None,
        },
        DemoPerson {
            username: "marco",
            display_name: "Marco Bellini",
            title: "Customer Support",
            avatar_asset: None,
            availability: Availability::Offline,
            status: None,
            bio: "Support team. Bringing the tickets to you.",
            location: "Milan",
            affinity: Affinity::None,
        },
    ]
}

fn demo_person(username: &str) -> Option<DemoPerson> {
    demo_people()
        .into_iter()
        .find(|person| person.username.eq_ignore_ascii_case(username))
}

/// People other than the demo user whose username or name contains `query`.
pub(crate) fn demo_user_search(query: &str) -> Vec<UserSummary> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    demo_people()
        .into_iter()
        .filter(|person| person.username != DEMO_ME)
        .filter(|person| {
            person.username.contains(&query) || person.display_name.to_lowercase().contains(&query)
        })
        .map(|person| demo_user_summary(&UserId::new(person.username)))
        .collect()
}

/// A query for the new-chat scene and the person already added to it.
pub(crate) const DEMO_NEW_CHAT_QUERY: &str = "li";
/// Members filter that matches nobody in #general.
pub(crate) const DEMO_MEMBERS_EMPTY_FILTER: &str = "quinn";
/// Composer draft for the `composer-format` scene, and the words selected in it.
pub(crate) const DEMO_FORMAT_DRAFT: &str = "Ship rc1 once the smoke test passes";
pub(crate) const DEMO_FORMAT_SELECTION: &str = "smoke test";
/// Composer draft for the `composer-autocomplete` scene; it ends in a bare `@`.
pub(crate) const DEMO_AUTOCOMPLETE_DRAFT: &str = "Release notes are up for review\nThanks @";
pub(crate) const DEMO_NEW_CHAT_PICK: &str = "samr";

pub(crate) fn demo_user_summary(user_id: &UserId) -> UserSummary {
    match demo_person(&user_id.0) {
        Some(person) => UserSummary {
            id: UserId::new(person.username),
            display_name: person.display_name.to_string(),
            title: person.title.to_string(),
            avatar_asset: person.avatar_asset.map(str::to_string),
            presence: Presence {
                availability: person.availability,
                status_text: person.status.map(str::to_string),
            },
            affinity: person.affinity,
        },
        None => UserSummary {
            id: user_id.clone(),
            display_name: user_id.0.clone(),
            title: String::new(),
            avatar_asset: None,
            presence: Presence {
                availability: Availability::Unknown,
                status_text: None,
            },
            affinity: Affinity::None,
        },
    }
}

fn social_entry(username: &str) -> SocialGraphEntry {
    let summary = demo_user_summary(&UserId::new(username));
    SocialGraphEntry {
        user_id: summary.id,
        display_name: summary.display_name,
        avatar_asset: summary.avatar_asset,
        affinity: summary.affinity,
    }
}

pub(crate) fn demo_social_graph(
    user_id: &UserId,
    list_type: SocialGraphListType,
) -> Vec<SocialGraphEntry> {
    let others = demo_people()
        .into_iter()
        .map(|person| person.username)
        .filter(|username| !username.eq_ignore_ascii_case(&user_id.0))
        .collect::<Vec<_>>();
    let take = match list_type {
        SocialGraphListType::Followers => others.len(),
        SocialGraphListType::Following => 4,
    };
    others.into_iter().take(take).map(social_entry).collect()
}

pub(crate) fn demo_user_profile(user_id: &UserId) -> Option<UserProfile> {
    let person = demo_person(&user_id.0)?;
    let summary = demo_user_summary(user_id);
    let followers = demo_social_graph(user_id, SocialGraphListType::Followers);
    let following = demo_social_graph(user_id, SocialGraphListType::Following);
    Some(UserProfile {
        user_id: summary.id.clone(),
        username: person.username.to_string(),
        display_name: person.display_name.to_string(),
        avatar_asset: summary.avatar_asset.clone(),
        presence: summary.presence.clone(),
        affinity: person.affinity,
        bio: Some(person.bio.to_string()),
        location: Some(person.location.to_string()),
        title: Some(person.title.to_string()),
        sections: vec![
            ProfileSection::IdentityProofs(vec![
                IdentityProof {
                    service_name: "github".to_string(),
                    service_username: format!("{}-dev", person.username),
                    proof_url: None,
                    site_url: None,
                    icon_asset: None,
                    state: ProofState::Verified,
                },
                IdentityProof {
                    service_name: "dns".to_string(),
                    service_username: format!("{}.example.com", person.username),
                    proof_url: None,
                    site_url: None,
                    icon_asset: None,
                    state: ProofState::Verified,
                },
            ]),
            ProfileSection::SocialGraph(SocialGraph {
                followers_count: Some(followers.len() as u32),
                following_count: Some(following.len() as u32),
                is_following_you: true,
                you_are_following: person.affinity == Affinity::Positive,
                followers: Some(followers),
                following: Some(following),
            }),
            ProfileSection::TeamShowcase(vec![TeamShowcaseEntry {
                name: DEMO_TEAM_ID.to_string(),
                description: "The team behind the Lighthouse desktop app.".to_string(),
                is_open: false,
                members_count: demo_people().len() as u32,
            }]),
        ],
    })
}

pub(crate) fn demo_workspace_id() -> WorkspaceId {
    WorkspaceId::new(DEMO_WORKSPACE_ID)
}

pub(crate) fn demo_route_for_conversation(conversation_id: &ConversationId) -> Route {
    let is_dm = demo_direct_messages()
        .iter()
        .any(|summary| &summary.id == conversation_id);
    if is_dm {
        Route::DirectMessage {
            workspace_id: demo_workspace_id(),
            dm_id: DmId::new(conversation_id.0.clone()),
        }
    } else {
        Route::Channel {
            workspace_id: demo_workspace_id(),
            channel_id: ChannelId::new(conversation_id.0.clone()),
        }
    }
}

fn lighthouse_group() -> ConversationGroup {
    ConversationGroup {
        id: DEMO_TEAM_ID.to_string(),
        display_name: DEMO_TEAM_ID.to_string(),
        team_type: ConversationTeamType::Complex,
    }
}

fn northwind_group() -> ConversationGroup {
    ConversationGroup {
        id: "northwind".to_string(),
        display_name: "northwind".to_string(),
        team_type: ConversationTeamType::Complex,
    }
}

fn channel(
    id: &str,
    title: &str,
    topic: &str,
    group: ConversationGroup,
    unread_count: u32,
    mention_count: u32,
    last_activity_ms: i64,
) -> ConversationSummary {
    ConversationSummary {
        id: ConversationId::new(id),
        title: title.to_string(),
        kind: ConversationKind::Channel,
        topic: topic.to_string(),
        group: Some(group),
        unread_count,
        mention_count,
        muted: false,
        last_activity_ms,
        last_message: None,
    }
}

/// The newest message of every demo conversation, as the inbox reports it. Each
/// matches the conversation's activity time and, where the demo has a timeline,
/// its last message.
fn demo_last_message(conversation_id: &str) -> Option<MessagePreview> {
    let text = |value: &str| PreviewBody::Text(value.to_string());
    let (id, author, (days_ago, hour, minute), body) = match conversation_id {
        DEMO_GENERAL_ID => (
            1017,
            "devon",
            (0, 10, 40),
            text("Ping me when the build is up and I'll run the smoke test."),
        ),
        DEMO_DESIGN_ID => (
            3004,
            "priya",
            (0, 8, 50),
            text(
                "Release banner is final :tada: The export lives in `assets/release/banner@2x.png`, *please* keep that name.",
            ),
        ),
        DEMO_RELEASES_ID => (
            4012,
            "samr",
            (1, 11, 5),
            PreviewBody::Attachment {
                kind: AttachmentKind::File,
                name: "rc1-checklist.pdf".to_string(),
                caption: None,
            },
        ),
        DEMO_RANDOM_ID => (
            5031,
            "marco",
            (0, 8, 12),
            text("Anyone up for ramen at 12:30? The new place on 5th opened :ramen:"),
        ),
        DEMO_BOOKCLUB_ID => (
            6020,
            "lena_f",
            (0, 7, 15),
            text("Chapter 9 moved to **Thursday**. Bring snacks :cookie:"),
        ),
        DEMO_DM_ID => (
            2006,
            "alicej",
            (0, 10, 30),
            text("Yes! 1:30 works. I'll bring the updated build."),
        ),
        DEMO_DM_SAM_ID => (
            7003,
            DEMO_ME,
            (1, 18, 2),
            text("Thanks! Merging once CI is green :+1:"),
        ),
        DEMO_GROUP_DM_ID => (
            8014,
            "devon",
            (2, 16, 45),
            text("Booked the big room for Friday's retro, bring your sticky notes"),
        ),
        "northwind.general" => (
            9007,
            "priya",
            (0, 9, 5),
            text("Welcome to the Northwind pilot, everyone :wave:"),
        ),
        "northwind.oncall" => (
            9104,
            "devon",
            (0, 9, 48),
            text("Pager is quiet. Handing over to Sam at noon."),
        ),
        "northwind.deploys" => (9203, "samr", (1, 15, 20), text("Staging is on 2.4.0-rc1")),
        "dm.priya" => (
            9301,
            "priya",
            (0, 9, 12),
            text("Can you review the onboarding copy before lunch?"),
        ),
        "dm.lena_f" => (
            9402,
            DEMO_ME,
            (1, 17, 40),
            text("Sending the icon exports tonight"),
        ),
        "dm.marco" => (
            9503,
            "marco",
            (2, 11, 15),
            text("Customer asked about dark mode again :sweat_smile:"),
        ),
        "dm.devon" => (
            9604,
            "devon",
            (3, 14, 0),
            text("Smoke test passed on the Intel box"),
        ),
        "gdm.alicej-priya" => (
            9705,
            "alicej",
            (4, 10, 22),
            text("Moving the design review to Thursday"),
        ),
        "gdm.marco-samr" => (9806, "samr", (6, 16, 5), text("Ticket closed, thanks both")),
        _ => return None,
    };
    Some(MessagePreview {
        message_id: Some(MessageId::new(id.to_string())),
        author_id: Some(UserId::new(author)),
        timestamp_ms: demo_time(days_ago, hour, minute),
        body,
    })
}

fn with_last_message(mut summaries: Vec<ConversationSummary>) -> Vec<ConversationSummary> {
    for summary in &mut summaries {
        summary.last_message = demo_last_message(&summary.id.0);
    }
    summaries
}

/// Fictional directory includes channels absent from the joined sidebar list.
pub(crate) fn demo_team_channels(
    team_name: &str,
) -> Vec<crate::domain::channel_details::TeamChannel> {
    use crate::domain::channel_details::TeamChannel;
    let mut channels = demo_channels()
        .into_iter()
        .filter(|c| c.group.as_ref().is_some_and(|g| g.id == team_name))
        .map(|conversation| TeamChannel {
            conversation,
            joined: true,
        })
        .collect::<Vec<_>>();
    if team_name == DEMO_TEAM_ID {
        for (name, topic) in [
            ("engineering", "Engineering discussions"),
            ("support", "Customer questions and help"),
        ] {
            channels.push(TeamChannel {
                conversation: channel(
                    &format!("{DEMO_TEAM_ID}.{name}"),
                    name,
                    topic,
                    lighthouse_group(),
                    0,
                    0,
                    demo_time(1, 9, 0),
                ),
                joined: false,
            });
        }
    }
    channels
}

pub(crate) fn demo_channels() -> Vec<ConversationSummary> {
    with_last_message(vec![
        channel(
            DEMO_GENERAL_ID,
            "general",
            "Lighthouse team: releases, standups, and everything else",
            lighthouse_group(),
            3,
            1,
            demo_time(0, 10, 40),
        ),
        channel(
            DEMO_DESIGN_ID,
            "design",
            "Design crits, mocks, and the UI kit",
            lighthouse_group(),
            0,
            0,
            demo_time(0, 8, 50),
        ),
        channel(
            DEMO_RELEASES_ID,
            "releases",
            "Release checklists and build status",
            lighthouse_group(),
            0,
            0,
            demo_time(1, 11, 5),
        ),
        channel(
            DEMO_RANDOM_ID,
            "random",
            "Lunch spots, pets, and links",
            lighthouse_group(),
            2,
            1,
            demo_time(0, 8, 12),
        ),
        channel(
            "northwind.general",
            "general",
            "Northwind pilot: everything else",
            northwind_group(),
            0,
            0,
            demo_time(0, 9, 5),
        ),
        channel(
            "northwind.oncall",
            "oncall",
            "Pager handovers",
            northwind_group(),
            2,
            0,
            demo_time(0, 9, 48),
        ),
        channel(
            "northwind.deploys",
            "deploys",
            "What runs where",
            northwind_group(),
            0,
            0,
            demo_time(1, 15, 20),
        ),
        channel(
            DEMO_BOOKCLUB_ID,
            "general",
            "Monthly book club",
            ConversationGroup {
                id: "bookclub".to_string(),
                display_name: "bookclub".to_string(),
                team_type: ConversationTeamType::Simple,
            },
            5,
            0,
            demo_time(0, 7, 15),
        ),
    ])
}

pub(crate) fn demo_direct_messages() -> Vec<ConversationSummary> {
    let dm = |id: &str, title: &str, kind, unread_count, last_activity_ms| ConversationSummary {
        id: ConversationId::new(id),
        title: title.to_string(),
        kind,
        topic: String::new(),
        group: None,
        unread_count,
        mention_count: 0,
        muted: false,
        last_activity_ms,
        last_message: None,
    };
    with_last_message(vec![
        dm(
            DEMO_DM_ID,
            "alicej",
            ConversationKind::DirectMessage,
            1,
            demo_time(0, 10, 30),
        ),
        dm(
            DEMO_DM_SAM_ID,
            "samr",
            ConversationKind::DirectMessage,
            0,
            demo_time(1, 18, 2),
        ),
        dm(
            DEMO_GROUP_DM_ID,
            "devon,lena_f",
            ConversationKind::GroupDirectMessage,
            0,
            demo_time(2, 16, 45),
        ),
        dm(
            "dm.priya",
            "priya",
            ConversationKind::DirectMessage,
            0,
            demo_time(0, 9, 12),
        ),
        dm(
            "dm.lena_f",
            "lena_f",
            ConversationKind::DirectMessage,
            0,
            demo_time(1, 17, 40),
        ),
        dm(
            "dm.marco",
            "marco",
            ConversationKind::DirectMessage,
            0,
            demo_time(2, 11, 15),
        ),
        dm(
            "dm.devon",
            "devon",
            ConversationKind::DirectMessage,
            2,
            demo_time(3, 14, 0),
        ),
        dm(
            "gdm.alicej-priya",
            "alicej,priya",
            ConversationKind::GroupDirectMessage,
            0,
            demo_time(4, 10, 22),
        ),
        dm(
            "gdm.marco-samr",
            "marco,samr",
            ConversationKind::GroupDirectMessage,
            0,
            demo_time(6, 16, 5),
        ),
    ])
}

fn find_demo_conversation(conversation_id: &ConversationId) -> Option<ConversationSummary> {
    demo_channels()
        .into_iter()
        .chain(demo_direct_messages())
        .find(|summary| &summary.id == conversation_id)
}

pub(crate) fn demo_conversation_members(conversation_id: &ConversationId) -> Vec<UserId> {
    let usernames: Vec<&str> = match conversation_id.0.as_str() {
        DEMO_DM_ID => vec![DEMO_ME, "alicej"],
        DEMO_DM_SAM_ID => vec![DEMO_ME, "samr"],
        DEMO_GROUP_DM_ID => vec![DEMO_ME, "devon", "lena_f"],
        DEMO_BOOKCLUB_ID => vec![DEMO_ME, "lena_f", "marco"],
        "dm.priya" => vec![DEMO_ME, "priya"],
        "dm.lena_f" => vec![DEMO_ME, "lena_f"],
        "dm.marco" => vec![DEMO_ME, "marco"],
        "dm.devon" => vec![DEMO_ME, "devon"],
        "gdm.alicej-priya" => vec![DEMO_ME, "alicej", "priya"],
        "gdm.marco-samr" => vec![DEMO_ME, "marco", "samr"],
        DEMO_DESIGN_ID => vec![DEMO_ME, "alicej", "lena_f", "priya"],
        _ => demo_people().iter().map(|person| person.username).collect(),
    };
    usernames.into_iter().map(UserId::new).collect()
}

pub(crate) fn demo_team_id_for_conversation(
    conversation_id: &ConversationId,
) -> Option<&'static str> {
    let team = conversation_id.0.split_once('.')?.0;
    ["lighthouse", "northwind"]
        .into_iter()
        .find(|known| *known == team)
}

pub(crate) fn demo_team_roles() -> Vec<(UserId, crate::state::event::TeamRoleKind)> {
    use crate::state::event::TeamRoleKind;
    demo_people()
        .iter()
        .map(|person| {
            let role = match person.username {
                "priya" => TeamRoleKind::Owner,
                "alicej" => TeamRoleKind::Admin,
                _ => TeamRoleKind::Member,
            };
            (UserId::new(person.username), role)
        })
        .collect()
}

/// Last message the current user has read; later messages from others are unread.
pub(crate) fn demo_read_marker(conversation_id: &ConversationId) -> Option<MessageId> {
    match conversation_id.0.as_str() {
        DEMO_GENERAL_ID => Some(MessageId::new("1014")),
        DEMO_DM_ID => Some(MessageId::new("2005")),
        _ => None,
    }
}

/// Local wall-clock time `days_ago` days back, so labels read "Yesterday" / today.
fn demo_time(days_ago: i64, hour: u32, minute: u32) -> i64 {
    let date = Local::now().date_naive() - chrono::Duration::days(days_ago);
    let time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or_default();
    Local
        .from_local_datetime(&date.and_time(time))
        .earliest()
        .map(|value| value.timestamp_millis())
        .unwrap_or_default()
}

/// One piece of message markup, mirroring what the Keybase adapter emits.
enum Part<'a> {
    /// Plain markdown: inline code, fenced code and quotes are parsed.
    Text(&'a str),
    Mention(&'a str),
    Channel(&'a str),
    Link(&'a str),
    Emoji(&'a str),
}

fn body(parts: &[Part<'_>]) -> (Vec<MessageFragment>, String) {
    let mut fragments = Vec::new();
    let mut source = String::new();
    for part in parts {
        match part {
            Part::Text(text) => {
                push_text_fragment(&mut fragments, text);
                source.push_str(text);
            }
            Part::Mention(username) => {
                fragments.push(MessageFragment::Mention(UserId::new(*username)));
                source.push('@');
                source.push_str(username);
            }
            Part::Channel(name) => {
                fragments.push(MessageFragment::ChannelMention {
                    name: name.to_string(),
                });
                source.push('#');
                source.push_str(name);
            }
            Part::Link(url) => {
                fragments.push(MessageFragment::Link {
                    url: url.to_string(),
                    display: url.to_string(),
                });
                source.push_str(url);
            }
            Part::Emoji(alias) => {
                fragments.push(MessageFragment::Emoji {
                    alias: alias.to_string(),
                    source_ref: None,
                });
                source.push(':');
                source.push_str(alias);
                source.push(':');
            }
        }
    }
    (fragments, source)
}

struct DemoMessage<'a> {
    id: u64,
    author: &'a str,
    at: (i64, u32, u32),
    parts: Vec<Part<'a>>,
    reactions: Vec<(&'a str, Vec<&'a str>)>,
    thread_root: Option<u64>,
    thread_reply_count: u32,
    edited: bool,
    attachments: Vec<AttachmentSummary>,
}

impl<'a> DemoMessage<'a> {
    fn new(id: u64, author: &'a str, at: (i64, u32, u32), parts: Vec<Part<'a>>) -> Self {
        Self {
            id,
            author,
            at,
            parts,
            reactions: Vec::new(),
            thread_root: None,
            thread_reply_count: 0,
            edited: false,
            attachments: Vec::new(),
        }
    }

    fn react(mut self, emoji: &'a str, actors: &[&'a str]) -> Self {
        self.reactions.push((emoji, actors.to_vec()));
        self
    }

    fn reply_to(mut self, root: u64) -> Self {
        self.thread_root = Some(root);
        self
    }

    fn replies(mut self, count: u32) -> Self {
        self.thread_reply_count = count;
        self
    }

    fn edited(mut self) -> Self {
        self.edited = true;
        self
    }

    fn attach(mut self, attachment: AttachmentSummary) -> Self {
        self.attachments.push(attachment);
        self
    }

    fn record(self, conversation_id: &str) -> MessageRecord {
        let (fragments, source_text) = body(&self.parts);
        let (days_ago, hour, minute) = self.at;
        let timestamp_ms = demo_time(days_ago, hour, minute);
        let id = MessageId::new(self.id.to_string());
        let root = self
            .thread_root
            .map(|root| MessageId::new(root.to_string()));
        MessageRecord {
            bot_username: None,
            permalink: format!("keybase://chat/{conversation_id}/{}", id.0),
            id,
            conversation_id: ConversationId::new(conversation_id),
            author_id: UserId::new(self.author),
            reply_to: root.clone(),
            thread_root_id: root,
            timestamp_ms: Some(timestamp_ms),
            event: None,
            link_previews: Vec::new(),
            fragments,
            source_text: Some(source_text),
            attachments: self.attachments,
            reactions: self
                .reactions
                .into_iter()
                .map(|(emoji, actors)| MessageReaction {
                    emoji: format!(":{emoji}:"),
                    source_ref: None,
                    actor_ids: actors.into_iter().map(UserId::new).collect(),
                })
                .collect(),
            thread_reply_count: self.thread_reply_count,
            send_state: MessageSendState::Sent,
            edited: self.edited.then(|| EditMeta {
                edit_id: MessageId::new(format!("{}", self.id + 500)),
                edited_at_ms: Some(timestamp_ms + 4 * 60 * 1000),
            }),
        }
    }
}

const RUST_SNIPPET: &str = "```rust
let index = SearchIndex::open(&store)?;
if index.schema_version() != SCHEMA_VERSION {
    index.rebuild(&store)?;
}
```";

fn general_messages() -> Vec<MessageRecord> {
    use Part::*;
    let y = |hour, minute| (1, hour, minute);
    let t = |hour, minute| (0, hour, minute);
    vec![
        DemoMessage::new(1001, "marco", y(15, 40), vec![Text(
            "Heads up from support: three tickets this week about CSV export dropping the header row. Repro steps are in the queue.",
        )]),
        DemoMessage::new(1002, DEMO_ME, y(15, 52), vec![Text(
            "I can pick that up. Looks like the exporter skips the first line when the file starts with a BOM.",
        )]),
        DemoMessage::new(1003, "marco", y(15, 53), vec![Emoji("pray")]),
        DemoMessage::new(1004, "priya", y(16, 2), vec![Text(
            "Reminder: 2.4 code freeze is tomorrow at noon. If something still needs to land, flag it here today.",
        )]),
        DemoMessage::new(1005, "priya", y(16, 3), vec![
            Text("Release notes draft: "),
            Link("https://docs.lighthouse.example/notes/2.4"),
            Text("\nPlease skim the section for your area before EOD."),
        ]),
        DemoMessage::new(1006, "samr", y(16, 20), vec![Text(
            "The sync migration is merged. Run `lh migrate --dry-run` before upgrading staging, it prints the tables it will touch.",
        )])
        .react("+1", &["priya", DEMO_ME, "devon"]),
        DemoMessage::new(1007, "lena_f", y(16, 48), vec![Text(
            "Is staging already on the new schema?",
        )]),
        DemoMessage::new(1008, "samr", y(16, 50), vec![Text(
            "Not yet, I'll upgrade it first thing tomorrow.",
        )]),
        DemoMessage::new(1009, "devon", t(9, 12), vec![
            Text("Linux tray fix is verified on Ubuntu 24.04 and Fedora 40 "),
            Emoji("white_check_mark"),
        ])
        .react("tada", &["priya", "alicej", "samr"]),
        DemoMessage::new(1010, "alicej", t(9, 20), vec![Text(
            "Final onboarding mocks are up. The empty state now points to Import instead of a blank canvas.\nCopy review before Thursday, please!",
        )])
        .attach(AttachmentSummary {
            name: "onboarding-v3.fig".to_string(),
            kind: AttachmentKind::File,
            size_bytes: 2_480_000,
            ..AttachmentSummary::default()
        })
        .react("eyes", &["lena_f", DEMO_ME])
        .replies(2),
        DemoMessage::new(1011, "lena_f", t(9, 26), vec![Text(
            "Love it. Maybe \"Start from scratch\" instead of \"Skip\"?",
        )])
        .reply_to(1010)
        .react("heart", &["alicej"]),
        DemoMessage::new(1012, DEMO_ME, t(9, 31), vec![Text(
            "+1 to that. Also the illustration crops below 1280px wide, sending a screenshot.",
        )])
        .reply_to(1010),
        DemoMessage::new(1013, "samr", t(9, 45), vec![Text(&format!(
            "Found the slow cold start. We rebuilt the search index on every launch because the version check was missing:\n{RUST_SNIPPET}"
        ))]),
        DemoMessage::new(1014, "samr", t(9, 46), vec![Text(
            "Cold start goes from 4.1s to 1.3s on my machine.",
        )])
        .react("rocket", &["priya", "lena_f", "devon", DEMO_ME])
        .react("fire", &["alicej"]),
        DemoMessage::new(1015, "priya", t(10, 15), vec![
            Mention(DEMO_ME),
            Text(" can you take the release build today? I'm out from 2pm. Checklist is in "),
            Channel("releases"),
            Text("."),
        ]),
        DemoMessage::new(1016, "alicej", t(10, 22), vec![Text(
            "Final icon set is in. Dark variants use the `-dark` suffix.",
        )])
        .edited(),
        DemoMessage::new(1017, "devon", t(10, 40), vec![Text(
            "Ping me when the build is up and I'll run the smoke test.",
        )]),
    ]
    .into_iter()
    .map(|message| message.record(DEMO_GENERAL_ID))
    .collect()
}

fn design_messages() -> Vec<MessageRecord> {
    use Part::*;
    vec![
        DemoMessage::new(
            3001,
            "alicej",
            (1, 14, 0),
            vec![Text(
                "Crit at 3pm: settings redesign v2. Link is in the calendar invite.",
            )],
        ),
        DemoMessage::new(
            3002,
            "lena_f",
            (1, 14, 20),
            vec![Text(
                "Can we also look at the new toast styles? They feel heavy in dark mode.",
            )],
        ),
        DemoMessage::new(
            3003,
            "alicej",
            (1, 14, 22),
            vec![Text("Yes, adding it to the agenda.")],
        )
        .react("+1", &["lena_f"]),
        DemoMessage::new(
            3004,
            "priya",
            (0, 8, 50),
            vec![
                Text("Release banner is final "),
                Emoji("tada"),
                Text(
                    " The export lives in `assets/release/banner@2x.png`, *please* keep that name.",
                ),
            ],
        ),
    ]
    .into_iter()
    .map(|message| message.record(DEMO_DESIGN_ID))
    .collect()
}

fn dm_messages() -> Vec<MessageRecord> {
    use Part::*;
    vec![
        DemoMessage::new(2001, "alicej", (1, 18, 10), vec![Text(
            "Hey, quick one: could you try the onboarding prototype on a small laptop screen? I only have the big monitor here.",
        )]),
        DemoMessage::new(2002, DEMO_ME, (1, 18, 25), vec![Text(
            "Sure, I'll try it on the 13\" tomorrow morning.",
        )]),
        DemoMessage::new(2003, DEMO_ME, (0, 9, 40), vec![Text(
            "Tried it. Two things:\n1. The import card overflows below 800px height\n2. The close button overlaps the title in German",
        )]),
        DemoMessage::new(2004, "alicej", (0, 9, 52), vec![
            Text("German strikes again "),
            Emoji("sweat_smile"),
            Text(" thanks, fixing both."),
        ]),
        DemoMessage::new(2005, DEMO_ME, (0, 9, 55), vec![Text(
            "No rush. Are you around for a quick call after lunch?",
        )]),
        DemoMessage::new(2006, "alicej", (0, 10, 30), vec![Text(
            "Yes! 1:30 works. I'll bring the updated build.",
        )])
        .react("+1", &[DEMO_ME]),
    ]
    .into_iter()
    .map(|message| message.record(DEMO_DM_ID))
    .collect()
}

/// Client ids of the demo's in-flight sends; pending messages are keyed
/// `local:<client id>`, as the reducer names real ones.
/// The timeline orders pending messages by id, so these sort in send order.
pub(crate) const DEMO_FAILED_CLIENT_ID: &str = "demo-1-failed";
pub(crate) const DEMO_UPLOAD_CLIENT_ID: &str = "demo-2-upload";
pub(crate) const DEMO_PENDING_CLIENT_ID: &str = "demo-3-pending";
/// Bytes of the demo upload that have reached the server, of `DEMO_UPLOAD_BYTES`.
pub(crate) const DEMO_UPLOAD_SENT_BYTES: u64 = 1_740_000;
pub(crate) const DEMO_UPLOAD_BYTES: u64 = 4_200_000;

/// Your own messages in #general still on their way: one that failed to send, an
/// attachment mid-upload and a text that is pending. Ids follow the reducer's
/// pending-message scheme so send events find them.
pub(crate) fn demo_send_state_messages() -> Vec<MessageRecord> {
    use Part::*;
    let pending = |client_id: &str, message: DemoMessage, state: MessageSendState| {
        let mut record = message.record(DEMO_GENERAL_ID);
        record.id = MessageId::new(format!("local:{client_id}"));
        record.permalink = format!("zbase://pending/{client_id}");
        record.send_state = state;
        record
    };
    vec![
        pending(
            DEMO_FAILED_CLIENT_ID,
            DemoMessage::new(
                0,
                DEMO_ME,
                (0, 10, 46),
                vec![
                    Mention("devon"),
                    Text(" the smoke test checklist is pinned in "),
                    Channel("releases"),
                    Text("."),
                ],
            ),
            MessageSendState::Failed,
        ),
        pending(
            DEMO_UPLOAD_CLIENT_ID,
            DemoMessage::new(
                0,
                DEMO_ME,
                (0, 10, 47),
                vec![Text("Release notes for rc1:")],
            )
            .attach(AttachmentSummary {
                name: "release-notes-2.4.pdf".to_string(),
                kind: AttachmentKind::File,
                size_bytes: DEMO_UPLOAD_BYTES,
                ..AttachmentSummary::default()
            }),
            MessageSendState::Pending,
        ),
        pending(
            DEMO_PENDING_CLIENT_ID,
            DemoMessage::new(
                0,
                DEMO_ME,
                (0, 10, 48),
                vec![Text("Build 2.4.0-rc1 is up, starting the smoke test now.")],
            ),
            MessageSendState::Pending,
        ),
    ]
}

/// Main-timeline messages for a demo conversation, oldest first. Thread replies are
/// included inline, as the Keybase backend delivers them.
pub(crate) fn demo_conversation_messages(conversation_id: &ConversationId) -> Vec<MessageRecord> {
    match conversation_id.0.as_str() {
        DEMO_GENERAL_ID => general_messages(),
        DEMO_DESIGN_ID => design_messages(),
        DEMO_DM_ID
            if matches!(
                crate::app::demo::DemoScene::from_env(),
                crate::app::demo::DemoScene::FilesMedia
                    | crate::app::demo::DemoScene::FilesDocuments
            ) =>
        {
            demo_files_rows()
                .into_iter()
                .filter_map(|row| match row {
                    TimelineRow::Message(row) => Some(row.message),
                    _ => None,
                })
                .rev()
                .collect()
        }
        DEMO_DM_ID => dm_messages(),
        _ => Vec::new(),
    }
}

/// Root plus replies for a thread, oldest first.
pub(crate) fn demo_thread_messages(
    conversation_id: &ConversationId,
    root_id: &MessageId,
) -> Vec<MessageRecord> {
    demo_conversation_messages(conversation_id)
        .into_iter()
        .filter(|message| {
            &message.id == root_id || message.thread_root_id.as_ref() == Some(root_id)
        })
        .collect()
}

/// Builds view-model rows with the same grouping rules as the live store projection.
fn demo_timeline(conversation_id: &ConversationId) -> TimelineModel {
    const GROUP_WINDOW_MS: i64 = 15 * 60 * 1000;
    let messages = demo_conversation_messages(conversation_id);
    let read_marker = demo_read_marker(conversation_id);
    let mut rows = Vec::new();
    let mut previous: Option<&MessageRecord> = None;
    let mut unread_inserted = read_marker.is_none();
    let mut day_dividers = DayDividers::new(Local::now().date_naive());
    for message in &messages {
        let timestamp = message.timestamp_ms.unwrap_or_default();
        let divider = day_dividers.before(message.timestamp_ms);
        let new_day = divider.is_some();
        if let Some(label) = divider {
            rows.push(TimelineRow::DateDivider(label));
        }
        let unread_starts = !unread_inserted
            && message.author_id.0 != DEMO_ME
            && read_marker.as_ref().is_some_and(|marker| {
                message.id.0.parse::<u64>().ok() > marker.0.parse::<u64>().ok()
            });
        if unread_starts {
            rows.push(TimelineRow::UnreadDivider("New messages".to_string()));
            unread_inserted = true;
        }
        let show_header = new_day
            || unread_starts
            || message.reply_to.is_some()
            || previous.is_none_or(|previous| {
                previous.author_id != message.author_id
                    || previous.reply_to.is_some()
                    || timestamp - previous.timestamp_ms.unwrap_or_default() > GROUP_WINDOW_MS
            });
        rows.push(TimelineRow::Message(MessageRow {
            author: demo_user_summary(&message.author_id),
            message: message.clone(),
            show_header,
        }));
        previous = Some(message);
    }

    TimelineModel {
        conversation_id: conversation_id.clone(),
        current_user_id: Some(UserId::new(DEMO_ME)),
        highlighted_message_id: None,
        unread_marker: read_marker,
        older_cursor: None,
        typing_text: (conversation_id.0 == DEMO_GENERAL_ID)
            .then(|| {
                typing_indicator_label(&[
                    demo_user_summary(&UserId::new(DEMO_TYPING_USER)).display_name
                ])
            })
            .flatten(),
        rows,
        ..TimelineModel::default()
    }
}

impl AppModels {
    pub fn demo() -> Self {
        Self::demo_with_settings(SettingsModel::default())
    }

    pub fn demo_with_settings(settings: SettingsModel) -> Self {
        let sidebar_style = settings.sidebar_style;
        let workspace_id = demo_workspace_id();
        let general_id = ConversationId::new(DEMO_GENERAL_ID);
        let route = demo_route_for_conversation(&general_id);
        let channels = demo_channels();
        let direct_messages = demo_direct_messages();
        let general = find_demo_conversation(&general_id).expect("missing demo #general");
        let me = demo_user_summary(&UserId::new(DEMO_ME));

        let group_rows = |group_id: &str| {
            channels
                .iter()
                .filter(|summary| {
                    summary
                        .group
                        .as_ref()
                        .is_some_and(|group| group.id == group_id)
                })
                .map(|summary| SidebarRow {
                    team: summary.group.clone(),
                    label: summary.title.clone(),
                    unread_count: summary.unread_count,
                    mention_count: summary.mention_count,
                    route: demo_route_for_conversation(&summary.id),
                    preview: None,
                    muted: summary.muted,
                })
                .collect::<Vec<_>>()
        };
        let conversation_rows = direct_messages
            .iter()
            .map(|summary| SidebarRow {
                team: None,
                label: summary
                    .title
                    .split(',')
                    .map(|username| demo_user_summary(&UserId::new(username)).display_name)
                    .collect::<Vec<_>>()
                    .join(", "),
                unread_count: summary.unread_count,
                mention_count: summary.mention_count,
                route: demo_route_for_conversation(&summary.id),
                preview: None,
                muted: summary.muted,
            })
            .collect::<Vec<_>>();

        let composer = ComposerModel {
            conversation_id: general_id.clone(),
            mode: ComposerMode::Compose,
            draft_text: String::new(),
            attachments: Vec::new(),
            autocomplete: None,
        };
        let thread_pane = ThreadPaneModel {
            open: false,
            root_message_id: None,
            width_px: 360.0,
            following: true,
            replies: Vec::new(),
            reply_draft: String::new(),
            reply_attachments: Vec::new(),
            reply_autocomplete: None,
            loading: false,
        };

        let mut models = Self {
            fixture_mode: true,
            app: AppModel {
                open_workspaces: vec![workspace_id.clone()],
                active_workspace_id: workspace_id.clone(),
                connectivity: Connectivity::Online,
                global_unread_count: 4,
                current_user_id: Some(me.id.clone()),
                current_user_display_name: me.display_name.clone(),
                current_user_avatar_asset: me.avatar_asset.clone(),
            },
            workspace: WorkspaceModel {
                workspace_id: workspace_id.clone(),
                workspace_name: DEMO_ME.to_string(),
                channels: channels.clone(),
                direct_messages: direct_messages.clone(),
            },
            navigation: NavigationModel {
                current: route.clone(),
                back_stack: Vec::new(),
                forward_stack: Vec::new(),
                right_pane: RightPaneMode::Hidden,
            },
            sidebar: SidebarModel {
                mode: Default::default(),
                style: sidebar_style,
                recent_rows: Vec::new(),
                sections: vec![
                    SidebarSection {
                        id: SidebarSectionId::new(CONVERSATIONS_SECTION_ID),
                        title: "Conversations".to_string(),
                        rows: conversation_rows,
                        collapsed: false,
                    },
                    SidebarSection {
                        id: SidebarSectionId::new(format!("group_{DEMO_TEAM_ID}")),
                        title: DEMO_TEAM_ID.to_string(),
                        rows: group_rows(DEMO_TEAM_ID),
                        collapsed: false,
                    },
                ],
                filter: String::new(),
                highlighted_route: Some(route.clone()),
                width_px: crate::views::sidebar_width_for_style(sidebar_style),
                live_previews: Default::default(),
            },
            conversation: ConversationModel {
                participant_avatars: Vec::new(),
                summary: general.clone(),
                pinned_message: None,
                details: None,
                avatar_asset: None,
                member_count: demo_conversation_members(&general_id).len() as u32,
                can_post: true,
                is_archived: false,
            },
            timeline: demo_timeline(&general_id),
            composer,
            quick_switcher: QuickSwitcherModel::default(),
            emoji_picker: EmojiPickerModel {
                query: String::new(),
                selected_skin_tone: skin_tone_from_setting_value(
                    settings.emoji_skin_tone.as_deref(),
                ),
                active_group: None,
                hovered: None,
                recent_aliases: settings.emoji_recents.clone(),
                skin_tone_expanded: false,
            },
            find_in_chat: FindInChatModel::default(),
            search: SearchModel {
                query: String::new(),
                filters: Vec::new(),
                results: Vec::new(),
                submitted_query: None,
                highlighted_index: None,
                is_loading: false,
            },
            new_chat: NewChatModel::default(),
            thread_pane,
            profile_panel: ProfilePanelModel::default(),
            overlay: OverlayModel {
                new_chat_open: false,
                quick_switcher_open: false,
                command_palette_open: false,
                emoji_picker_open: false,
                reaction_target_message_id: None,
                fullscreen_image: None,
                file_upload_lightbox: None,
                active_modal: None,
                profile_card_user_id: None,
                profile_card_position: None,
                sidebar_hover_tooltip: None,
                reaction_hover_tooltip: None,
            },
            call: CallModel {
                active_call: None,
                is_muted: false,
                is_sharing_screen: false,
            },
            notifications: NotificationsModel {
                toasts: Vec::new(),
                notification_center_count: 2,
                activity_items: demo_activity_items(&workspace_id),
                highlighted_index: Some(0),
            },
            settings,
            quick_switcher_profile_names: HashMap::new(),
            quick_switcher_search_corpus: Arc::new(QuickSwitcherSearchCorpus {
                revision: 0,
                entries: Vec::new(),
                channel_entry_indices: Vec::new(),
                dm_entry_indices: Vec::new(),
            }),
            quick_switcher_search_corpus_revision: 0,
            quick_switcher_search_corpus_dirty: false,
            composer_drafts: HashMap::new(),
            thread_reply_drafts: HashMap::new(),
            pending_send_outcomes: HashMap::new(),
            send_attempt_count: 0,
        };

        models.store_current_composer_draft();
        models.store_current_thread_draft();
        models.refresh_notification_counts();
        models.apply_saved_sidebar_order();
        models.rebuild_quick_switcher_local_search_corpus();
        models
    }

    pub(super) fn sync_demo_route_models(&mut self, route: &Route) {
        self.find_in_chat.close();
        match route {
            Route::Channel {
                channel_id: ChannelId(id),
                workspace_id,
            }
            | Route::DirectMessage {
                dm_id: DmId(id),
                workspace_id,
            } => {
                self.app.active_workspace_id = workspace_id.clone();
                let conversation_id = ConversationId::new(id.clone());
                let is_dm = matches!(route, Route::DirectMessage { .. });
                let summary = find_demo_conversation(&conversation_id).unwrap_or_else(|| {
                    ConversationSummary {
                        id: conversation_id.clone(),
                        title: id.clone(),
                        kind: if is_dm {
                            ConversationKind::DirectMessage
                        } else {
                            ConversationKind::Channel
                        },
                        topic: String::new(),
                        group: None,
                        unread_count: 0,
                        mention_count: 0,
                        muted: false,
                        last_activity_ms: 0,
                        last_message: None,
                    }
                });
                self.conversation = ConversationModel {
                    participant_avatars: Vec::new(),
                    summary: summary.clone(),
                    pinned_message: None,
                    details: None,
                    avatar_asset: None,
                    member_count: demo_conversation_members(&conversation_id).len() as u32,
                    can_post: true,
                    is_archived: false,
                };
                self.timeline = demo_timeline(&conversation_id);
                self.composer = self.restore_composer_for_conversation(&summary, String::new());
            }
            Route::Search {
                query,
                workspace_id,
            } => {
                self.search.query = query.clone();
                self.search.results =
                    demo_search_results(query, &self.search.filters, workspace_id);
                self.search
                    .results
                    .sort_by(crate::domain::search::SearchResult::cmp_for_display);
                self.search.highlighted_index = (!self.search.results.is_empty()).then_some(0);
                self.set_right_pane(RightPaneMode::Hidden);
            }
            Route::Activity { .. } | Route::Preferences | Route::WorkspaceHome { .. } => {
                self.set_right_pane(RightPaneMode::Hidden);
            }
            Route::ActiveCall { .. } => {
                self.set_right_pane(RightPaneMode::Hidden);
                if self.call.active_call.is_none() {
                    let _ = self.start_or_open_call();
                }
            }
        }

        self.sidebar.highlighted_route = self
            .visible_sidebar_routes()
            .into_iter()
            .find(|candidate| candidate == route)
            .or_else(|| self.sidebar.highlighted_route.clone());

        self.notifications.highlighted_index = if self.notifications.activity_items.is_empty() {
            None
        } else {
            Some(
                self.notifications
                    .highlighted_index
                    .unwrap_or(0)
                    .min(self.notifications.activity_items.len().saturating_sub(1)),
            )
        };

        if self.timeline.highlighted_message_id.is_none() {
            self.timeline.highlighted_message_id = self.default_timeline_highlight();
        }
    }

    pub fn add_composer_attachment(&mut self) {
        if !self.fixture_mode {
            return;
        }
        self.composer.attachments.push(next_demo_attachment(
            self.composer.attachments.len(),
            "composer",
        ));
        self.store_current_composer_draft();
    }

    pub fn add_thread_attachment(&mut self) {
        if !self.fixture_mode {
            return;
        }
        self.thread_pane
            .reply_attachments
            .push(next_demo_attachment(
                self.thread_pane.reply_attachments.len(),
                "thread",
            ));
        self.store_current_thread_draft();
    }

    pub(super) fn start_demo_call(&mut self) -> Route {
        if let Some(route) = self.active_call_route() {
            return route;
        }

        let call_id = CallId::new(format!("call_{}", self.conversation.summary.title));
        self.call.active_call = Some(CallSessionSummary {
            call_id: call_id.clone(),
            status: if matches!(
                self.conversation.summary.kind,
                ConversationKind::DirectMessage
            ) {
                CallStatus::ActiveVideo
            } else {
                CallStatus::ActiveAudio
            },
            participants: active_call_participants(&self.conversation.summary),
        });
        self.call.is_muted = false;
        self.call.is_sharing_screen = false;
        self.push_toast("Call started", Some(ToastAction::OpenActiveCall));
        Route::ActiveCall {
            workspace_id: self.app.active_workspace_id.clone(),
            call_id,
        }
    }
}

pub(crate) fn demo_search_results(
    query: &str,
    filters: &[SearchFilter],
    workspace_id: &WorkspaceId,
) -> Vec<SearchResult> {
    let query = query.trim().to_lowercase();
    let conversations = [DEMO_GENERAL_ID, DEMO_DESIGN_ID, DEMO_DM_ID];
    let mut results = conversations
        .iter()
        .flat_map(|id| demo_conversation_messages(&ConversationId::new(*id)))
        .filter_map(|message| {
            let snippet = message
                .source_text
                .clone()
                .unwrap_or_else(|| flatten_fragments(&message.fragments));
            let haystack = snippet.to_lowercase();
            if !query.is_empty() && !haystack.contains(&query) {
                return None;
            }
            let snippet_highlight_ranges = if query.is_empty() {
                Vec::new()
            } else {
                haystack
                    .match_indices(&query)
                    .map(|(start, matched)| (start, start + matched.len()))
                    .collect()
            };
            let mut route = demo_route_for_conversation(&message.conversation_id);
            match &mut route {
                Route::Channel {
                    workspace_id: route_workspace,
                    ..
                }
                | Route::DirectMessage {
                    workspace_id: route_workspace,
                    ..
                } => *route_workspace = workspace_id.clone(),
                _ => {}
            }
            Some(SearchResult {
                conversation_id: message.conversation_id.clone(),
                route,
                snippet,
                snippet_highlight_ranges,
                message,
            })
        })
        .filter(|result| {
            filters
                .iter()
                .all(|filter| search_filter_matches(filter, result))
        })
        .collect::<Vec<_>>();
    // Newest first, like the live search index.
    results.sort_by_key(|left| std::cmp::Reverse(left.message.timestamp_ms));
    results
}

fn demo_activity_items(workspace_id: &WorkspaceId) -> Vec<ActivityItem> {
    let route = |id: &str| {
        let mut route = demo_route_for_conversation(&ConversationId::new(id));
        if let Route::Channel {
            workspace_id: route_workspace,
            ..
        }
        | Route::DirectMessage {
            workspace_id: route_workspace,
            ..
        } = &mut route
        {
            *route_workspace = workspace_id.clone();
        }
        route
    };
    vec![
        ActivityItem {
            kind: ActivityKind::Mention,
            title: "Priya mentioned you in #general".to_string(),
            detail: "Asked you to take the 2.4 release build today.".to_string(),
            route: route(DEMO_GENERAL_ID),
            message_id: Some(MessageId::new("1015")),
            unread: true,
        },
        ActivityItem {
            kind: ActivityKind::ThreadReply,
            title: "New replies in the onboarding thread".to_string(),
            detail: "Lena and Alice discussed the secondary button copy.".to_string(),
            route: route(DEMO_GENERAL_ID),
            message_id: Some(MessageId::new(DEMO_THREAD_ROOT_ID)),
            unread: true,
        },
        ActivityItem {
            kind: ActivityKind::Reminder,
            title: "Code freeze".to_string(),
            detail: "2.4 code freeze is today at noon.".to_string(),
            route: Route::Preferences,
            message_id: None,
            unread: false,
        },
        ActivityItem {
            kind: ActivityKind::Reaction,
            title: "Alice reacted in your DM".to_string(),
            detail: "Reacted with a thumbs up to your call time.".to_string(),
            route: route(DEMO_DM_ID),
            message_id: Some(MessageId::new("2006")),
            unread: false,
        },
    ]
}

fn search_filter_matches(filter: &SearchFilter, result: &SearchResult) -> bool {
    match filter {
        SearchFilter::FromUser(user) => {
            let author = demo_user_summary(&result.message.author_id);
            let needle = user.to_lowercase();
            author.id.0.to_lowercase().contains(&needle)
                || author.display_name.to_lowercase().contains(&needle)
        }
        SearchFilter::InChannel(channel) => match &result.route {
            Route::Channel { .. } => find_demo_conversation(&result.conversation_id)
                .is_some_and(|summary| summary.title.contains(&channel.to_lowercase())),
            Route::DirectMessage { .. } => channel == "dm",
            _ => false,
        },
        SearchFilter::HasFile => !result.message.attachments.is_empty(),
        SearchFilter::HasLink => result
            .message
            .fragments
            .iter()
            .any(|fragment| matches!(fragment, MessageFragment::Link { .. })),
        SearchFilter::MentionsMe => result.message.fragments.iter().any(
            |fragment| matches!(fragment, MessageFragment::Mention(user) if user.0 == DEMO_ME),
        ),
    }
}

fn next_demo_attachment(index: usize, prefix: &str) -> AttachmentSummary {
    let (name, kind, size_bytes) = match index % 3 {
        0 => (format!("{prefix}-notes.md"), AttachmentKind::File, 16_500),
        1 => (format!("{prefix}-mock.png"), AttachmentKind::Image, 248_000),
        _ => (
            format!("{prefix}-clip.mov"),
            AttachmentKind::Video,
            3_200_000,
        ),
    };

    AttachmentSummary {
        name,
        kind,
        size_bytes,
        ..AttachmentSummary::default()
    }
}

fn active_call_participants(conversation: &ConversationSummary) -> Vec<UserSummary> {
    let me = demo_user_summary(&UserId::new(DEMO_ME));
    let alice = demo_user_summary(&UserId::new("alicej"));
    let sam = demo_user_summary(&UserId::new("samr"));
    match conversation.kind {
        ConversationKind::DirectMessage => vec![me, alice],
        ConversationKind::Channel | ConversationKind::GroupDirectMessage => vec![me, alice, sam],
    }
}

/// Fictional attachments for offline Files captures. No user files or network URLs.
pub(crate) fn demo_files_rows() -> Vec<TimelineRow> {
    let samples = [
        ("onboarding-layout.png", AttachmentKind::Image),
        ("color-study.png", AttachmentKind::Image),
        ("mobile-layout.png", AttachmentKind::Image),
        ("navigation-study.png", AttachmentKind::Image),
        ("release-banner.png", AttachmentKind::Image),
        ("empty-state.png", AttachmentKind::Image),
        (
            "Lighthouse-onboarding-review-with-final-comments.pdf",
            AttachmentKind::File,
        ),
        ("design-tokens.json", AttachmentKind::File),
        ("prototype-source.zip", AttachmentKind::File),
        ("research-notes.txt", AttachmentKind::File),
    ];
    samples
        .into_iter()
        .enumerate()
        .map(|(i, (name, kind))| {
            let image = kind == AttachmentKind::Image;
            let source = image.then(|| {
                crate::domain::attachment::AttachmentSource::LocalPath(format!(
                    "{}/assets/demo/files/sample-{}.png",
                    env!("CARGO_MANIFEST_DIR"),
                    i % 3
                ))
            });
            let attachment = AttachmentSummary {
                name: name.into(),
                kind,
                size_bytes: 214_000 + i as u64 * 102_400,
                source,
                width: image.then_some(480),
                height: image.then_some(360),
                ..Default::default()
            };
            let mut message = DemoMessage::new(
                3000 + i as u64,
                "alicej",
                (0, 10, 30),
                vec![Part::Text("Here are the mock onboarding review files.")],
            )
            .attach(attachment)
            .record(DEMO_DM_ID);
            // Stable month boundaries and ordering in captures.
            message.timestamp_ms = Some(1794220200000 - i as i64 * 86400000 * 4);
            TimelineRow::Message(MessageRow {
                author: demo_user_summary(&UserId::new("alicej")),
                message,
                show_header: true,
            })
        })
        .collect()
}

/// Bot fixtures shared by the offline backend and native UI scenes.
pub(crate) fn demo_bot_snapshot(
    conversation_id: &ConversationId,
) -> crate::domain::bots::BotSnapshot {
    use crate::domain::bots::{BotCommand, BotInfo, BotSettings, BotSnapshot};
    let bot = BotInfo {
        username: "zoombot".into(),
        restricted: true,
        settings: Some(BotSettings {
            commands: true,
            mentions: true,
            conversations: vec![
                conversation_id
                    .0
                    .strip_prefix("kb_conv:")
                    .unwrap_or(&conversation_id.0)
                    .to_string(),
            ],
            ..Default::default()
        }),
    };
    BotSnapshot {
        bots: vec![
            bot.clone(),
            BotInfo {
                username: "buildbot".into(),
                restricted: true,
                settings: Some(BotSettings {
                    commands: true,
                    ..Default::default()
                }),
            },
        ],
        commands: vec![
            BotCommand {
                name: "zoom".into(),
                username: "zoombot".into(),
                description: "Create a Zoom meeting".into(),
                usage: "!zoom".into(),
            },
            BotCommand {
                name: "build".into(),
                username: "buildbot".into(),
                description: "Check the latest build".into(),
                usage: "!build [branch]".into(),
            },
        ],
        can_manage: true,
    }
}

pub(crate) fn demo_featured_bots() -> crate::domain::bots::FeaturedBotPage {
    use crate::domain::bots::{FeaturedBot, FeaturedBotPage};
    FeaturedBotPage {bots:vec![FeaturedBot {username:"zoombot".into(),name:"Zoom Meetings".into(),description:"Zoom video calls in chat".into(),extended_description:"Start a Zoom meeting with the people in this conversation. Type !zoom after adding the bot to get a meeting link everyone can join.".into(),owner:"keybase".into(),rank:100},FeaturedBot {username:"reminderbot".into(),name:"Reminders".into(),description:"Set reminders for your team".into(),rank:10,..Default::default()}],is_last_page:true}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_timeline_reads_like_a_real_conversation() {
        let general = ConversationId::new(DEMO_GENERAL_ID);
        let messages = demo_conversation_messages(&general);
        assert!((15..=25).contains(&messages.len()));
        let authors = messages
            .iter()
            .map(|message| message.author_id.0.as_str())
            .collect::<HashSet<_>>();
        assert!(authors.len() >= 5);
        for author in &authors {
            assert!(
                demo_person(author).is_some(),
                "{author} has no display name"
            );
        }
        let fragments = messages.iter().flat_map(|message| &message.fragments);
        let kinds = fragments.fold([false; 5], |mut kinds, fragment| {
            match fragment {
                MessageFragment::InlineCode(_) => kinds[0] = true,
                MessageFragment::Code { .. } => kinds[1] = true,
                MessageFragment::Link { .. } => kinds[2] = true,
                MessageFragment::Mention(user) if user.0 == DEMO_ME => kinds[3] = true,
                MessageFragment::Emoji { .. } => kinds[4] = true,
                _ => {}
            }
            kinds
        });
        assert_eq!(kinds, [true; 5]);
        assert!(messages.iter().any(|message| message.edited.is_some()));
        assert!(messages.iter().any(|message| {
            message
                .reactions
                .iter()
                .any(|reaction| reaction.actor_ids.iter().any(|actor| actor.0 == DEMO_ME))
        }));

        let model = demo_timeline(&general);
        let dividers = model
            .rows
            .iter()
            .filter_map(|row| match row {
                TimelineRow::DateDivider(label) | TimelineRow::UnreadDivider(label) => {
                    Some(label.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(dividers, ["Yesterday", "Today", "New messages"]);
        assert!(model.rows.iter().any(|row| matches!(
            row,
            TimelineRow::Message(row) if !row.show_header
        )));
    }

    #[test]
    fn every_demo_conversation_previews_its_newest_message() {
        for summary in demo_channels().into_iter().chain(demo_direct_messages()) {
            let preview = summary
                .last_message
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no preview", summary.id.0));
            assert_eq!(
                preview.timestamp_ms, summary.last_activity_ms,
                "{}",
                summary.id.0
            );
            if let Some(last) = demo_conversation_messages(&summary.id).last() {
                assert_eq!(
                    preview.message_id.as_ref(),
                    Some(&last.id),
                    "{}",
                    summary.id.0
                );
            }
        }
    }

    #[test]
    fn thread_root_has_its_replies() {
        let general = ConversationId::new(DEMO_GENERAL_ID);
        let root_id = MessageId::new(DEMO_THREAD_ROOT_ID);
        let thread = demo_thread_messages(&general, &root_id);
        let root = thread
            .iter()
            .find(|message| message.id == root_id)
            .expect("thread root");
        assert_eq!(root.thread_reply_count as usize, thread.len() - 1);
        assert!(thread.len() >= 3);
    }

    #[test]
    fn search_finds_release_messages_across_conversations() {
        let results = demo_search_results(DEMO_SEARCH_QUERY, &[], &demo_workspace_id());
        assert!(results.len() >= 2);
        assert!(results.iter().all(|result| {
            result.snippet.to_lowercase().contains(DEMO_SEARCH_QUERY)
                && !result.snippet_highlight_ranges.is_empty()
        }));
        let mentions = demo_search_results("", &[SearchFilter::MentionsMe], &demo_workspace_id());
        assert_eq!(mentions.len(), 1);
    }
}
