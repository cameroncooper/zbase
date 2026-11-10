//! Demo mode: the fictional "lighthouse" workspace from `models::demo`, with no
//! Keybase service, for repeatable screenshots. The gpui app's `apply_demo_scene`.
//!
//! Enabled by `ZBASE_BENCH_USE_DEMO=1 ZBASE_BENCH_SKIP_BACKEND=1` with an
//! isolated `ZBASE_DATA_DIR`, in builds with `--features demo-fixtures`.
//! `ZBASE_DEMO_SCENE`, `ZBASE_DEMO_THEME` and `ZBASE_DEMO_SIDEBAR_STYLE` pick
//! what the window shows.

use serde_json::json;

use super::{Engine, Intent};
use crate::{
    domain::{
        backend::{
            AccountId, BackendCapabilities, BackendId, ProviderConversationRef, ProviderMessageRef,
            ProviderWorkspaceRef,
        },
        ids::ConversationId,
    },
    models::{
        AppModels, demo,
        demo_scene::{self, DemoScene},
        settings_model::SettingsModel,
        timeline_model::TimelineRow,
    },
    services::backends::{demo::DemoBackend, router::BackendRouter},
    state::{
        AccountState, AppStore, ConnectionState, MessageBinding, UiState, WorkspaceBinding,
        bindings::ConversationBinding,
    },
};

/// Whether this run shows the demo workspace instead of connecting to Keybase.
pub fn enabled() -> bool {
    let on = |name| {
        std::env::var(name).is_ok_and(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
    };
    let enabled = on("ZBASE_BENCH_USE_DEMO") && on("ZBASE_BENCH_SKIP_BACKEND");
    assert!(
        !enabled || std::env::var_os("ZBASE_DATA_DIR").is_some(),
        "Demo fixtures require an isolated ZBASE_DATA_DIR"
    );
    enabled
}

/// The demo models, store and backend, as `open_main_window` built them.
pub fn build(
    mut settings: SettingsModel,
    account_id: &AccountId,
    backend_id: &BackendId,
) -> (AppModels, AppStore, BackendRouter) {
    if let Some(theme) = demo_scene::theme_override() {
        settings.theme_mode = theme;
    }
    if let Some(style) = demo_scene::sidebar_style_override() {
        settings.sidebar_style = style;
    }
    let models = AppModels::demo_with_settings(settings);

    let mut store = AppStore::new(UiState::with_route(models.navigation.current.clone()));
    let mut router = BackendRouter::default();
    router.register_backend(Box::new(DemoBackend::new()));
    router.register_account(account_id.clone(), backend_id.clone());
    store.register_account(AccountState {
        account_id: account_id.clone(),
        backend_id: backend_id.clone(),
        display_name: "Keybase".into(),
        avatar: None,
        connection_state: ConnectionState::Disconnected,
        capabilities: BackendCapabilities::keybase_defaults(),
    });
    for workspace_id in &models.app.open_workspaces {
        let binding = WorkspaceBinding {
            workspace_id: workspace_id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_workspace_ref: ProviderWorkspaceRef::new(workspace_id.0.clone()),
        };
        store.register_workspace_binding(binding.clone());
        router.register_workspace_binding(binding);
    }
    for conversation in models
        .workspace
        .channels
        .iter()
        .chain(&models.workspace.direct_messages)
    {
        let binding = ConversationBinding {
            conversation_id: conversation.id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_conversation_ref: ProviderConversationRef::new(conversation.id.0.clone()),
        };
        store.register_conversation_binding(binding.clone());
        router.register_conversation_binding(binding);
    }
    for row in &models.timeline.rows {
        if let TimelineRow::Message(row) = row {
            let binding = MessageBinding {
                message_id: row.message.id.clone(),
                backend_id: backend_id.clone(),
                account_id: account_id.clone(),
                provider_message_ref: ProviderMessageRef::new(row.message.id.0.clone()),
            };
            store.register_message_binding(binding.clone());
            router.register_message_binding(binding);
        }
    }
    demo_scene::seed_store(&mut store, &models, account_id);

    // The fixtures' timeline lives in the models; give the store the same messages.
    let mut state = store.snapshot().clone();
    state.timeline.typing_text = models.timeline.typing_text.clone();
    state.timeline.messages = models
        .timeline
        .rows
        .iter()
        .filter_map(|row| match row {
            TimelineRow::Message(row) => Some(row.message.clone()),
            _ => None,
        })
        .collect();
    (models, AppStore::new(state), router)
}

/// `apply_demo_scene`, for the scenes the comparison screenshots use. Each one
/// sends the intents the interface would.
pub fn apply_scene(engine: &mut Engine) {
    let scene = DemoScene::from_env();
    tracing::warn!("demo.scene={}", scene.slug());
    let conversation = if scene == DemoScene::Dm {
        demo::DEMO_DM_ID
    } else {
        demo::DEMO_GENERAL_ID
    };
    let route = demo::demo_route_for_conversation(&ConversationId::new(conversation));
    super::shell::navigate_to(engine, route);
    engine.sync_models();
    let intent = match scene {
        DemoScene::ChatGrouped | DemoScene::ChatPreviewsGrouped => Some(json!({
            "area": "shell", "intent": { "type": "set_sidebar_mode", "mode": "Grouped" }
        })),
        DemoScene::Thread => Some(json!({
            "area": "right_pane",
            "intent": { "type": "open_thread", "root_id": demo::DEMO_THREAD_ROOT_ID }
        })),
        DemoScene::Profile => Some(json!({
            "area": "right_pane",
            "intent": { "type": "open_profile", "user_id": demo::DEMO_PROFILE_USER }
        })),
        DemoScene::Members => Some(json!({
            "area": "right_pane", "intent": { "type": "toggle_pane", "pane": "members" }
        })),
        DemoScene::QuickSwitcher => Some(json!({
            "area": "overlays", "intent": { "type": "toggle_quick_switcher" }
        })),
        DemoScene::Chat | DemoScene::ChatPreviews | DemoScene::Dm => None,
        other => {
            tracing::warn!("demo scene {} is not ported; showing chat", other.slug());
            None
        }
    };
    if let Some(intent) = intent {
        match serde_json::from_value::<Intent>(intent) {
            Ok(intent) => engine.handle(intent),
            Err(error) => tracing::warn!("demo intent: {error}"),
        }
    }
    engine.mark_dirty();
}
