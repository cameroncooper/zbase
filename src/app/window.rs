use crate::{
    domain::{
        backend::{
            AccountId, BackendCapabilities, BackendId, ProviderConversationRef, ProviderMessageRef,
            ProviderWorkspaceRef,
        },
        channel_details::NotificationLevel,
        conversation::ConversationSummary,
        route::Route,
    },
    models::{
        AppModels,
        settings_model::{SettingsModel, SidebarStyle},
    },
    services::{
        backends::{keybase::KeybaseBackend, router::BackendRouter},
        local_store::LocalStore,
        notifications_service::ClickedNotification,
        settings_store::SettingsStore,
        window_preferences::WindowPreferences,
    },
    state::{
        AccountState, AppStore, ConnectionState, ConversationBinding, MessageBinding, UiState,
        WorkspaceBinding,
    },
    views::{
        WINDOW_MIN_HEIGHT_PX, WINDOW_MIN_WIDTH_PX, app_window::AppWindow, input::TextField,
        select::StyledSelect, sidebar_width_for_style,
    },
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Focusable, Global, TitlebarOptions,
    WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, point, px, size,
};
use std::env;
use std::sync::Arc;

const WINDOW_DEFAULT_HEIGHT_PX: f32 = 880.;

const ENV_BENCH_USE_DEMO: &str = "ZBASE_BENCH_USE_DEMO";

/// Saved settings, with the demo build's per-run overrides applied.
fn load_settings() -> SettingsModel {
    #[allow(unused_mut)]
    let mut settings = SettingsStore::load_from_disk().unwrap_or_default();
    #[cfg(any(test, feature = "demo-fixtures"))]
    if env_flag(ENV_BENCH_USE_DEMO) {
        // Demo settings live in ZBASE_DATA_DIR, so this never reaches the user's file.
        if let Some(theme_mode) = crate::app::demo::theme_override() {
            settings.theme_mode = theme_mode;
        }
        if let Some(size) = std::env::var("ZBASE_DEMO_TEXT_SIZE")
            .ok()
            .and_then(|value| value.parse().ok())
            .and_then(crate::models::settings_model::TextSize::from_percent)
        {
            settings.text_size = size;
        }
        if let Some(style) = crate::app::demo::sidebar_style_override() {
            settings.sidebar_style = style;
        }
    }
    settings
}
const ENV_BENCH_SKIP_BACKEND: &str = "ZBASE_BENCH_SKIP_BACKEND";

