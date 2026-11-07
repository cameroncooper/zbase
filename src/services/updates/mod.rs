//! Sparkle owns transport, signature verification, installation, and scheduling.
//! This module exposes presentation state; it never installs an archive itself.
pub mod session;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct UpdateSnapshot {
    pub state: String,
    pub version: String,
    pub notes: String,
    pub release_url: String,
    pub message: String,
    pub received: u64,
    pub total: u64,
    pub attention: bool,
    pub informational: bool,
    pub automatic_checks: bool,
    pub automatic_downloads: bool,
    pub last_check: f64,
}
impl Default for UpdateSnapshot {
    fn default() -> Self {
        Self { state: "disabled".into(), version: String::new(), notes: String::new(),
            release_url: "https://github.com/cameroncooper/zbase/releases".into(),
            message: "Automatic installation is available in official macOS releases. For Homebrew, use brew upgrade zbase.".into(),
            received: 0, total: 0, attention: false, informational: false,
            automatic_checks: false, automatic_downloads: false, last_check: 0. }
    }
}
impl UpdateSnapshot {
    pub fn has_reminder(&self) -> bool {
        matches!(
            self.state.as_str(),
            "available" | "downloaded" | "downloading" | "preparing" | "ready" | "installing"
        )
    }
    pub fn progress(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.received as f64 / self.total as f64).clamp(0., 1.) as f32)
    }
}
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum UpdateAction {
    Check = 0,
    Download = 1,
    Later = 2,
    Skip = 3,
    Cancel = 4,
    Restart = 5,
    ToggleChecks = 6,
    ToggleDownloads = 7,
}

#[cfg(all(target_os = "macos", feature = "macos-updater"))]
mod native {
    use super::*;
    use std::ffi::{CStr, c_char, c_void};
    unsafe extern "C" {
        fn zbase_update_start();
        fn zbase_update_snapshot() -> *mut c_char;
        fn zbase_update_action(action: i32);
        fn free(pointer: *mut c_void);
    }
    pub fn start() {
        unsafe { zbase_update_start() }
    }
    pub fn snapshot() -> UpdateSnapshot {
        unsafe {
            let pointer = zbase_update_snapshot();
            if pointer.is_null() {
                return UpdateSnapshot::default();
            }
            let result =
                serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()).unwrap_or_default();
            free(pointer.cast());
            result
        }
    }
    pub fn action(action: UpdateAction) {
        unsafe { zbase_update_action(action as i32) }
    }
}
#[cfg(not(all(target_os = "macos", feature = "macos-updater")))]
mod native {
    use super::*;
    pub fn start() {}
    pub fn snapshot() -> UpdateSnapshot {
        UpdateSnapshot::default()
    }
    pub fn action(_: UpdateAction) {}
}
pub use native::{action, snapshot, start};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_progress_handles_unknown_and_incorrect_content_lengths() {
        let mut state = UpdateSnapshot::default();
        assert_eq!(state.progress(), None);
        state.total = 100;
        state.received = 150;
        assert_eq!(state.progress(), Some(1.));
        state.received = 25;
        assert_eq!(state.progress(), Some(0.25));
        assert!(!state.has_reminder());
        state.state = "ready".into();
        assert!(state.has_reminder());
    }
}
