//! Demo-only startup wiring: store seeding, scene selection and theme override.
//! Compiled only with `--features demo-fixtures` (and in tests).
use crate::{
    domain::backend::AccountId,
    domain::ids::ConversationId,
    models::{
        AppModels, demo,
        settings_model::{SidebarStyle, ThemeMode},
    },
    services::backends::demo::DemoBackend,
    state::{
        AppStore,
        action::UiAction,
        event::{BackendEvent, BootstrapPayload, BootstrapSource},
    },
};

pub const ENV_DEMO_SCENE: &str = "ZBASE_DEMO_SCENE";
pub const ENV_DEMO_THEME: &str = "ZBASE_DEMO_THEME";
pub const ENV_DEMO_SIDEBAR_STYLE: &str = "ZBASE_DEMO_SIDEBAR_STYLE";

/// A UI state the demo build can open into, for repeatable screenshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoScene {
    Chat,
    /// Chat with the sidebar in Grouped (teams and DMs) mode.
    ChatGrouped,
    /// Chat with the Previews sidebar style (latest message under each row).
    ChatPreviews,
    /// Previews sidebar style in Grouped mode.
    ChatPreviewsGrouped,
    /// Previews with live preview lines: someone typing, a draft, a failed send.
    ChatPreviewsLive,
    /// The menu under your name in the sidebar footer.
    IdentityMenu,
    /// The sidebar order pop-up on the list's label.
    OrderMenu,
    /// Send-state feedback: a pending, a failed and an uploading message.
    ChatStates,
    Thread,
    ThreadEmpty,
    Profile,
    Preferences,
    QuickSwitcher,
    QuickSwitcherSearch,
    Search,
    SearchEmpty,
    Dm,
    Members,
    FilesMedia,
    FilesDocuments,
    FilesEmpty,
    /// Members pane with a filter that matches nobody.
    MembersFilterEmpty,
    NewChat,
    /// Chat while the Keybase service connection is lost.
    Offline,
    /// Chat in a window that is not key: neutral selection, no focus ring.
    Inactive,
    /// The composer's format menu open over a draft with a word selected.
    ComposerFormat,
    ReactionTooltip,
    ReactionTooltipLong,
}

impl DemoScene {
    pub const ALL: [Self; 28] = [
        Self::Chat,
        Self::ChatGrouped,
        Self::ChatPreviews,
        Self::ChatPreviewsGrouped,
        Self::ChatPreviewsLive,
        Self::IdentityMenu,
        Self::OrderMenu,
        Self::ChatStates,
        Self::Thread,
        Self::ThreadEmpty,
        Self::Profile,
        Self::Preferences,
        Self::QuickSwitcher,
        Self::QuickSwitcherSearch,
        Self::Search,
        Self::SearchEmpty,
        Self::Dm,
        Self::Members,
        Self::FilesMedia,
        Self::FilesDocuments,
        Self::FilesEmpty,
        Self::MembersFilterEmpty,
        Self::NewChat,
        Self::Offline,
        Self::Inactive,
        Self::ComposerFormat,
        Self::ReactionTooltip,
        Self::ReactionTooltipLong,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::ChatGrouped => "chat-grouped",
            Self::ChatPreviews => "chat-previews",
            Self::ChatPreviewsGrouped => "chat-previews-grouped",
            Self::ChatPreviewsLive => "chat-previews-live",
            Self::IdentityMenu => "identity-menu",
            Self::OrderMenu => "order-menu",
            Self::ChatStates => "chat-states",
            Self::Thread => "thread",
            Self::ThreadEmpty => "thread-empty",
            Self::Profile => "profile",
            Self::Preferences => "preferences",
            Self::QuickSwitcher => "quick-switcher",
            Self::QuickSwitcherSearch => "quick-switcher-search",
            Self::Search => "search",
            Self::SearchEmpty => "search-empty",
            Self::Dm => "dm",
            Self::Members => "members",
            Self::FilesMedia => "files-media",
            Self::FilesDocuments => "files-documents",
            Self::FilesEmpty => "files-empty",
            Self::MembersFilterEmpty => "members-filter-empty",
            Self::NewChat => "new-chat",
            Self::Offline => "offline",
            Self::Inactive => "inactive",
            Self::ComposerFormat => "composer-format",
            Self::ReactionTooltip => "reaction-tooltip",
            Self::ReactionTooltipLong => "reaction-tooltip-long",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_ascii_lowercase().replace('_', "-");
        Self::ALL.into_iter().find(|scene| scene.slug() == raw)
    }