pub fn open_main_window(cx: &mut App) -> MainWindow {
    let settings = load_settings();
    crate::domain::user::set_name_style(settings.name_style);
    // A first window leaves the conversation its full minimum width beside a
    // wider Previews sidebar.
    let default_width = WINDOW_MIN_WIDTH_PX + sidebar_width_for_style(settings.sidebar_style)
        - sidebar_width_for_style(SidebarStyle::Compact);
    let default = Bounds::centered(
        None,
        size(px(default_width), px(WINDOW_DEFAULT_HEIGHT_PX)),
        cx,
    );
    let preferred = WindowPreferences::load().unwrap_or(WindowPreferences {
        x: default.origin.x.into(),
        y: default.origin.y.into(),
        width: default_width,
        height: WINDOW_DEFAULT_HEIGHT_PX,
    });
    let mut displays = cx.displays();
    if let Some(primary) = cx.primary_display() {
        displays.sort_by_key(|display| display.id() != primary.id());
    }
    let displays = displays
        .iter()
        .map(|display| {
            let bounds = display.bounds();
            WindowPreferences {
                x: bounds.origin.x.into(),
                y: bounds.origin.y.into(),
                width: bounds.size.width.into(),
                height: bounds.size.height.into(),
            }
        })
        .collect::<Vec<_>>();
    let preferred = preferred.constrained(&displays, (WINDOW_MIN_WIDTH_PX, WINDOW_MIN_HEIGHT_PX));
    let bounds = Bounds::new(
        point(px(preferred.x), px(preferred.y)),
        size(px(preferred.width), px(preferred.height)),
    );
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(WINDOW_MIN_WIDTH_PX), px(WINDOW_MIN_HEIGHT_PX))),
        titlebar: Some(TitlebarOptions {
            // Other platforms show the title in the title bar, task switcher and taskbar.
            title: (!cfg!(target_os = "macos")).then(|| "zBase".into()),
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: cfg!(target_os = "macos").then_some(point(px(18.), px(18.))),
        }),
        // Opaque: no surface lets the desktop show through.
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };

    let main = gpui_kit::open_window(options, cx, |window, cx| {
        let models = if env_flag(ENV_BENCH_USE_DEMO) {
            #[cfg(any(test, feature = "demo-fixtures"))]
            {
                assert!(env_flag(ENV_BENCH_SKIP_BACKEND) && std::env::var_os("ZBASE_DATA_DIR").is_some(),
                    "Demo fixtures require ZBASE_BENCH_SKIP_BACKEND=1 and an isolated ZBASE_DATA_DIR");
                AppModels::demo_with_settings(settings)
            }
            #[cfg(not(any(test, feature = "demo-fixtures")))]
            panic!("Demo fixtures are excluded from this build; explicitly build with --features demo-fixtures for isolated benchmarks");
        } else {
            AppModels::empty_with_settings(settings)
        };
        let app_store = build_app_store(&models);
        let local_store =
            Arc::new(LocalStore::open().unwrap_or_else(|error| {
                panic!("failed to initialize local RocksDB store: {error}")
            }));
        let backend_router = build_backend_router(&models, Arc::clone(&local_store));
        let search_text = models.search.query.clone();
        let emoji_picker_text = models.emoji_picker.query.clone();
        let find_in_chat_text = models.find_in_chat.query.clone();
        let composer_text = models.composer.draft_text.clone();
        let thread_text = models.thread_pane.reply_draft.clone();
        let sidebar_filter_text = models.sidebar.filter.clone();

        let root = cx.new(|cx| {
            let search_input = cx.new(|cx| {
                TextField::new(
                    cx.focus_handle(),
                    "Search messages, people, channels",
                    search_text.clone(),
                )
            });
            let quick_switcher_input = cx.new(|cx| {
                TextField::new_with_key_context(
                    cx.focus_handle(),
                    "Jump to channel, DM, or message",
                    String::new(),
                    "QuickSwitcherTextField",
                )
            });
            let new_chat_input = cx.new(|cx| {
                TextField::new(
                    cx.focus_handle(),
                    "Search people by username",
                    String::new(),
                )
            });
            let emoji_picker_input = cx.new(|cx| {
                TextField::new(cx.focus_handle(), "Search emoji", emoji_picker_text.clone())
            });
            let file_upload_caption_input = cx.new(|cx| {
                TextField::new_multiline(cx.focus_handle(), "Add a caption...", String::new(), 2)
            });
            let find_in_chat_input = cx.new(|cx| {
                TextField::new_with_key_context(
                    cx.focus_handle(),
                    "Find in this conversation",
                    find_in_chat_text.clone(),
                    "FindInChatTextField",
                )
            });
            let composer_input = cx.new(|cx| {
                TextField::new_auto_grow(
                    cx.focus_handle(),
                    "Message this conversation",
                    composer_text.clone(),
                    8,
                )
            });
            let thread_input = cx.new(|cx| {
                TextField::new_auto_grow(
                    cx.focus_handle(),
                    "Reply in thread",
                    thread_text.clone(),
                    6,
                )
            });
            let sidebar_filter_input = cx.new(|cx| {
                TextField::new(
                    cx.focus_handle(),
                    "Filter channels and DMs",
                    sidebar_filter_text,
                )
            });

            let members_filter_input = cx.new(|cx| {
                TextField::new(cx.focus_handle(), "Filter members", String::new())
            });

            let notification_selects = ["Desktop notifications", "Mobile notifications"].map(|label| {
                cx.new(|cx| StyledSelect::new(label,
                    NotificationLevel::ALL.into_iter().map(|level| level.label().into()).collect(), cx))
            });
            let mut root = AppWindow::new(
                models,
                app_store,
                backend_router,
                local_store,
                cx.focus_handle(),
                quick_switcher_input,
                new_chat_input,
                emoji_picker_input,
                file_upload_caption_input,
                find_in_chat_input,
                search_input,
                composer_input,
                thread_input,
                sidebar_filter_input,
                members_filter_input,
                notification_selects,
            );
            root.init(window, cx);
            root
        });
        window.focus(&root.focus_handle(cx), cx);
        root
    })
    .map(|(handle, view)| MainWindow {
        handle,
        view: view.downgrade(),
    })
    .expect("failed to open zbase window");
    cx.set_global(main.clone());
    main
}

/// The open main window. GPUI Kit wraps its root view, so it is kept here
/// rather than found by downcasting the window.
#[derive(Clone)]
pub struct MainWindow {
    handle: AnyWindowHandle,
    view: WeakEntity<AppWindow>,
}

impl Global for MainWindow {}

/// The main window's view, while it is open.
pub fn main_window_view(cx: &App) -> Option<WeakEntity<AppWindow>> {
    cx.try_global::<MainWindow>()
        .filter(|main| main.view.upgrade().is_some() && cx.windows().contains(&main.handle))
        .map(|main| main.view.clone())
}

/// Brings zBase forward with its main window active, reopening it if it was
/// closed, and updates its view.
fn update_main_window_in_front(
    cx: &mut App,
    update: impl FnOnce(&mut AppWindow, &mut Window, &mut Context<AppWindow>),
) {
    cx.activate(true);
    let main = cx
        .try_global::<MainWindow>()
        .filter(|main| main.view.upgrade().is_some() && cx.windows().contains(&main.handle))
        .cloned()
        .unwrap_or_else(|| open_main_window(cx));
    let _ = main.handle.update(cx, |_, window, cx| {
        window.activate_window();
        main.view.update(cx, |this, cx| update(this, window, cx))
    });
}

