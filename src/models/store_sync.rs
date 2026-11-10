//! Projects the store's `UiState` into the `AppModels` the views render.
//!
//! This is the UI-framework-free half of the app window's model sync: it only
//! writes models and reports what changed, so the window can replay its
//! scrolling and fetch scheduling afterwards.
use std::collections::{HashMap, HashSet};

use super::{
    AppModels,
    app_model::Connectivity,
    conversation_model::AvatarParticipant,
    link_previews::{
        apply_link_previews_to_thread_replies, apply_og_previews, sync_timeline_row_link_previews,
    },
    navigation_model::RightPaneMode,
    profile_panel_model::SocialTab,
    recent_sidebar::build_recent_rows,
    sidebar_model::{SidebarAvatar, team_avatar_key},
    sidebar_preview::SidebarPreview,
    sync_signatures::{
        sidebar_avatar_inputs_signature, sidebar_sections_state_signature,
        timeline_emoji_signature, timeline_link_previews_signature, timeline_reactions_signature,
        timeline_rows_input_signature, workspace_state_signature,
    },
};
use crate::domain::{
    affinity::Affinity,
    channel_details::{ChannelDetails, ChannelMemberPreview, NotificationLevel},
    conversation::{ConversationKind, ConversationSummary},
    ids::{ConversationId, MessageId, SidebarSectionId, UserId},
    message::{ChatEvent, MessageFragment, MessageRecord, MessageSendState},
    profile::SocialGraphListType,
    route::Route,
};
use crate::services::{github_pr_service::GithubPrService, og_service::OgService};
use crate::state::{ConnectionState, UiState, event::TeamRoleKind, state::BootPhase};

const NON_TEXT_PLACEHOLDER_BODY: &str = "<non-text message>";

/// Caches that let repeated syncs skip rebuilding models whose inputs are unchanged.
#[derive(Default)]
pub struct StoreSync {
    projected_conversation_id: Option<ConversationId>,
    /// Signed-in user the sidebar previews were built for ("You").
    preview_user: Option<UserId>,
    workspace_sig: Option<u64>,
    sidebar_sections_sig: Option<u64>,
    sidebar_avatar_sig: Option<u64>,
    timeline_rows_sig: Option<u64>,
    timeline_link_previews_sig: Option<u64>,
    timeline_emoji_sig: Option<u64>,
    timeline_reactions_sig: Option<u64>,
    last_timeline_loading_older: bool,
    /// Sidebar avatars by route label, and team icons by `team_avatar_key`.
    pub sidebar_conversation_avatars: HashMap<String, SidebarAvatar>,
    /// Second lines of the Previews sidebar style, by conversation id.
    pub sidebar_previews: HashMap<String, SidebarPreview>,
}

/// What the frontend's timeline viewport looks like right before the sync.
#[derive(Clone, Debug, Default)]
pub struct ViewHints {
    pub timeline_near_bottom: bool,
    /// A jump to a message is settling; background loads must not move the viewport.
    pub jump_to_message_active: bool,
    /// The first message at or below the top of the viewport. Only consulted when
    /// the user is scrolled up (or jumping), to keep that message in place when
    /// rows are rebuilt; pass `None` while another scroll restore is pending.
    pub first_visible_message_id: Option<MessageId>,
}

pub struct SyncInputs<'a> {
    /// Summaries of conversations previewed without joining, with whether you can post.
    pub preview_summaries: &'a HashMap<ConversationId, (ConversationSummary, bool)>,
    pub og_service: &'a OgService,
    pub github_pr_service: &'a GithubPrService,
}

/// What a sync changed, for the frontend to replay its view side effects.
#[derive(Default, Debug)]
pub struct SyncOutcome {
    pub route_changed: bool,
    /// The route navigated to, when `route_changed`.
    pub next_route: Option<Route>,
    pub conversation_changed: bool,
    pub timeline_rows_changed: bool,
    pub timeline_emoji_changed: bool,
    pub timeline_reactions_changed: bool,
    pub timeline_link_previews_changed: bool,
    pub pinned_banner_visibility_changed: bool,
    pub has_pinned_banner: bool,
    pub loading_older: bool,
    pub loading_older_just_finished: bool,
    pub was_near_bottom_before_sync: bool,
    /// Row to keep at the top of the viewport after rows were rebuilt.
    pub anchor_row_index: Option<usize>,
    /// The quick switcher corpus or its profile names changed, so incremental
    /// local results computed before the sync must not be reused.
    pub quick_switcher_local_results_stale: bool,
}

