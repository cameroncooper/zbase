//! The navigation steps `AppWindow` runs when an overlay opens a destination
//! (`navigate_to`, `navigate_to_message`, `navigate_back`, `close_right_pane_state`).
//!
//! Navigation itself is the shell's; this keeps the pieces the shell keeps private.

use crate::{
    domain::{backend::ProviderMessageRef, ids::MessageId, route::Route},
    models::navigation_model::RightPaneMode,
    state::{MessageBinding, UiAction},
};

use super::super::Engine;

/// `AppWindow::navigate_to`, shared with the shell (settings open in their window).
pub fn navigate_to(engine: &mut Engine, route: Route) {
    super::super::shell::navigate_to(engine, route);
}

/// `AppWindow::navigate_to_message`, shared with the shell.
pub fn navigate_to_message(engine: &mut Engine, route: Route, message_id: MessageId) {
    super::super::shell::navigate_to_message(engine, route, message_id);
}

/// `AppWindow::navigate_back`.
pub fn navigate_back(engine: &mut Engine) {
    if engine.models.navigate_back() {
        let route = engine.models.navigation.current.clone();
        engine.dispatch(UiAction::Navigate(route.clone()));
        engine.models.expand_section_for_route(&route);
        engine.mark_dirty();
    }
}

/// `AppWindow::close_right_pane_state`, minus the closing animation snapshot.
pub fn close_right_pane_state(engine: &mut Engine) {
    engine.dispatch(UiAction::CloseRightPane);
    engine.models.set_right_pane(RightPaneMode::Hidden);
    engine.models.profile_panel.user_id = None;
    engine.models.profile_panel.profile = None;
}

/// `AppWindow::ensure_message_binding_for_active_conversation`.
pub fn ensure_message_binding_for_active_conversation(engine: &mut Engine, message_id: &MessageId) {
    let snapshot = engine.store.snapshot();
    if snapshot.backend.message_bindings.contains_key(message_id) {
        return;
    }
    let Some(conversation_id) = snapshot.timeline.conversation_id.clone() else {
        return;
    };
    let Some(conversation_binding) = snapshot
        .backend
        .conversation_bindings
        .get(&conversation_id)
        .cloned()
    else {
        return;
    };
    let binding = MessageBinding {
        message_id: message_id.clone(),
        backend_id: conversation_binding.backend_id,
        account_id: conversation_binding.account_id,
        provider_message_ref: ProviderMessageRef::new(message_id.0.clone()),
    };
    engine.router.register_message_binding(binding.clone());
    engine.store.register_message_binding(binding);
}