/// Brings zBase forward on the message a clicked banner points at, reopening
/// the main window if it was closed.
pub fn open_clicked_notification(click: ClickedNotification, cx: &mut App) {
    update_main_window_in_front(cx, |this, window, cx| {
        this.open_message(&click.conversation_id, click.message_id, window, cx);
    });
}

/// Brings zBase forward as it was.
pub fn show_main_window(cx: &mut App) {
    update_main_window_in_front(cx, |_, _, _| {});
}

/// Brings zBase forward on the team details of a channel whose logo was
/// clicked in the menu bar panel.
pub fn open_conversation_team(route: Route, cx: &mut App) {
    update_main_window_in_front(cx, |this, _, cx| {
        this.open_conversation_team(&route, cx);
    });
}

/// Brings zBase forward on a conversation picked in the menu bar panel.
pub fn open_route(route: Route, cx: &mut App) {
    update_main_window_in_front(cx, |this, window, cx| {
        this.navigate_to(route, window, cx);
    });
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn build_app_store(models: &AppModels) -> AppStore {
    let mut store = AppStore::new(UiState::with_route(models.navigation.current.clone()));
    let account_id = AccountId::new("account_demo_keybase");
    let backend_id = BackendId::new("keybase");

    store.register_account(AccountState {
        account_id: account_id.clone(),
        backend_id: backend_id.clone(),
        display_name: "Keybase".to_string(),
        avatar: None,
        connection_state: ConnectionState::Disconnected,
        capabilities: BackendCapabilities::keybase_defaults(),
    });

    for ws_id in &models.app.open_workspaces {
        store.register_workspace_binding(WorkspaceBinding {
            workspace_id: ws_id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_workspace_ref: ProviderWorkspaceRef::new(ws_id.0.clone()),
        });
    }

    for conversation in models
        .workspace
        .channels
        .iter()
        .chain(models.workspace.direct_messages.iter())
    {
        register_conversation_binding(&mut store, &backend_id, &account_id, conversation);
    }

    for row in &models.timeline.rows {
        let crate::models::timeline_model::TimelineRow::Message(message_row) = row else {
            continue;
        };

        let message = &message_row.message;
        store.register_message_binding(MessageBinding {
            message_id: message.id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_message_ref: ProviderMessageRef::new(message.id.0.clone()),
        });
    }

    #[cfg(any(test, feature = "demo-fixtures"))]
    if env_flag(ENV_BENCH_USE_DEMO) && env_flag(ENV_BENCH_SKIP_BACKEND) {
        crate::app::demo::seed_store(&mut store, models, &account_id);
    }

    store
}

fn build_backend_router(models: &AppModels, local_store: Arc<LocalStore>) -> BackendRouter {
    let mut router = BackendRouter::default();
    let account_id = AccountId::new("account_demo_keybase");
    let backend_id = BackendId::new("keybase");

    #[cfg(any(test, feature = "demo-fixtures"))]
    let demo_backend = env_flag(ENV_BENCH_USE_DEMO) && env_flag(ENV_BENCH_SKIP_BACKEND);
    #[cfg(not(any(test, feature = "demo-fixtures")))]
    let demo_backend = false;
    if demo_backend {
        #[cfg(any(test, feature = "demo-fixtures"))]
        router.register_backend(Box::new(crate::services::backends::demo::DemoBackend::new()));
        drop(local_store);
    } else if env_flag(ENV_BENCH_SKIP_BACKEND) {
        tracing::warn!("bench.backend.skip enabled at router setup");
        return router;
    } else {
        router.register_backend(Box::new(KeybaseBackend::new(local_store)));
    }
    router.register_account(account_id.clone(), backend_id.clone());

    for ws_id in &models.app.open_workspaces {
        router.register_workspace_binding(WorkspaceBinding {
            workspace_id: ws_id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_workspace_ref: ProviderWorkspaceRef::new(ws_id.0.clone()),
        });
    }

    for conversation in models
        .workspace
        .channels
        .iter()
        .chain(models.workspace.direct_messages.iter())
    {
        router.register_conversation_binding(ConversationBinding {
            conversation_id: conversation.id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_conversation_ref: ProviderConversationRef::new(conversation.id.0.clone()),
        });
    }

    for row in &models.timeline.rows {
        let crate::models::timeline_model::TimelineRow::Message(message_row) = row else {
            continue;
        };

        let message = &message_row.message;
        router.register_message_binding(MessageBinding {
            message_id: message.id.clone(),
            backend_id: backend_id.clone(),
            account_id: account_id.clone(),
            provider_message_ref: ProviderMessageRef::new(message.id.0.clone()),
        });
    }

    router
}

fn register_conversation_binding(
    store: &mut AppStore,
    backend_id: &BackendId,
    account_id: &AccountId,
    conversation: &ConversationSummary,
) {
    store.register_conversation_binding(ConversationBinding {
        conversation_id: conversation.id.clone(),
        backend_id: backend_id.clone(),
        account_id: account_id.clone(),
        provider_conversation_ref: ProviderConversationRef::new(conversation.id.0.clone()),
    });
}
