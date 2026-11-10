//! Cheap hashes of the store inputs each part of the model sync projects, so
//! `StoreSync` only rebuilds a model when its inputs changed.
use std::collections::HashMap;

use super::store_sync::team_avatar_asset;
use crate::domain::{
    ids::{ConversationId, MessageId, UserId},
    message::ChatEvent,
    route::Route,
};

const SIG_SEED: u64 = 0xcbf29ce484222325;
const SIG_MUL: u64 = 0x100000001b3;

fn mix_sig(sig: &mut u64, value: u64) {
    *sig = sig.wrapping_mul(SIG_MUL).wrapping_add(value);
}

fn mix_sig_bool(sig: &mut u64, value: bool) {
    mix_sig(sig, u64::from(value));
}

fn mix_sig_str(sig: &mut u64, value: &str) {
    mix_sig(sig, value.len() as u64);
    for byte in value.as_bytes() {
        *sig = sig.wrapping_mul(SIG_MUL) ^ u64::from(*byte);
    }
}

fn mix_sig_route(sig: &mut u64, route: &Route) {
    match route {
        Route::WorkspaceHome { workspace_id } => {
            mix_sig(sig, 1);
            mix_sig_str(sig, &workspace_id.0);
        }
        Route::Channel {
            workspace_id,
            channel_id,
        } => {
            mix_sig(sig, 2);
            mix_sig_str(sig, &workspace_id.0);
            mix_sig_str(sig, &channel_id.0);
        }
        Route::DirectMessage {
            workspace_id,
            dm_id,
        } => {
            mix_sig(sig, 3);
            mix_sig_str(sig, &workspace_id.0);
            mix_sig_str(sig, &dm_id.0);
        }
        Route::Search {
            workspace_id,
            query,
        } => {
            mix_sig(sig, 4);
            mix_sig_str(sig, &workspace_id.0);
            mix_sig_str(sig, query);
        }
        Route::Activity { workspace_id } => {
            mix_sig(sig, 5);
            mix_sig_str(sig, &workspace_id.0);
        }
        Route::Preferences => {
            mix_sig(sig, 6);
        }
        Route::ActiveCall {
            workspace_id,
            call_id,
        } => {
            mix_sig(sig, 7);
            mix_sig_str(sig, &workspace_id.0);
            mix_sig_str(sig, &call_id.0);
        }
    }
}

fn mix_sig_conversation_summary(
    sig: &mut u64,
    summary: &crate::domain::conversation::ConversationSummary,
) {
    mix_sig_str(sig, &summary.id.0);
    mix_sig_str(sig, &summary.title);
    mix_sig(
        sig,
        match summary.kind {
            crate::domain::conversation::ConversationKind::Channel => 1,
            crate::domain::conversation::ConversationKind::DirectMessage => 2,
            crate::domain::conversation::ConversationKind::GroupDirectMessage => 3,
        },
    );
    mix_sig_str(sig, &summary.topic);
    mix_sig(sig, summary.last_activity_ms as u64);
    match &summary.last_message {
        Some(preview) => {
            use crate::domain::message_preview::PreviewBody;
            mix_sig(sig, 1);
            mix_sig_str(
                sig,
                preview.message_id.as_ref().map_or("", |id| id.0.as_str()),
            );
            mix_sig_str(
                sig,
                preview.author_id.as_ref().map_or("", |id| id.0.as_str()),
            );
            mix_sig(sig, preview.timestamp_ms as u64);
            match &preview.body {
                PreviewBody::Text(text) => {
                    mix_sig(sig, 1);
                    mix_sig_str(sig, text);
                }
                PreviewBody::Attachment { name, caption, .. } => {
                    mix_sig(sig, 2);
                    mix_sig_str(sig, name);
                    mix_sig_str(sig, caption.as_deref().unwrap_or(""));
                }
                PreviewBody::Event(text) => {
                    mix_sig(sig, 3);
                    mix_sig_str(sig, text);
                }
                PreviewBody::Deleted => mix_sig(sig, 4),
                PreviewBody::Snippet(text) => {
                    mix_sig(sig, 5);
                    mix_sig_str(sig, text);
                }
            }
        }
        None => mix_sig(sig, 0),
    }
    mix_sig(sig, summary.unread_count as u64);
    mix_sig(sig, summary.mention_count as u64);
    mix_sig_bool(sig, summary.muted);
    match &summary.group {
        Some(group) => {
            mix_sig(sig, 1);
            mix_sig_str(sig, &group.id);
            mix_sig_str(sig, &group.display_name);
            mix_sig(
                sig,
                match group.team_type {
                    crate::domain::conversation::ConversationTeamType::Unknown => 0,
                    crate::domain::conversation::ConversationTeamType::Simple => 1,
                    crate::domain::conversation::ConversationTeamType::Complex => 2,
                },
            );
        }
        None => mix_sig(sig, 0),
    }
}