impl StoreSync {
    pub fn sync(
        &mut self,
        models: &mut AppModels,
        snapshot: &UiState,
        inputs: SyncInputs<'_>,
        hints: ViewHints,
    ) -> SyncOutcome {
        let mut outcome = SyncOutcome::default();
        models.app.connectivity = if snapshot
            .backend
            .accounts
            .values()
            .any(|account| matches!(account.connection_state, ConnectionState::Connected))
        {
            Connectivity::Online
        } else if matches!(
            snapshot.app.boot_phase,
            BootPhase::Launching | BootPhase::HydratingLocalState | BootPhase::ConnectingBackend
        ) {
            Connectivity::Reconnecting
        } else {
            Connectivity::Offline
        };
        models.sidebar.filter = snapshot.sidebar.filter.clone();
        models.search.query = snapshot.search.query.clone();
        models.search.filters = snapshot.search.filters.clone();
        models.search.results = snapshot.search.results.clone();
        models.search.highlighted_index = snapshot.search.highlighted_index;
        models.search.is_loading = snapshot.search.is_loading;
        models.search.submitted_query = snapshot.search.submitted_query.clone();
        models.new_chat.open = snapshot.new_chat.open;
        models.new_chat.is_searching = snapshot.new_chat.is_searching;
        if models.new_chat.search_query != snapshot.new_chat.search_query {
            models.new_chat.highlighted_index = 0;
        }
        models.new_chat.search_query = snapshot.new_chat.search_query.clone();
        models.new_chat.search_results = snapshot.new_chat.search_results.clone();
        models.new_chat.selected_participants = snapshot.new_chat.selected_participants.clone();
        models.new_chat.creating = snapshot.new_chat.creating;
        models.new_chat.error = snapshot.new_chat.error.clone();
        models.overlay.new_chat_open = snapshot.new_chat.open;

        let workspace_sig = workspace_state_signature(&snapshot.workspace);
        let workspace_changed = self.workspace_sig != Some(workspace_sig);
        if workspace_changed {
            models.workspace.workspace_name = snapshot.workspace.workspace_name.clone();
            models.workspace.channels = snapshot.workspace.channels.clone();
            models.workspace.direct_messages = snapshot.workspace.direct_messages.clone();
            self.workspace_sig = Some(workspace_sig);
        }

        let mut current_user_id: Option<UserId> = None;
        let previous_active_workspace_id = models.app.active_workspace_id.clone();
        if let Some(workspace_id) = snapshot.workspace.active_workspace_id.clone() {
            models.app.active_workspace_id = workspace_id.clone();
            models.workspace.workspace_id = workspace_id;
        }
        let active_workspace_changed =
            models.app.active_workspace_id != previous_active_workspace_id;
        if let Some(account) = snapshot
            .backend
            .accounts
            .values()
            .find(|account| matches!(account.connection_state, ConnectionState::Connected))
            .or_else(|| snapshot.backend.accounts.values().next())
        {
            let account_user = UserId::new(account.display_name.clone());
            current_user_id = Some(account_user.clone());
            let profile = snapshot
                .backend
                .user_profiles
                .get(&account_user)
                .or_else(|| {
                    let lower = account_user.0.to_ascii_lowercase();
                    if lower == account_user.0 {
                        None
                    } else {
                        snapshot.backend.user_profiles.get(&UserId::new(lower))
                    }
                });
            models.app.current_user_id = Some(account_user);
            models.app.current_user_display_name = profile
                .map(|value| value.display_name.clone())
                .unwrap_or_else(|| account.display_name.clone());
            models.app.current_user_avatar_asset = profile
                .and_then(|value| value.avatar_asset.clone())
                .or_else(|| account.avatar.clone());
        } else {
            models.app.current_user_id = None;
            models.app.current_user_display_name = "You".to_string();
            models.app.current_user_avatar_asset = None;
        }
        let quick_switcher_profiles_changed = models.update_quick_switcher_profile_names(
            snapshot
                .backend
                .user_profiles
                .iter()
                .filter_map(|(user_id, profile)| {
                    let display_name = profile.display_name.trim();
                    if display_name.is_empty() {
                        None
                    } else {
                        Some((user_id.0.to_ascii_lowercase(), display_name.to_string()))
                    }
                })
                .collect(),
        );
        if workspace_changed || active_workspace_changed {
            models.rebuild_quick_switcher_local_search_corpus();
        }
        outcome.quick_switcher_local_results_stale =
            workspace_changed || active_workspace_changed || quick_switcher_profiles_changed;
        if quick_switcher_profiles_changed
            && models.overlay.quick_switcher_open
            && models.quick_switcher.query.trim().is_empty()
        {
            models.update_quick_switcher_query(String::new());
        }
        let current_user_avatar = models.app.current_user_avatar_asset.clone();

        let next_route = snapshot
            .navigation
            .current_route
            .clone()
            .unwrap_or_else(|| models.navigation.current.clone());
        let route_changed = models.navigation.current != next_route;
        if route_changed {
            if conversation_id_from_navigated_route(&models.navigation.current)
                != conversation_id_from_navigated_route(&next_route)
            {
                models.navigation.right_pane = RightPaneMode::Hidden;
            }
            models.navigation.current = next_route.clone();
        }

        models.profile_panel.user_id = snapshot.backend.profile_panel.selected_user.clone();
        models.profile_panel.active_social_tab =
            match snapshot.backend.profile_panel.active_social_tab {
                SocialGraphListType::Followers => SocialTab::Followers,
                SocialGraphListType::Following => SocialTab::Following,
            };
        if let Some(user_id) = models.profile_panel.user_id.clone() {
            models.profile_panel.profile = snapshot
                .backend
                .profile_panel
                .profiles
                .get(&user_id)
                .cloned();
            models.profile_panel.loading =
                snapshot.backend.profile_panel.loading.contains(&user_id);
            models.profile_panel.social_list_status = snapshot
                .backend
                .profile_panel
                .social_lists
                .get(&(
                    user_id.clone(),
                    models.profile_panel.active_social_tab.as_list_type(),
                ))
                .copied();
        } else {
            models.profile_panel.profile = None;
            models.profile_panel.loading = false;
            models.profile_panel.social_list_status = None;
        }

        let sidebar_sections_sig = sidebar_sections_state_signature(&snapshot.sidebar.sections);
        let sidebar_sections_changed = self.sidebar_sections_sig != Some(sidebar_sections_sig);
        // Previews name people ("You", "Lena"), so they follow profile changes too.
        let previews_changed =
            quick_switcher_profiles_changed || self.preview_user != models.app.current_user_id;
        if sidebar_sections_changed || workspace_changed || previews_changed {
            self.preview_user = models.app.current_user_id.clone();
            self.sidebar_previews = super::sidebar_preview::sidebar_previews(
                snapshot
                    .workspace
                    .channels
                    .iter()
                    .chain(&snapshot.workspace.direct_messages),
                &super::sidebar_preview::PreviewNames {
                    me: models.app.current_user_id.as_ref().map(|id| id.0.as_str()),
                    display_names: &models.quick_switcher_profile_names,
                },
            );
        }
        if sidebar_sections_changed || workspace_changed || previews_changed {
            let channel_teams = snapshot
                .workspace
                .channels
                .iter()
                .filter(|summary| !summary.is_simple_team())
                .filter_map(|summary| {
                    summary
                        .group
                        .as_ref()
                        .map(|group| (summary.id.0.as_str(), group))
                })
                .collect::<HashMap<_, _>>();
            let muted = snapshot
                .workspace
                .channels
                .iter()
                .chain(&snapshot.workspace.direct_messages)
                .filter(|summary| summary.muted)
                .map(|summary| summary.id.0.as_str())
                .collect::<HashSet<_>>();
            models.sidebar.sections = snapshot
                .sidebar
                .sections
                .iter()
                .map(|section| super::sidebar_model::SidebarSection {
                    id: section
                        .id
                        .clone()
                        .unwrap_or_else(|| SidebarSectionId::new("section")),
                    title: section.title.clone(),
                    rows: section
                        .rows
                        .iter()
                        .filter_map(|row| {
                            row.route
                                .clone()
                                .map(|route| super::sidebar_model::SidebarRow {
                                    label: row.label.clone(),
                                    team: match &route {
                                        Route::Channel { channel_id, .. } => channel_teams
                                            .get(channel_id.0.as_str())
                                            .map(|group| (*group).clone()),
                                        _ => None,
                                    },
                                    unread_count: row.unread_count,
                                    mention_count: row.mention_count,
                                    preview: route_conversation_key(&route)
                                        .and_then(|id| self.sidebar_previews.get(id))
                                        .cloned(),
                                    muted: route_conversation_key(&route)
                                        .is_some_and(|id| muted.contains(id)),
                                    route,
                                })
                        })
                        .collect(),
                    collapsed: section.collapsed,
                })
                .collect();
            self.sidebar_sections_sig = Some(sidebar_sections_sig);
        }
        models.sidebar.highlighted_route = snapshot.sidebar.highlighted_route.clone();
        if sidebar_sections_changed || workspace_changed || previews_changed {
            models.apply_saved_sidebar_order();
        }
        if route_changed {
            models.expand_section_for_route(&next_route);
        }

        models.restore_sidebar_mode();
        if sidebar_sections_changed
            || workspace_changed
            || active_workspace_changed
            || previews_changed
        {
            models.sidebar.recent_rows = build_recent_rows(
                &snapshot.workspace.channels,
                &snapshot.workspace.direct_messages,
                &models.sidebar.sections,
                &models.app.active_workspace_id,
                &self.sidebar_previews,
            );
        }

        let sidebar_avatar_sig = sidebar_avatar_inputs_signature(
            &models.sidebar.sections,
            &snapshot.workspace.direct_messages,
            &snapshot.workspace.channels,
            &snapshot.backend.user_profiles,
            &snapshot.backend.user_presences,
        );
        if self.sidebar_avatar_sig != Some(sidebar_avatar_sig) {
            let dm_conversation_avatar_assets = snapshot
                .workspace
                .direct_messages
                .iter()
                .map(|summary| {
                    (
                        summary.id.0.clone(),
                        SidebarAvatar::Participants(dm_participant_avatars(
                            summary,
                            &snapshot.backend.user_profiles,
                            &snapshot.backend.user_presences,
                        )),
                    )
                })
                .collect::<HashMap<_, _>>();

            let team_avatars = snapshot
                .workspace
                .channels
                .iter()
                .filter_map(|summary| {
                    simple_team_avatar_asset(summary, &snapshot.backend.user_profiles)
                        .map(|asset| (summary.id.0.clone(), asset))
                })
                .collect::<HashMap<_, _>>();
            let mut conversation_avatars = HashMap::new();
            for summary in &snapshot.workspace.channels {
                if let Some(group) = &summary.group
                    && let Some(asset) = team_avatar_asset(summary, &snapshot.backend.user_profiles)
                {
                    conversation_avatars
                        .insert(team_avatar_key(&group.id), SidebarAvatar::Team(asset));
                }
            }
            for row in models
                .sidebar
                .sections
                .iter()
                .flat_map(|section| section.rows.iter())
            {
                if let Route::DirectMessage { dm_id, .. } = &row.route
                    && let Some(avatar_asset) = dm_conversation_avatar_assets.get(&dm_id.0)
                {
                    conversation_avatars.insert(row.route.label(), avatar_asset.clone());
                }
                if let Route::Channel { channel_id, .. } = &row.route
                    && let Some(asset) = team_avatars.get(&channel_id.0)
                {
                    conversation_avatars
                        .insert(row.route.label(), SidebarAvatar::Team(asset.clone()));
                }
            }
            self.sidebar_conversation_avatars = conversation_avatars;
            self.sidebar_avatar_sig = Some(sidebar_avatar_sig);
        }

        models.conversation.participant_avatars = self
            .sidebar_conversation_avatars
            .get(&next_route.label())
            .map(|avatar| avatar.participants().to_vec())
            .unwrap_or_default();

        // Compare with the last store projection, not a view model that navigation
        // may already have reset to the destination conversation.
        let conversation_changed =
            self.projected_conversation_id != snapshot.timeline.conversation_id;
        self.projected_conversation_id = snapshot.timeline.conversation_id.clone();
        if let Some(conversation_id) = snapshot.timeline.conversation_id.clone() {
            models.timeline.conversation_id = conversation_id;
        }
        let was_near_bottom_before_sync =
            hints.timeline_near_bottom && !hints.jump_to_message_active;
        let previous_has_pinned_banner = models.conversation.pinned_message.is_some();
        let timeline_anchor_message_id = if !conversation_changed
            && models.timeline.pending_scroll_target.is_none()
            && (!hints.timeline_near_bottom || hints.jump_to_message_active)
        {
            hints.first_visible_message_id
        } else {
            None
        };

        let timeline_emoji_sig = timeline_emoji_signature(
            Some(&models.timeline.conversation_id),
            &snapshot.backend.conversation_emojis,
            &snapshot.backend.emoji_sources,
        );
        let timeline_emoji_changed = self.timeline_emoji_sig != Some(timeline_emoji_sig);
        if timeline_emoji_changed {
            models.timeline.emoji_index = snapshot
                .timeline
                .conversation_id
                .as_ref()
                .and_then(|conversation_id| {
                    snapshot.backend.conversation_emojis.get(conversation_id)
                })
                .map(|emoji_index| {
                    emoji_index
                        .iter()
                        .map(|(alias, value)| {
                            (
                                alias.clone(),
                                super::timeline_model::InlineEmojiRender {
                                    alias: value.alias.clone(),
                                    unicode: value.unicode.clone(),
                                    asset_path: value.asset_path.clone(),
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            models.timeline.emoji_source_index = snapshot
                .backend
                .emoji_sources
                .iter()
                .map(|(source_key, value)| {
                    (
                        source_key.clone(),
                        super::timeline_model::InlineEmojiRender {
                            alias: value.alias.clone(),
                            unicode: value.unicode.clone(),
                            asset_path: value.asset_path.clone(),
                        },
                    )
                })
                .collect();
            self.timeline_emoji_sig = Some(timeline_emoji_sig);
            models.refresh_quick_react_recent();
        }
        let timeline_reactions_sig = timeline_reactions_signature(
            Some(&models.timeline.conversation_id),
            &snapshot.backend.message_reactions,
            &snapshot.backend.user_profiles,
        );
        let timeline_reactions_changed =
            self.timeline_reactions_sig != Some(timeline_reactions_sig);
        if timeline_reactions_changed {
            let current_user_id = snapshot
                .backend
                .accounts
                .values()
                .find(|account| matches!(account.connection_state, ConnectionState::Connected))
                .map(|account| account.display_name.to_ascii_lowercase());
            models.timeline.reaction_index = snapshot
                .timeline
                .conversation_id
                .as_ref()
                .and_then(|conversation_id| snapshot.backend.message_reactions.get(conversation_id))
                .map(|reactions_by_message| {
                    reactions_by_message
                        .iter()
                        .map(|(message_id, reactions)| {
                            (
                                message_id.clone(),
                                reactions
                                    .iter()
                                    .map(|reaction| {
                                        let mut actors = reaction
                                            .actor_ids
                                            .iter()
                                            .map(|actor_id| {
                                                let display_name = snapshot
                                                    .backend
                                                    .user_profiles
                                                    .get(actor_id)
                                                    .map(|profile| profile.display_name.trim())
                                                    .filter(|name| !name.is_empty())
                                                    .map(str::to_string)
                                                    .unwrap_or_else(|| actor_id.0.clone());
                                                super::timeline_model::ReactionActorRender {
                                                    user_id: actor_id.0.clone(),
                                                    display_name,
                                                }
                                            })
                                            .collect::<Vec<_>>();
                                        actors.sort_by(|left, right| {
                                            left.display_name
                                                .cmp(&right.display_name)
                                                .then_with(|| left.user_id.cmp(&right.user_id))
                                        });
                                        let reacted_by_me =
                                            current_user_id.as_ref().is_some_and(|current_user| {
                                                reaction.actor_ids.iter().any(|actor_id| {
                                                    actor_id.0.eq_ignore_ascii_case(current_user)
                                                })
                                            });
                                        super::timeline_model::MessageReactionRender {
                                            emoji: reaction.emoji.clone(),
                                            source_ref: reaction.source_ref.clone(),
                                            count: actors.len(),
                                            actors,
                                            reacted_by_me,
                                        }
                                    })
                                    .collect(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.timeline_reactions_sig = Some(timeline_reactions_sig);
        }
        models.timeline.highlighted_message_id = snapshot.timeline.highlighted_message_id.clone();
        models.timeline.unread_marker = snapshot.timeline.unread_marker.clone();
        models.timeline.typing_text = snapshot.timeline.typing_text.clone();
        if models.timeline.upload_progress != snapshot.timeline.upload_progress {
            models.timeline.upload_progress = snapshot.timeline.upload_progress.clone();
        }
        models.timeline.current_user_id = current_user_id.clone();
        models.timeline.affinity_index = snapshot.backend.user_affinities.clone();
        models.timeline.older_cursor = snapshot.timeline.older_cursor.clone();
        models.timeline.newer_cursor = snapshot.timeline.newer_cursor.clone();
        models.timeline.loading_older = snapshot.timeline.loading_older;
        sync_thread_pane(
            models,
            snapshot,
            inputs.og_service,
            inputs.github_pr_service,
        );

        if workspace_changed || conversation_changed {
            models.conversation.can_post = false;
            if let Some(active_conversation) = models
                .workspace
                .channels
                .iter()
                .chain(models.workspace.direct_messages.iter())
                .find(|summary| Some(summary.id.clone()) == snapshot.timeline.conversation_id)
                .cloned()
            {
                models.conversation.summary = active_conversation.clone();
                models.composer.conversation_id = active_conversation.id.clone();
                models.conversation.can_post = true;
                models.conversation.is_archived = false;
                models.conversation.avatar_asset = match active_conversation.kind {
                    crate::domain::conversation::ConversationKind::Channel => {
                        team_avatar_asset(&active_conversation, &snapshot.backend.user_profiles)
                    }
                    crate::domain::conversation::ConversationKind::DirectMessage => {
                        let username = first_dm_username(&active_conversation.title)
                            .unwrap_or("")
                            .trim();
                        if username.is_empty() {
                            None
                        } else {
                            let lower_id = UserId::new(username.to_ascii_lowercase());
                            let profile =
                                snapshot.backend.user_profiles.get(&lower_id).or_else(|| {
                                    snapshot
                                        .backend
                                        .user_profiles
                                        .get(&UserId::new(username.to_string()))
                                });

                            if let Some(profile) = profile {
                                let display_name = profile.display_name.trim();
                                if !display_name.is_empty()
                                    && !display_name.eq_ignore_ascii_case(username)
                                {
                                    models.conversation.summary.title = display_name.to_string();
                                } else {
                                    models.conversation.summary.title = username.to_string();
                                }
                                profile.avatar_asset.clone()
                            } else {
                                models.conversation.summary.title = username.to_string();
                                None
                            }
                        }
                    }
                    crate::domain::conversation::ConversationKind::GroupDirectMessage => {
                        let mut participants = Vec::new();
                        for username in active_conversation
                            .title
                            .split(',')
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            let lower_id = UserId::new(username.to_ascii_lowercase());
                            let profile =
                                snapshot.backend.user_profiles.get(&lower_id).or_else(|| {
                                    snapshot
                                        .backend
                                        .user_profiles
                                        .get(&UserId::new(username.to_string()))
                                });
                            let display_name = profile
                                .map(|profile| profile.display_name.trim())
                                .filter(|name| !name.is_empty())
                                .filter(|name| !name.eq_ignore_ascii_case(username))
                                .unwrap_or(username);
                            participants.push(display_name.to_string());
                        }
                        if !participants.is_empty() {
                            models.conversation.summary.title = participants.join(", ");
                        }
                        None
                    }
                };
            } else if let Some(conversation_id) = snapshot.timeline.conversation_id.clone() {
                if let Some((cached_summary, cached_can_post)) =
                    inputs.preview_summaries.get(&conversation_id).cloned()
                {
                    models.conversation.summary = cached_summary;
                    models.conversation.can_post = cached_can_post;
                    models.conversation.avatar_asset = None;
                } else {
                    models.conversation.avatar_asset = None;
                    if models.conversation.summary.id != conversation_id {
                        models.conversation.summary.id = conversation_id.clone();
                    }
                }
                models.composer.conversation_id = conversation_id;
            }
        }
        models.conversation.pinned_message =
            snapshot
                .timeline
                .conversation_id
                .as_ref()
                .and_then(|conversation_id| {
                    snapshot
                        .backend
                        .conversation_pins
                        .get(conversation_id)
                        .and_then(|pinned| pinned.items.first().cloned())
                        .filter(|pinned_item| {
                            models
                                .settings
                                .dismissed_pinned_items
                                .get(&conversation_id.0)
                                .is_none_or(|dismissed_id| dismissed_id != &pinned_item.id)
                        })
                });
        models.conversation.details = snapshot
            .timeline
            .conversation_id
            .as_ref()
            .filter(|conversation_id| **conversation_id == models.conversation.summary.id)
            .map(|_| {
                let mut details = build_channel_details(
                    &models.conversation.summary,
                    models.conversation.member_count,
                    models.conversation.can_post,
                    models.conversation.is_archived,
                    &snapshot.backend,
                    current_user_id.as_ref(),
                );
                details.can_leave &= snapshot
                    .workspace
                    .channels
                    .iter()
                    .any(|channel| channel.id == details.conversation_id)
                    && !inputs
                        .preview_summaries
                        .contains_key(&details.conversation_id);
                details.leave_error = snapshot
                    .workspace
                    .leave_channel_errors
                    .get(&details.conversation_id)
                    .cloned();
                details.is_leaving = snapshot
                    .workspace
                    .leaving_conversation_ids
                    .contains(&details.conversation_id);
                details
            });
        let has_pinned_banner = models.conversation.pinned_message.is_some();
        let pinned_banner_visibility_changed = previous_has_pinned_banner != has_pinned_banner;

        let timeline_messages = &snapshot.timeline.messages;
        let loading_older = snapshot.timeline.loading_older;
        let loading_older_just_finished = self.last_timeline_loading_older && !loading_older;
        self.last_timeline_loading_older = loading_older;
        let timeline_link_previews_sig = timeline_link_previews_signature(
            snapshot.timeline.conversation_id.as_ref(),
            timeline_messages,
        );
        let timeline_link_previews_changed =
            self.timeline_link_previews_sig != Some(timeline_link_previews_sig);
        let timeline_rows_sig = timeline_rows_input_signature(
            snapshot.timeline.conversation_id.as_ref(),
            timeline_messages,
            snapshot.timeline.unread_marker.as_ref(),
            loading_older,
            &snapshot.backend.user_profiles,
            &snapshot.backend.user_affinities,
            &snapshot.backend.user_presences,
            current_user_id.as_ref(),
            current_user_avatar.as_deref(),
        );
        let timeline_rows_changed =
            conversation_changed || self.timeline_rows_sig != Some(timeline_rows_sig);
        if timeline_rows_changed {
            let mut rows: Vec<super::timeline_model::TimelineRow> = Vec::new();
            let unread_marker = snapshot.timeline.unread_marker.clone();
            let mut unread_divider_inserted = unread_marker.is_none();
            let mut previous_message: Option<(UserId, Option<i64>, bool)> = None;
            let mut day_dividers =
                super::timeline_model::DayDividers::new(chrono::Local::now().date_naive());

            for message in timeline_messages {
                let renders = !is_non_text_placeholder_message(message)
                    && !matches!(message.event, Some(ChatEvent::MessageDeleted { .. }));
                // Open each local calendar day with a divider; the row timestamps
                // then only need the time of day.
                if renders && let Some(label) = day_dividers.before(message.timestamp_ms) {
                    rows.push(super::timeline_model::TimelineRow::DateDivider(label));
                    previous_message = None;
                }
                if !unread_divider_inserted
                    && unread_marker.as_ref().is_some_and(|marker| {
                        message_starts_unread_region(message, marker, current_user_id.as_ref())
                    })
                {
                    rows.push(super::timeline_model::TimelineRow::UnreadDivider(
                        "New messages".to_string(),
                    ));
                    unread_divider_inserted = true;
                    previous_message = None;
                }

                if is_non_text_placeholder_message(message) {
                    continue;
                }

                if let Some(event) = &message.event {
                    if matches!(event, ChatEvent::MessageDeleted { .. }) {
                        continue;
                    }
                    rows.push(super::timeline_model::TimelineRow::SystemEvent(
                        format_chat_event(
                            event,
                            &message.author_id,
                            &snapshot.backend.user_profiles,
                            team_name_for_conversation(
                                &message.conversation_id,
                                &snapshot.backend.conversation_team_ids,
                                &models.workspace.channels,
                            ),
                        ),
                    ));
                    previous_message = None;
                    continue;
                }

                let author_profile = snapshot
                    .backend
                    .user_profiles
                    .get(&message.author_id)
                    .or_else(|| {
                        let lower = message.author_id.0.to_ascii_lowercase();
                        if lower == message.author_id.0 {
                            None
                        } else {
                            snapshot.backend.user_profiles.get(&UserId::new(lower))
                        }
                    });
                let display_name = author_profile
                    .map(|profile| profile.display_name.clone())
                    .unwrap_or_else(|| message.author_id.0.clone());
                let avatar_asset = author_profile
                    .and_then(|profile| profile.avatar_asset.clone())
                    .or_else(|| {
                        if current_user_id.as_ref() == Some(&message.author_id) {
                            current_user_avatar.clone()
                        } else {
                            None
                        }
                    });
                let author_presence = snapshot
                    .backend
                    .user_presences
                    .get(&message.author_id)
                    .cloned()
                    .or_else(|| {
                        let lower = message.author_id.0.to_ascii_lowercase();
                        if lower == message.author_id.0 {
                            None
                        } else {
                            snapshot
                                .backend
                                .user_presences
                                .get(&UserId::new(lower))
                                .cloned()
                        }
                    })
                    .unwrap_or(crate::domain::presence::Presence {
                        availability: crate::domain::presence::Availability::Offline,
                        status_text: None,
                    });
                let author_affinity = snapshot
                    .backend
                    .user_affinities
                    .get(&message.author_id)
                    .copied()
                    .or_else(|| {
                        let lower = message.author_id.0.to_ascii_lowercase();
                        if lower == message.author_id.0 {
                            None
                        } else {
                            snapshot
                                .backend
                                .user_affinities
                                .get(&UserId::new(lower))
                                .copied()
                        }
                    })
                    .unwrap_or_default();

                let show_header = previous_message.as_ref().is_none_or(
                    |(previous_author, previous_timestamp_ms, previous_was_thread_reply_stub)| {
                        if previous_author != &message.author_id {
                            return true;
                        }
                        // Don't group a main-chat message immediately after a thread-reply stub
                        // ("Replied in thread") from the same author.
                        if *previous_was_thread_reply_stub && message.reply_to.is_none() {
                            return true;
                        }
                        let (Some(previous_timestamp_ms), Some(current_timestamp_ms)) =
                            (*previous_timestamp_ms, message.timestamp_ms)
                        else {
                            return true;
                        };
                        if current_timestamp_ms <= previous_timestamp_ms {
                            return true;
                        }
                        const GROUP_WINDOW_MS: i64 = 15 * 60 * 1000;
                        current_timestamp_ms.saturating_sub(previous_timestamp_ms) > GROUP_WINDOW_MS
                    },
                );

                rows.push(super::timeline_model::TimelineRow::Message(
                    super::timeline_model::MessageRow {
                        author: crate::domain::user::UserSummary {
                            id: message.author_id.clone(),
                            display_name,
                            title: String::new(),
                            avatar_asset,
                            presence: author_presence,
                            affinity: author_affinity,
                        },
                        message: message.clone(),
                        show_header,
                    },
                ));
                previous_message = Some((
                    message.author_id.clone(),
                    message.timestamp_ms,
                    message.reply_to.is_some(),
                ));
            }

            if loading_older {
                rows.insert(
                    0,
                    super::timeline_model::TimelineRow::LoadingIndicator(
                        "Loading older messages…".to_string(),
                    ),
                );
            }

            models.timeline.rows = rows;
            models.timeline.thread_summaries =
                super::timeline_model::thread_summaries(&models.timeline.rows);
            if let Some(anchor_message_id) = timeline_anchor_message_id.as_ref()
                && models.timeline.pending_scroll_target.is_none()
            {
                outcome.anchor_row_index =
                    timeline_row_index_for_message(&models.timeline.rows, anchor_message_id);
            }
            apply_og_previews(models, inputs.og_service, inputs.github_pr_service);
            self.timeline_rows_sig = Some(timeline_rows_sig);
            self.timeline_link_previews_sig = Some(timeline_link_previews_sig);
        } else if timeline_link_previews_changed {
            sync_timeline_row_link_previews(
                models,
                timeline_messages,
                inputs.og_service,
                inputs.github_pr_service,
            );
            self.timeline_link_previews_sig = Some(timeline_link_previews_sig);
        }

        outcome.route_changed = route_changed;
        outcome.next_route = route_changed.then_some(next_route);
        outcome.conversation_changed = conversation_changed;
        outcome.timeline_rows_changed = timeline_rows_changed;
        outcome.timeline_emoji_changed = timeline_emoji_changed;
        outcome.timeline_reactions_changed = timeline_reactions_changed;
        outcome.timeline_link_previews_changed = timeline_link_previews_changed;
        outcome.pinned_banner_visibility_changed = pinned_banner_visibility_changed;
        outcome.has_pinned_banner = has_pinned_banner;
        outcome.loading_older = loading_older;
        outcome.loading_older_just_finished = loading_older_just_finished;
        outcome.was_near_bottom_before_sync = was_near_bottom_before_sync;
        outcome
    }
}

/// Projects only the thread pane; opening a thread needs nothing else.
pub fn sync_thread_pane(
    models: &mut AppModels,
    snapshot: &UiState,
    og_service: &OgService,
    github_pr_service: &GithubPrService,
) {
    models.thread_pane.open = snapshot.thread.open;
    models.thread_pane.root_message_id = snapshot.thread.root_message_id.clone();
    models.thread_pane.replies = snapshot.thread.replies.clone();
    models.thread_pane.reply_draft = snapshot.thread.reply_draft.clone();
    models.thread_pane.loading = snapshot.thread.loading;
    apply_link_previews_to_thread_replies(
        og_service,
        github_pr_service,
        &mut models.thread_pane.replies,
    );
}

fn is_non_text_placeholder_message(message: &MessageRecord) -> bool {
    if message.event.is_some() {
        return false;
    }
    if !message.attachments.is_empty() || !message.link_previews.is_empty() {
        return false;
    }
    if message.fragments.len() != 1 {
        return false;
    }
    matches!(
        message.fragments.first(),
        Some(MessageFragment::Text(text)) if text.trim() == NON_TEXT_PLACEHOLDER_BODY
    )
}

pub fn timeline_row_index_for_message(
    rows: &[super::timeline_model::TimelineRow],
    message_id: &MessageId,
) -> Option<usize> {
    rows.iter().position(|row| match row {
        super::timeline_model::TimelineRow::Message(message_row) => {
            &message_row.message.id == message_id
        }
        _ => false,
    })
}

fn build_channel_details(
    summary: &ConversationSummary,
    member_count: u32,
    can_post: bool,
    is_archived: bool,
    backend: &crate::state::state::BackendRuntimeState,
    current_user_id: Option<&UserId>,
) -> ChannelDetails {
    let mut members = Vec::new();
    let team_role_map = backend
        .conversation_team_ids
        .get(&summary.id)
        .and_then(|team_id| backend.team_roles.get(team_id));
    let mut sorted_member_ids = backend
        .conversation_members
        .get(&summary.id)
        .map(|state| state.members.clone())
        .unwrap_or_default();
    if sorted_member_ids.is_empty() {
        sorted_member_ids = match summary.kind {
            ConversationKind::DirectMessage | ConversationKind::GroupDirectMessage => summary
                .title
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|username| UserId::new(username.to_ascii_lowercase()))
                .collect(),
            ConversationKind::Channel => Vec::new(),
        };
    }
    sorted_member_ids.sort_by(|left, right| left.0.cmp(&right.0));
    for user_id in sorted_member_ids.into_iter() {
        let profile = backend.user_profiles.get(&user_id).or_else(|| {
            backend
                .user_profiles
                .get(&UserId::new(user_id.0.to_ascii_lowercase()))
        });
        let display_name = profile
            .map(|profile| profile.display_name.trim())
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string())
            .unwrap_or_else(|| user_id.0.clone());
        let preview = ChannelMemberPreview {
            user_id: user_id.clone(),
            display_name,
            avatar_asset: profile.and_then(|profile| profile.avatar_asset.clone()),
            affinity: backend
                .user_affinities
                .get(&user_id)
                .cloned()
                .or_else(|| {
                    backend
                        .user_affinities
                        .get(&UserId::new(user_id.0.to_ascii_lowercase()))
                        .cloned()
                })
                .unwrap_or(Affinity::None),
            is_team_admin_or_owner: team_role_map.is_some_and(|roles| {
                matches!(
                    roles.get(&user_id),
                    Some(TeamRoleKind::Admin | TeamRoleKind::Owner)
                )
            }),
            is_team_owner: team_role_map
                .is_some_and(|roles| matches!(roles.get(&user_id), Some(TeamRoleKind::Owner))),
            availability: backend
                .user_presences
                .get(&user_id)
                .or_else(|| {
                    backend
                        .user_presences
                        .get(&UserId::new(user_id.0.to_ascii_lowercase()))
                })
                .map(|presence| presence.availability.clone()),
        };
        members.push(preview);
    }
    // If we don't have a roster yet:
    // - DMs/GDMs: we can reasonably derive from title.
    // - Channels: we intentionally show empty until backend provides roster.

    members.sort_by(|left, right| {
        let left_name = left.display_name.trim().to_ascii_lowercase();
        let right_name = right.display_name.trim().to_ascii_lowercase();
        left_name
            .cmp(&right_name)
            .then_with(|| left.user_id.0.cmp(&right.user_id.0))
    });
    let member_preview = members.iter().take(6).cloned().collect::<Vec<_>>();

    let role = current_user_id
        .and_then(|user_id| team_role_map.and_then(|roles| roles.get(user_id)))
        .copied();
    let can_manage_members = matches!(role, Some(TeamRoleKind::Admin | TeamRoleKind::Owner));
    let notification_level = if summary.muted {
        NotificationLevel::Nothing
    } else {
        NotificationLevel::All
    };
    let topic = summary.topic.trim().to_string();

    let derived_member_count = if let Some(state) = backend.conversation_members.get(&summary.id) {
        state.members.len() as u32
    } else if member_count > 0 {
        member_count
    } else {
        members.len() as u32
    };

    ChannelDetails {
        conversation_id: summary.id.clone(),
        title: summary.title.clone(),
        topic: topic.clone(),
        kind: summary.kind.clone(),
        group: summary.group.clone(),
        member_count: derived_member_count,
        members,
        member_preview,
        notification_level,
        pinned_items: backend
            .conversation_pins
            .get(&summary.id)
            .map(|pinned| pinned.items.clone())
            .unwrap_or_default(),
        can_edit_topic: can_manage_members,
        can_manage_members,
        can_archive: can_manage_members,
        can_leave: summary.can_leave_channel(),
        is_leaving: false,
        leave_error: None,
        can_post,
        created_at: None,
        description: (!topic.is_empty()).then_some(topic),
        is_archived,
    }
}

/// Conversation id behind a conversation route, as keyed in `sidebar_previews`.
fn route_conversation_key(route: &Route) -> Option<&str> {
    match route {
        Route::Channel { channel_id, .. } => Some(channel_id.0.as_str()),
        Route::DirectMessage { dm_id, .. } => Some(dm_id.0.as_str()),
        _ => None,
    }
}

fn first_dm_username(title: &str) -> Option<&str> {
    title
        .split(',')
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn dm_participant_avatars(
    summary: &crate::domain::conversation::ConversationSummary,
    profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
    presences: &HashMap<UserId, crate::domain::presence::Presence>,
) -> Vec<AvatarParticipant> {
    let mut usernames = summary
        .title
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    usernames.sort();
    usernames.dedup();
    usernames
        .into_iter()
        .map(|username| {
            let user_id = UserId::new(username.clone());
            let profile = profiles.get(&user_id);
            AvatarParticipant {
                display_name: profile
                    .map(|profile| profile.display_name.clone())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or(username),
                asset: profile.and_then(|profile| profile.avatar_asset.clone()),
                presence: presences.get(&user_id).cloned(),
            }
        })
        .collect()
}

pub(crate) fn team_avatar_asset(
    summary: &ConversationSummary,
    profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
) -> Option<String> {
    let team = summary.group.as_ref()?.id.trim();
    if team.is_empty() {
        return None;
    }
    profiles
        .get(&UserId::new(format!("team:{team}")))?
        .avatar_asset
        .clone()
}

fn simple_team_avatar_asset(
    summary: &ConversationSummary,
    profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
) -> Option<String> {
    summary
        .is_simple_team()
        .then(|| team_avatar_asset(summary, profiles))
        .flatten()
}

fn profile_display_name(
    user_id: &UserId,
    user_profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
) -> String {
    user_profiles
        .get(user_id)
        .map(|profile| profile.display_name.clone())
        .unwrap_or_else(|| user_id.0.clone())
}

fn team_name_for_conversation(
    conversation_id: &ConversationId,
    conversation_team_ids: &HashMap<ConversationId, String>,
    channels: &[ConversationSummary],
) -> Option<String> {
    if let Some(from_channels) = channels
        .iter()
        .find(|summary| summary.id == *conversation_id)
        .and_then(|summary| summary.group.as_ref())
        .map(|group| group.id.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(from_channels);
    }

    conversation_team_ids
        .get(conversation_id)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| !looks_like_keybase_team_id(value))
}

pub fn looks_like_keybase_team_id(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() >= 32 && trimmed.bytes().all(|b| (b as char).is_ascii_hexdigit())
}

fn format_chat_event(
    event: &ChatEvent,
    author_id: &UserId,
    user_profiles: &HashMap<UserId, crate::state::state::UserProfileState>,
    team_name: Option<String>,
) -> super::timeline_model::SystemEventRow {
    use super::timeline_model::{EventSpan, SystemEventIcon, SystemEventRow};

    let author = profile_display_name(author_id, user_profiles);

    match event {
        ChatEvent::MemberJoined => SystemEventRow {
            icon: SystemEventIcon::Join,
            spans: vec![
                EventSpan::Actor(author),
                EventSpan::Text("joined the conversation".to_string()),
            ],
        },
        ChatEvent::MemberLeft => SystemEventRow {
            icon: SystemEventIcon::Leave,
            spans: vec![
                EventSpan::Actor(author),
                EventSpan::Text("left the conversation".to_string()),
            ],
        },
        ChatEvent::MembersAdded { user_ids, role } => {
            let role_suffix = role
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" as {value}"))
                .unwrap_or_default();
            let mut spans = vec![EventSpan::Actor(author), EventSpan::Text("added".into())];
            if user_ids.is_empty() {
                spans.push(EventSpan::Text(format!("members{role_suffix}")));
            } else {
                for (i, uid) in user_ids.iter().enumerate() {
                    spans.push(EventSpan::UserLink(uid.clone()));
                    if i + 1 < user_ids.len() {
                        spans.push(EventSpan::Text(",".into()));
                    }
                }
                if !role_suffix.is_empty() {
                    spans.push(EventSpan::Text(role_suffix));
                }
            }
            SystemEventRow {
                icon: SystemEventIcon::Add,
                spans,
            }
        }
        ChatEvent::MembersRemoved { user_ids } => {
            let mut spans = vec![EventSpan::Actor(author)];
            if user_ids.is_empty() {
                spans.push(EventSpan::Text("removed members".into()));
            } else {
                spans.push(EventSpan::Text("removed".into()));
                for (i, uid) in user_ids.iter().enumerate() {
                    spans.push(EventSpan::UserLink(uid.clone()));
                    if i + 1 < user_ids.len() {
                        spans.push(EventSpan::Text(",".into()));
                    }
                }
            }
            SystemEventRow {
                icon: SystemEventIcon::Remove,
                spans,
            }
        }
        ChatEvent::DescriptionChanged { description } => {
            let text = description
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("changed the channel description to \"{value}\""))
                .unwrap_or_else(|| "cleared the channel description".to_string());
            SystemEventRow {
                icon: SystemEventIcon::Description,
                spans: vec![EventSpan::Actor(author), EventSpan::Text(text)],
            }
        }
        ChatEvent::ChannelRenamed { new_name } => SystemEventRow {
            icon: SystemEventIcon::Description,
            spans: vec![
                EventSpan::Actor(author),
                EventSpan::Text(format!("set the channel name to #{new_name}")),
            ],
        },
        ChatEvent::AvatarChanged => SystemEventRow {
            icon: SystemEventIcon::Info,
            spans: vec![
                EventSpan::Actor(author),
                EventSpan::Text("changed the avatar".to_string()),
            ],
        },
        ChatEvent::MessagePinned { target_message_id } => {
            let text = if let Some(target_message_id) = target_message_id {
                format!("pinned message {}", target_message_id.0)
            } else {
                "pinned a message".to_string()
            };
            SystemEventRow {
                icon: SystemEventIcon::Pin,
                spans: vec![EventSpan::Actor(author), EventSpan::Text(text)],
            }
        }
        ChatEvent::MessageDeleted { target_message_id } => {
            let text = if let Some(target_message_id) = target_message_id {
                format!("deleted message {}", target_message_id.0)
            } else {
                "deleted a message".to_string()
            };
            SystemEventRow {
                icon: SystemEventIcon::Remove,
                spans: vec![EventSpan::Actor(author), EventSpan::Text(text)],
            }
        }
        ChatEvent::HistoryCleared => SystemEventRow {
            icon: SystemEventIcon::Remove,
            spans: vec![
                EventSpan::Actor(author),
                EventSpan::Text("cleared chat history".to_string()),
            ],
        },
        ChatEvent::RetentionChanged { summary } => SystemEventRow {
            icon: SystemEventIcon::Settings,
            spans: vec![EventSpan::Text(summary.clone())],
        },
        ChatEvent::ChannelCreated {
            channel_name,
            conv_id,
        } => SystemEventRow {
            icon: SystemEventIcon::Add,
            spans: vec![
                EventSpan::Actor(author),
                EventSpan::Text("created".to_string()),
                EventSpan::ChannelLink {
                    channel_name: channel_name.clone(),
                    team_name,
                    conv_id: conv_id.clone(),
                },
            ],
        },
        ChatEvent::Other { text } => SystemEventRow {
            icon: SystemEventIcon::Info,
            spans: vec![EventSpan::Text(text.clone())],
        },
    }
}

pub fn conversation_id_from_navigated_route(route: &Route) -> Option<ConversationId> {
    match route {
        Route::Channel { channel_id, .. } => Some(ConversationId::new(channel_id.0.clone())),
        Route::DirectMessage { dm_id, .. } => Some(ConversationId::new(dm_id.0.clone())),
        _ => None,
    }
}

fn message_starts_unread_region(
    message: &MessageRecord,
    read_upto: &MessageId,
    current_user: Option<&UserId>,
) -> bool {
    message.send_state == MessageSendState::Sent
        && !current_user.is_some_and(|user| user.0.eq_ignore_ascii_case(&message.author_id.0))
        && !is_non_text_placeholder_message(message)
        && !matches!(message.event, Some(ChatEvent::MessageDeleted { .. }))
        && message_id_is_after(&message.id, read_upto)
}

pub fn message_id_is_after(candidate: &MessageId, baseline: &MessageId) -> bool {
    match (
        candidate.0.parse::<u64>().ok(),
        baseline.0.parse::<u64>().ok(),
    ) {
        (Some(candidate), Some(baseline)) => candidate > baseline,
        _ => candidate.0 > baseline.0,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::sidebar_avatar_inputs_signature;
    use crate::domain::{
        ids::{ConversationId, MessageId, UserId},
        message::{ChatEvent, MessageFragment, MessageRecord, MessageSendState},
    };

    #[test]
    fn group_dm_avatar_projection_is_stable_and_tracks_every_peers_image() {
        use crate::domain::conversation::{ConversationKind, ConversationSummary};
        use crate::state::state::UserProfileState;
        let summary = ConversationSummary {
            id: ConversationId::new("group"),
            title: "bob, alice, carol, BOB".into(),
            kind: ConversationKind::GroupDirectMessage,
            topic: String::new(),
            group: None,
            unread_count: 0,
            mention_count: 0,
            muted: false,
            last_activity_ms: 0,
            last_message: None,
        };
        let mut profiles = std::collections::HashMap::from([
            (
                UserId::new("alice"),
                UserProfileState {
                    display_name: "Alice".into(),
                    avatar_asset: Some("/cache/alice.png".into()),
                    updated_ms: 1,
                },
            ),
            (
                UserId::new("bob"),
                UserProfileState {
                    display_name: "Robert".into(),
                    avatar_asset: Some("/cache/bob.png".into()),
                    updated_ms: 1,
                },
            ),
        ]);
        let avatars = super::dm_participant_avatars(&summary, &profiles, &HashMap::new());
        assert_eq!(avatars.len(), 3);
        assert_eq!(avatars[0].display_name, "Alice");
        assert_eq!(avatars[1].asset.as_deref(), Some("/cache/bob.png"));
        assert_eq!(avatars[2].display_name, "carol");
        let dms = std::slice::from_ref(&summary);
        let no_presence = HashMap::new();
        let before = sidebar_avatar_inputs_signature(&[], dms, &[], &profiles, &no_presence);
        profiles.get_mut(&UserId::new("bob")).unwrap().avatar_asset =
            Some("/cache/bob-new.png".into());
        let presences = HashMap::from([(
            UserId::new("bob"),
            crate::domain::presence::Presence {
                availability: crate::domain::presence::Availability::Active,
                status_text: None,
            },
        )]);
        let after_avatar = sidebar_avatar_inputs_signature(&[], dms, &[], &profiles, &no_presence);
        assert_ne!(before, after_avatar);
        assert_ne!(
            after_avatar,
            sidebar_avatar_inputs_signature(&[], dms, &[], &profiles, &presences)
        );
        let avatars = super::dm_participant_avatars(&summary, &profiles, &presences);
        assert_eq!(
            avatars[1]
                .presence
                .as_ref()
                .map(|presence| &presence.availability),
            Some(&crate::domain::presence::Availability::Active)
        );
    }

    #[test]
    fn simple_team_sidebar_icons_share_header_assets_and_invalidate_when_loaded() {
        use crate::domain::conversation::{
            ConversationGroup, ConversationKind, ConversationSummary, ConversationTeamType,
        };
        let mut summary = ConversationSummary {
            id: ConversationId::new("example_team-general"),
            title: "general".into(),
            kind: ConversationKind::Channel,
            topic: String::new(),
            group: Some(ConversationGroup {
                id: "example_team".into(),
                display_name: "example_team".into(),
                team_type: ConversationTeamType::Simple,
            }),
            unread_count: 0,
            mention_count: 0,
            muted: false,
            last_activity_ms: 0,
            last_message: None,
        };
        let mut profiles = std::collections::HashMap::new();
        let missing = sidebar_avatar_inputs_signature(
            &[],
            &[],
            &[summary.clone()],
            &profiles,
            &HashMap::new(),
        );
        assert_eq!(super::simple_team_avatar_asset(&summary, &profiles), None);
        profiles.insert(
            UserId::new("team:example_team"),
            crate::state::state::UserProfileState {
                display_name: "example_team".into(),
                avatar_asset: Some("/cache/example_team.png".into()),
                updated_ms: 1,
            },
        );
        assert_eq!(
            super::simple_team_avatar_asset(&summary, &profiles),
            super::team_avatar_asset(&summary, &profiles)
        );
        assert_eq!(
            super::simple_team_avatar_asset(&summary, &profiles).as_deref(),
            Some("/cache/example_team.png")
        );
        assert_ne!(
            missing,
            sidebar_avatar_inputs_signature(
                &[],
                &[],
                &[summary.clone()],
                &profiles,
                &HashMap::new()
            ),
            "an asynchronously loaded team icon must invalidate the sidebar cache"
        );
        summary.group.as_mut().unwrap().team_type = ConversationTeamType::Complex;
        assert_eq!(
            super::simple_team_avatar_asset(&summary, &profiles),
            None,
            "ordinary channel rows keep their hash icon"
        );
        let before = sidebar_avatar_inputs_signature(
            &[],
            &[],
            &[summary.clone()],
            &profiles,
            &HashMap::new(),
        );
        profiles
            .get_mut(&UserId::new("team:example_team"))
            .unwrap()
            .avatar_asset = Some("/cache/example_team-updated.png".into());
        assert_ne!(
            before,
            sidebar_avatar_inputs_signature(&[], &[], &[summary], &profiles, &HashMap::new()),
            "complex team header icons must also refresh when their image changes"
        );
    }

    fn test_message(id: &str, event: Option<ChatEvent>) -> MessageRecord {
        MessageRecord {
            id: MessageId::new(id),
            conversation_id: ConversationId::new("kb_conv:test"),
            author_id: UserId::new("alice"),
            reply_to: None,
            thread_root_id: None,
            timestamp_ms: None,
            event,
            link_previews: Vec::new(),
            permalink: String::new(),
            fragments: vec![MessageFragment::Text(format!("message-{id}"))],
            source_text: None,
            attachments: Vec::new(),
            reactions: Vec::new(),
            thread_reply_count: 0,
            send_state: MessageSendState::Sent,
            edited: None,
        }
    }

    #[test]
    fn own_messages_never_start_an_unread_region_but_incoming_messages_still_do() {
        let current = UserId::new("ALICE");
        let marker = MessageId::new("42");
        let mut message = test_message("local-pending-send", None);
        for state in [
            MessageSendState::Pending,
            MessageSendState::Failed,
            MessageSendState::Sent,
        ] {
            message.send_state = state;
            assert!(!super::message_starts_unread_region(
                &message,
                &marker,
                Some(&current)
            ));
        }
        message.id = MessageId::new("43");
        assert!(
            !super::message_starts_unread_region(&message, &marker, Some(&current)),
            "the confirmed self-message must not create the divider either"
        );
        message.author_id = UserId::new("bob");
        assert!(
            super::message_starts_unread_region(&message, &marker, Some(&current)),
            "an incoming message after an outgoing one still starts the unread region"
        );
        message.id = marker.clone();
        assert!(!super::message_starts_unread_region(
            &message,
            &marker,
            Some(&current)
        ));
        message.id = MessageId::new("44");
        message.event = Some(ChatEvent::MessageDeleted {
            target_message_id: None,
        });
        assert!(
            !super::message_starts_unread_region(&message, &marker, Some(&current)),
            "hidden deleted messages must not leave an orphan divider"
        );
    }
}
