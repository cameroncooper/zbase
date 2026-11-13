pub mod backends;
pub mod draft_store;
pub mod github_pr_service;
pub mod local_store;
pub mod media_service;
pub mod notifications_service;
pub mod og_service;
pub mod search_index;
pub mod search_service;
pub mod settings_store;
pub mod status_item;
pub mod sync_service;
pub mod uploads_service;
pub mod window_preferences;
pub mod window_preferences_writer;

use self::{
    draft_store::DraftStore, media_service::MediaService, search_service::SearchService,
    settings_store::SettingsStore, sync_service::SyncService, uploads_service::UploadsService,
};

#[derive(Default)]
pub struct AppServices {
    pub sync: SyncService,
    pub media: MediaService,
    pub search: SearchService,
    pub uploads: UploadsService,
    pub draft_store: DraftStore,
    pub settings_store: SettingsStore,
}

pub mod updates;