pub(crate) fn workspace_state_signature(workspace: &crate::state::state::UiWorkspaceState) -> u64 {
    let mut sig = SIG_SEED;
    if let Some(active_workspace_id) = workspace.active_workspace_id.as_ref() {
        mix_sig(&mut sig, 1);
        mix_sig_str(&mut sig, &active_workspace_id.0);
    } else {
        mix_sig(&mut sig, 0);
    }
    mix_sig_str(&mut sig, &workspace.workspace_name);
    mix_sig(&mut sig, workspace.channels.len() as u64);
    for summary in &workspace.channels {
        mix_sig_conversation_summary(&mut sig, summary);
    }
    mix_sig(&mut sig, workspace.direct_messages.len() as u64);
    for summary in &workspace.direct_messages {
        mix_sig_conversation_summary(&mut sig, summary);
    }
    sig
}

pub(crate) fn sidebar_sections_state_signature(
    sections: &[crate::state::state::UiSidebarSectionState],
) -> u64 {
    let mut sig = SIG_SEED;
    mix_sig(&mut sig, sections.len() as u64);
    for section in sections {
        match section.id.as_ref() {
            Some(id) => {
                mix_sig(&mut sig, 1);
                mix_sig_str(&mut sig, &id.0);
            }
            None => mix_sig(&mut sig, 0),
        }
        mix_sig_str(&mut sig, &section.title);
        mix_sig_bool(&mut sig, section.collapsed);
        mix_sig(&mut sig, section.rows.len() as u64);
        for row in &section.rows {
            mix_sig_str(&mut sig, &row.label);
            mix_sig(&mut sig, row.unread_count as u64);
            mix_sig(&mut sig, row.mention_count as u64);
            match row.route.as_ref() {
                Some(route) => {
                    mix_sig(&mut sig, 1);
                    mix_sig_route(&mut sig, route);
                }
                None => mix_sig(&mut sig, 0),
            }
        }
    }
    sig
}

pub(crate) fn sidebar_avatar_inputs_signature(
    sections: &[crate::models::sidebar_model::SidebarSection],
    direct_messages: &[crate::domain::conversation::ConversationSummary],
    channels: &[crate::domain::conversation::ConversationSummary],
    user_profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
    user_presences: &HashMap<UserId, crate::domain::presence::Presence>,
) -> u64 {
    let mut sig = SIG_SEED;
    for row in sections.iter().flat_map(|section| section.rows.iter()) {
        mix_sig_str(&mut sig, &row.route.label());
        mix_sig_str(&mut sig, &row.label);
    }

    for summary in channels {
        mix_sig_str(&mut sig, &summary.id.0);
        mix_sig_bool(&mut sig, summary.is_simple_team());
        if let Some(group) = &summary.group {
            mix_sig_str(&mut sig, &group.id);
        }
        if let Some(asset) = team_avatar_asset(summary, user_profiles) {
            mix_sig_str(&mut sig, &asset);
        }
    }
    for summary in direct_messages {
        mix_sig_str(&mut sig, &summary.id.0);
        mix_sig_str(&mut sig, &summary.title);
        for username in summary
            .title
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let user_id = UserId::new(username.to_ascii_lowercase());
            if let Some(profile) = user_profiles.get(&user_id) {
                mix_sig_str(&mut sig, &profile.display_name);
                if let Some(asset) = profile.avatar_asset.as_deref() {
                    mix_sig_str(&mut sig, asset);
                }
            }
            if let Some(presence) = user_presences.get(&user_id) {
                mix_sig_str(&mut sig, &format!("{:?}", presence.availability));
                mix_sig_str(
                    &mut sig,
                    presence.status_text.as_deref().unwrap_or_default(),
                );
            }
        }
    }
    sig
}

