pub mod action;
pub mod bindings;
pub mod effect;
pub mod event;
pub mod ids;
mod new_chat_search;
pub mod reducer;
#[expect(
    clippy::module_inception,
    reason = "Preserve the established state::state module path."
)]
pub mod state;
pub mod store;

pub use action::{DraftKey, UiAction};
pub use bindings::{MessageBinding, WorkspaceBinding};
pub use state::{AccountState, ConnectionState, UiState};
pub use store::AppStore;