    /// `ZBASE_DEMO_SCENE`, defaulting to `chat`. Unknown names fall back to `chat`
    /// with a warning so a typo still produces a usable window.
    pub fn from_env() -> Self {
        let Ok(raw) = std::env::var(ENV_DEMO_SCENE) else {
            return Self::Chat;
        };
        Self::parse(&raw).unwrap_or_else(|| {
            tracing::warn!("unknown {ENV_DEMO_SCENE}={raw:?}; using chat");
            Self::Chat
        })
    }
}

impl DemoScene {
    /// Sidebar style the scene is about, if it is about one.
    pub fn sidebar_style(self) -> Option<SidebarStyle> {
        matches!(
            self,
            Self::ChatPreviews | Self::ChatPreviewsGrouped | Self::ChatPreviewsLive
        )
        .then_some(SidebarStyle::Previews)
    }
}

/// `ZBASE_DEMO_SIDEBAR_STYLE=compact|previews` overrides the sidebar style for
/// this run only; otherwise the scene's own style, if it has one.
pub fn sidebar_style_override() -> Option<SidebarStyle> {
    let Ok(raw) = std::env::var(ENV_DEMO_SIDEBAR_STYLE) else {
        return DemoScene::from_env().sidebar_style();
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "compact" => Some(SidebarStyle::Compact),
        "previews" => Some(SidebarStyle::Previews),
        other => {
            tracing::warn!("unknown {ENV_DEMO_SIDEBAR_STYLE}={other:?}; keeping saved style");
            DemoScene::from_env().sidebar_style()
        }
    }
}

/// `ZBASE_DEMO_THEME=light|dark` overrides the theme for this run only.
pub fn theme_override() -> Option<ThemeMode> {
    match std::env::var(ENV_DEMO_THEME)
        .ok()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        "system" => Some(ThemeMode::System),
        other => {
            tracing::warn!("unknown {ENV_DEMO_THEME}={other:?}; keeping saved theme");
            None
        }
    }
}

/// Feeds the store the events a freshly connected Keybase session would produce,
/// so the first projection already has profiles, presence, members and reactions.
pub fn seed_store(store: &mut AppStore, models: &AppModels, account_id: &AccountId) {
    let conversation_id = ConversationId::new(match &models.navigation.current {
        crate::domain::route::Route::Channel { channel_id, .. } => channel_id.0.clone(),
        crate::domain::route::Route::DirectMessage { dm_id, .. } => dm_id.0.clone(),
        _ => demo::DEMO_GENERAL_ID.to_string(),
    });
    store.dispatch_ui(UiAction::NavigateQuiet(models.navigation.current.clone()));
    let mut events = DemoBackend::people_events(account_id);
    events.push(BackendEvent::BootstrapLoaded {
        source: BootstrapSource::Server,
        account_id: account_id.clone(),
        payload: BootstrapPayload {
            workspace_ids: models.app.open_workspaces.clone(),
            active_workspace_id: Some(models.app.active_workspace_id.clone()),
            workspace_name: models.workspace.workspace_name.clone(),
            channels: models.workspace.channels.clone(),
            direct_messages: models.workspace.direct_messages.clone(),
            selected_conversation_id: Some(conversation_id.clone()),
            account_display_name: Some(demo::DEMO_ME.to_string()),
            ..BootstrapPayload::default()
        },
    });
    events.push(BackendEvent::AccountConnected {
        account_id: account_id.clone(),
    });
    events.extend(DemoBackend::conversation_events(conversation_id.clone()));
    events.extend(DemoBackend::member_events(conversation_id));
    for event in events {
        // Follow-up effects (reloads) are answered by the same fixtures; skip them.
        let _ = store.dispatch_backend(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_names_round_trip() {
        for scene in DemoScene::ALL {
            assert_eq!(DemoScene::parse(scene.slug()), Some(scene));
        }
        assert_eq!(
            DemoScene::parse("Quick_Switcher"),
            Some(DemoScene::QuickSwitcher)
        );
        assert_eq!(DemoScene::parse("nope"), None);
    }
}