pub(crate) fn timeline_emoji_signature(
    conversation_id: Option<&ConversationId>,
    conversation_emojis: &HashMap<
        ConversationId,
        HashMap<String, crate::state::state::EmojiRenderState>,
    >,
    emoji_sources: &HashMap<String, crate::state::state::EmojiRenderState>,
) -> u64 {
    let mut sig = SIG_SEED;
    if let Some(conversation_id) = conversation_id {
        mix_sig_str(&mut sig, &conversation_id.0);
        if let Some(emoji_index) = conversation_emojis.get(conversation_id) {
            let mut aliases = emoji_index.keys().collect::<Vec<_>>();
            aliases.sort_unstable();
            mix_sig(&mut sig, aliases.len() as u64);
            for alias in aliases {
                mix_sig_str(&mut sig, alias);
                if let Some(emoji) = emoji_index.get(alias) {
                    mix_sig_str(&mut sig, &emoji.alias);
                    if let Some(unicode) = emoji.unicode.as_deref() {
                        mix_sig_str(&mut sig, unicode);
                    } else {
                        mix_sig(&mut sig, 0);
                    }
                    if let Some(asset_path) = emoji.asset_path.as_deref() {
                        mix_sig_str(&mut sig, asset_path);
                    } else {
                        mix_sig(&mut sig, 0);
                    }
                    mix_sig(&mut sig, emoji.updated_ms as u64);
                }
            }
        } else {
            mix_sig(&mut sig, 0);
        }
    } else {
        mix_sig(&mut sig, 0);
    }

    let mut source_keys = emoji_sources.keys().collect::<Vec<_>>();
    source_keys.sort_unstable();
    mix_sig(&mut sig, source_keys.len() as u64);
    for source_key in source_keys {
        mix_sig_str(&mut sig, source_key);
        if let Some(emoji) = emoji_sources.get(source_key) {
            mix_sig_str(&mut sig, &emoji.alias);
            if let Some(unicode) = emoji.unicode.as_deref() {
                mix_sig_str(&mut sig, unicode);
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(asset_path) = emoji.asset_path.as_deref() {
                mix_sig_str(&mut sig, asset_path);
            } else {
                mix_sig(&mut sig, 0);
            }
            mix_sig(&mut sig, emoji.updated_ms as u64);
        }
    }
    sig
}

pub(crate) fn timeline_reactions_signature(
    conversation_id: Option<&ConversationId>,
    message_reactions: &HashMap<
        ConversationId,
        HashMap<MessageId, Vec<crate::state::state::MessageReactionState>>,
    >,
    user_profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
) -> u64 {
    let mut sig = SIG_SEED;
    let Some(conversation_id) = conversation_id else {
        mix_sig(&mut sig, 0);
        return sig;
    };
    mix_sig_str(&mut sig, &conversation_id.0);
    let Some(reactions_by_message) = message_reactions.get(conversation_id) else {
        mix_sig(&mut sig, 0);
        return sig;
    };

    let mut message_ids = reactions_by_message.keys().collect::<Vec<_>>();
    message_ids.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    mix_sig(&mut sig, message_ids.len() as u64);
    for message_id in message_ids {
        mix_sig_str(&mut sig, &message_id.0);
        let Some(reactions) = reactions_by_message.get(message_id) else {
            mix_sig(&mut sig, 0);
            continue;
        };

        let mut ordered = reactions.iter().collect::<Vec<_>>();
        ordered.sort_unstable_by(|left, right| left.emoji.cmp(&right.emoji));
        mix_sig(&mut sig, ordered.len() as u64);
        for reaction in ordered {
            mix_sig_str(&mut sig, &reaction.emoji);
            if let Some(source_ref) = &reaction.source_ref {
                mix_sig_str(&mut sig, &source_ref.cache_key());
            } else {
                mix_sig(&mut sig, 0);
            }
            let mut actor_ids = reaction.actor_ids.iter().collect::<Vec<_>>();
            actor_ids.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            mix_sig(&mut sig, actor_ids.len() as u64);
            for actor_id in actor_ids {
                mix_sig_str(&mut sig, &actor_id.0);
                if let Some(profile) = user_profiles.get(actor_id).or_else(|| {
                    let lower = actor_id.0.to_ascii_lowercase();
                    if lower == actor_id.0 {
                        None
                    } else {
                        user_profiles.get(&UserId::new(lower))
                    }
                }) {
                    mix_sig_str(&mut sig, &profile.display_name);
                    mix_sig(&mut sig, profile.updated_ms as u64);
                } else {
                    mix_sig(&mut sig, 0);
                }
            }
            mix_sig(&mut sig, reaction.updated_ms as u64);
        }
    }

    sig
}

fn attachment_kind_sig(kind: &crate::domain::attachment::AttachmentKind) -> u64 {
    match kind {
        crate::domain::attachment::AttachmentKind::Image => 1,
        crate::domain::attachment::AttachmentKind::Video => 2,
        crate::domain::attachment::AttachmentKind::Audio => 3,
        crate::domain::attachment::AttachmentKind::File => 4,
    }
}

fn mix_optional_u32(sig: &mut u64, value: Option<u32>) {
    if let Some(value) = value {
        mix_sig(sig, 1);
        mix_sig(sig, value as u64);
    } else {
        mix_sig(sig, 0);
    }
}

fn mix_optional_u64(sig: &mut u64, value: Option<u64>) {
    if let Some(value) = value {
        mix_sig(sig, 1);
        mix_sig(sig, value);
    } else {
        mix_sig(sig, 0);
    }
}

fn mix_optional_str(sig: &mut u64, value: Option<&str>) {
    if let Some(value) = value {
        mix_sig(sig, 1);
        mix_sig_str(sig, value);
    } else {
        mix_sig(sig, 0);
    }
}

fn mix_attachment_source(sig: &mut u64, source: &crate::domain::attachment::AttachmentSource) {
    match source {
        crate::domain::attachment::AttachmentSource::Url(url) => {
            mix_sig(sig, 1);
            mix_sig_str(sig, url);
        }
        crate::domain::attachment::AttachmentSource::LocalPath(path) => {
            mix_sig(sig, 2);
            mix_sig_str(sig, path);
        }
    }
}

fn message_send_state_sig(send_state: &crate::domain::message::MessageSendState) -> u64 {
    match send_state {
        crate::domain::message::MessageSendState::Sent => 1,
        crate::domain::message::MessageSendState::Pending => 2,
        crate::domain::message::MessageSendState::Failed => 3,
    }
}

fn mix_chat_event_signature(sig: &mut u64, event: &ChatEvent) {
    match event {
        ChatEvent::MemberJoined => mix_sig(sig, 1),
        ChatEvent::MemberLeft => mix_sig(sig, 2),
        ChatEvent::MembersAdded { user_ids, role } => {
            mix_sig(sig, 3);
            mix_sig(sig, user_ids.len() as u64);
            for user_id in user_ids {
                mix_sig_str(sig, &user_id.0);
            }
            if let Some(role) = role {
                mix_sig_str(sig, role);
            } else {
                mix_sig(sig, 0);
            }
        }
        ChatEvent::MembersRemoved { user_ids } => {
            mix_sig(sig, 4);
            mix_sig(sig, user_ids.len() as u64);
            for user_id in user_ids {
                mix_sig_str(sig, &user_id.0);
            }
        }
        ChatEvent::DescriptionChanged { description } => {
            mix_sig(sig, 5);
            if let Some(description) = description {
                mix_sig_str(sig, description);
            } else {
                mix_sig(sig, 0);
            }
        }
        ChatEvent::ChannelRenamed { new_name } => {
            mix_sig(sig, 6);
            mix_sig_str(sig, new_name);
        }
        ChatEvent::AvatarChanged => mix_sig(sig, 7),
        ChatEvent::MessagePinned { target_message_id } => {
            mix_sig(sig, 8);
            if let Some(target_message_id) = target_message_id {
                mix_sig_str(sig, &target_message_id.0);
            } else {
                mix_sig(sig, 0);
            }
        }
        ChatEvent::MessageDeleted { target_message_id } => {
            mix_sig(sig, 9);
            if let Some(target_message_id) = target_message_id {
                mix_sig_str(sig, &target_message_id.0);
            } else {
                mix_sig(sig, 0);
            }
        }
        ChatEvent::HistoryCleared => mix_sig(sig, 10),
        ChatEvent::RetentionChanged { summary } => {
            mix_sig(sig, 11);
            mix_sig_str(sig, summary);
        }
        ChatEvent::ChannelCreated { channel_name, .. } => {
            mix_sig(sig, 13);
            mix_sig_str(sig, channel_name);
        }
        ChatEvent::Other { text } => {
            mix_sig(sig, 12);
            mix_sig_str(sig, text);
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keep the explicit inputs of this existing signature API."
)]
pub(crate) fn timeline_rows_input_signature(
    conversation_id: Option<&ConversationId>,
    messages: &[crate::domain::message::MessageRecord],
    unread_marker: Option<&MessageId>,
    loading_older: bool,
    user_profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
    user_affinities: &HashMap<UserId, crate::domain::affinity::Affinity>,
    user_presences: &HashMap<UserId, crate::domain::presence::Presence>,
    current_user_id: Option<&UserId>,
    current_user_avatar: Option<&str>,
) -> u64 {
    let mut sig = SIG_SEED;
    if let Some(conversation_id) = conversation_id {
        mix_sig(&mut sig, 1);
        mix_sig_str(&mut sig, &conversation_id.0);
    } else {
        mix_sig(&mut sig, 0);
    }
    if let Some(unread_marker) = unread_marker {
        mix_sig(&mut sig, 1);
        mix_sig_str(&mut sig, &unread_marker.0);
    } else {
        mix_sig(&mut sig, 0);
    }
    mix_sig_bool(&mut sig, loading_older);
    // Day divider labels are relative ("Today"), so rebuild rows when the date changes.
    mix_sig_str(&mut sig, &chrono::Local::now().date_naive().to_string());
    if let Some(current_user_id) = current_user_id {
        mix_sig_str(&mut sig, &current_user_id.0);
    } else {
        mix_sig(&mut sig, 0);
    }
    if let Some(current_user_avatar) = current_user_avatar {
        mix_sig_str(&mut sig, current_user_avatar);
    } else {
        mix_sig(&mut sig, 0);
    }

    mix_sig(&mut sig, messages.len() as u64);
    for message in messages {
        mix_sig_str(&mut sig, &message.id.0);
        mix_sig_str(&mut sig, &message.conversation_id.0);
        mix_sig_str(&mut sig, &message.author_id.0);
        if let Some(timestamp_ms) = message.timestamp_ms {
            mix_sig(&mut sig, 1);
            mix_sig(&mut sig, timestamp_ms as u64);
        } else {
            mix_sig(&mut sig, 0);
        }
        mix_sig_str(&mut sig, &message.permalink);
        mix_sig(&mut sig, message.thread_reply_count as u64);
        mix_sig(&mut sig, message_send_state_sig(&message.send_state));
        if let Some(edited) = &message.edited {
            mix_sig(&mut sig, 1);
            mix_sig_str(&mut sig, &edited.edit_id.0);
            if let Some(edited_at_ms) = edited.edited_at_ms {
                mix_sig(&mut sig, 1);
                mix_sig(&mut sig, edited_at_ms as u64);
            } else {
                mix_sig(&mut sig, 0);
            }
        } else {
            mix_sig(&mut sig, 0);
        }
        if let Some(event) = &message.event {
            mix_sig(&mut sig, 1);
            mix_chat_event_signature(&mut sig, event);
        } else {
            mix_sig(&mut sig, 0);
        }

        mix_sig(&mut sig, message.fragments.len() as u64);
        for fragment in &message.fragments {
            match fragment {
                crate::domain::message::MessageFragment::Text(value) => {
                    mix_sig(&mut sig, 1);
                    mix_sig_str(&mut sig, value);
                }
                crate::domain::message::MessageFragment::InlineCode(value) => {
                    mix_sig(&mut sig, 9);
                    mix_sig_str(&mut sig, value);
                }
                crate::domain::message::MessageFragment::Mention(user_id) => {
                    mix_sig(&mut sig, 2);
                    mix_sig_str(&mut sig, &user_id.0);
                }
                crate::domain::message::MessageFragment::Emoji { alias, source_ref } => {
                    mix_sig(&mut sig, 8);
                    mix_sig_str(&mut sig, alias);
                    if let Some(source_ref) = source_ref {
                        mix_sig_str(&mut sig, &source_ref.cache_key());
                    } else {
                        mix_sig(&mut sig, 0);
                    }
                }
                crate::domain::message::MessageFragment::Code { text: value, .. } => {
                    mix_sig(&mut sig, 3);
                    mix_sig_str(&mut sig, value);
                }
                crate::domain::message::MessageFragment::Quote(value) => {
                    mix_sig(&mut sig, 4);
                    mix_sig_str(&mut sig, value);
                }
                crate::domain::message::MessageFragment::ChannelMention { name } => {
                    mix_sig(&mut sig, 5);
                    mix_sig_str(&mut sig, name);
                }
                crate::domain::message::MessageFragment::BroadcastMention(kind) => {
                    mix_sig(&mut sig, 6);
                    match kind {
                        crate::domain::message::BroadcastKind::Here => mix_sig(&mut sig, 1),
                        crate::domain::message::BroadcastKind::All => mix_sig(&mut sig, 2),
                    }
                }
                crate::domain::message::MessageFragment::Link { url, display } => {
                    mix_sig(&mut sig, 7);
                    mix_sig_str(&mut sig, url);
                    mix_sig_str(&mut sig, display);
                }
            }
        }

        mix_sig(&mut sig, message.attachments.len() as u64);
        for attachment in &message.attachments {
            mix_sig_str(&mut sig, &attachment.name);
            mix_sig(&mut sig, attachment_kind_sig(&attachment.kind));
            mix_sig(&mut sig, attachment.size_bytes);
            mix_optional_str(&mut sig, attachment.mime_type.as_deref());
            mix_optional_u32(&mut sig, attachment.width);
            mix_optional_u32(&mut sig, attachment.height);
            if let Some(preview) = attachment.preview.as_ref() {
                mix_sig(&mut sig, 1);
                mix_attachment_source(&mut sig, &preview.source);
                mix_optional_u32(&mut sig, preview.width);
                mix_optional_u32(&mut sig, preview.height);
            } else {
                mix_sig(&mut sig, 0);
            }
            mix_optional_u64(&mut sig, attachment.duration_ms);
            if let Some(waveform) = attachment.waveform.as_ref() {
                mix_sig(&mut sig, 1);
                mix_sig(&mut sig, waveform.len() as u64);
                for sample in waveform {
                    mix_sig(&mut sig, sample.to_bits() as u64);
                }
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(source) = attachment.source.as_ref() {
                mix_sig(&mut sig, 1);
                mix_attachment_source(&mut sig, source);
            } else {
                mix_sig(&mut sig, 0);
            }
        }

        if let Some(profile) = user_profiles.get(&message.author_id).or_else(|| {
            let lower = message.author_id.0.to_ascii_lowercase();
            if lower == message.author_id.0 {
                None
            } else {
                user_profiles.get(&UserId::new(lower))
            }
        }) {
            mix_sig_str(&mut sig, &profile.display_name);
            if let Some(asset) = profile.avatar_asset.as_deref() {
                mix_sig_str(&mut sig, asset);
            } else {
                mix_sig(&mut sig, 0);
            }
            mix_sig(&mut sig, profile.updated_ms as u64);
        } else if current_user_id == Some(&message.author_id) {
            if let Some(asset) = current_user_avatar {
                mix_sig_str(&mut sig, asset);
            } else {
                mix_sig(&mut sig, 0);
            }
        }
        let affinity = user_affinities
            .get(&message.author_id)
            .copied()
            .or_else(|| {
                let lower = message.author_id.0.to_ascii_lowercase();
                if lower == message.author_id.0 {
                    None
                } else {
                    user_affinities.get(&UserId::new(lower)).copied()
                }
            })
            .unwrap_or(crate::domain::affinity::Affinity::None);
        match affinity {
            crate::domain::affinity::Affinity::None => mix_sig(&mut sig, 0),
            crate::domain::affinity::Affinity::Positive => mix_sig(&mut sig, 1),
            crate::domain::affinity::Affinity::Broken => mix_sig(&mut sig, 2),
        }
        let presence = user_presences.get(&message.author_id).or_else(|| {
            let lower = message.author_id.0.to_ascii_lowercase();
            if lower == message.author_id.0 {
                None
            } else {
                user_presences.get(&UserId::new(lower))
            }
        });
        if let Some(presence) = presence {
            match presence.availability {
                crate::domain::presence::Availability::Active => mix_sig(&mut sig, 1),
                crate::domain::presence::Availability::Away => mix_sig(&mut sig, 2),
                crate::domain::presence::Availability::DoNotDisturb => mix_sig(&mut sig, 3),
                crate::domain::presence::Availability::Offline => mix_sig(&mut sig, 4),
                crate::domain::presence::Availability::Unknown => mix_sig(&mut sig, 5),
            }
            if let Some(status_text) = presence.status_text.as_deref() {
                mix_sig_str(&mut sig, status_text);
            } else {
                mix_sig(&mut sig, 0);
            }
        } else {
            mix_sig(&mut sig, 0);
        }
    }
    sig
}

pub(crate) fn timeline_link_previews_signature(
    conversation_id: Option<&ConversationId>,
    messages: &[crate::domain::message::MessageRecord],
) -> u64 {
    let mut sig = SIG_SEED;
    if let Some(conversation_id) = conversation_id {
        mix_sig(&mut sig, 1);
        mix_sig_str(&mut sig, &conversation_id.0);
    } else {
        mix_sig(&mut sig, 0);
    }
    mix_sig(&mut sig, messages.len() as u64);
    for message in messages {
        mix_sig_str(&mut sig, &message.id.0);
        mix_sig(&mut sig, message.link_previews.len() as u64);
        for preview in &message.link_previews {
            mix_sig_str(&mut sig, &preview.url);
            if let Some(video_url) = preview.video_url.as_deref() {
                mix_sig_str(&mut sig, video_url);
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(title) = preview.title.as_deref() {
                mix_sig_str(&mut sig, title);
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(site) = preview.site.as_deref() {
                mix_sig_str(&mut sig, site);
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(description) = preview.description.as_deref() {
                mix_sig_str(&mut sig, description);
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(thumbnail_asset) = preview.thumbnail_asset.as_deref() {
                mix_sig_str(&mut sig, thumbnail_asset);
            } else {
                mix_sig(&mut sig, 0);
            }
            mix_sig_bool(&mut sig, preview.is_media);
            if let Some(width) = preview.media_width {
                mix_sig(&mut sig, width as u64);
            } else {
                mix_sig(&mut sig, 0);
            }
            if let Some(height) = preview.media_height {
                mix_sig(&mut sig, height as u64);
            } else {
                mix_sig(&mut sig, 0);
            }
            mix_sig_bool(&mut sig, preview.is_video);
        }
    }
    sig
}
