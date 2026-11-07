//! One-shot, account-scoped restoration when an update restarts the application.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UpdateSession {
    pub account: String,
    pub version: String,
    pub workspace: String,
    pub conversation: Option<String>,
    pub channel: bool,
    pub anchor: Option<String>,
    pub anchor_offset: f32,
    pub drafts: Vec<SavedDraft>,
    pub thread: Option<String>,
    pub pane: String,
}
impl UpdateSession {
    pub fn belongs_to(&self, confirmed: Option<&str>, visible: Option<&str>) -> bool {
        !self.account.trim().is_empty()
            && confirmed.is_some_and(|account| account.eq_ignore_ascii_case(&self.account))
            && visible.is_some_and(|account| account.eq_ignore_ascii_case(&self.account))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SavedDraft {
    pub id: String,
    pub thread: bool,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reminder {
    pub version: String,
    pub after: i64,
}
impl Reminder {
    pub fn due(&self, version: &str, now: i64) -> bool {
        !version.is_empty() && (self.version != version || now >= self.after)
    }
    pub fn postpone(&mut self, version: &str, now: i64) {
        self.version = version.into();
        self.after = now + 86400;
    }
    pub fn load() -> Self {
        std::fs::read(path().with_file_name("update-reminder.json"))
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let destination = path().with_file_name("update-reminder.json");
        if std::fs::create_dir_all(destination.parent().unwrap()).is_ok() {
            let _ = std::fs::write(destination, serde_json::to_vec(self).unwrap());
        }
    }
}

pub fn path() -> std::path::PathBuf {
    crate::services::local_store::paths::data_root().join("update-session.json")
}
pub fn save(session: &UpdateSession, path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing session directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let data = serde_json::to_vec(session)?;
    if data.len() > 8 * 1024 * 1024 {
        return Err(io::Error::other(
            "Draft snapshot is too large to restore safely",
        ));
    }
    file.write_all(&data)?;
    file.sync_all()?;
    fs::rename(temporary, path)
}
pub fn load(path: &Path) -> Option<UpdateSession> {
    if fs::metadata(path).ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_drafts_require_the_confirmed_account_not_just_cached_identity() {
        let session = UpdateSession {
            account: "alice".into(),
            ..Default::default()
        };
        assert!(!session.belongs_to(None, Some("alice")));
        assert!(!session.belongs_to(Some("bob"), Some("alice")));
        assert!(!session.belongs_to(Some("alice"), Some("bob")));
        assert!(session.belongs_to(Some("Alice"), Some("alice")));
    }
    #[test]
    fn reminders_snooze_a_day_but_never_hide_a_newer_release() {
        let mut reminder = Reminder::default();
        assert!(reminder.due("1.0.15", 100));
        reminder.postpone("1.0.15", 100);
        assert!(!reminder.due("1.0.15", 101));
        assert!(reminder.due("1.0.15", 86500));
        assert!(reminder.due("1.0.16", 101));
    }
    #[test]
    fn update_session_round_trips_drafts_and_rejects_partial_json() {
        let path =
            std::env::temp_dir().join(format!("zbase-update-session-{}.json", std::process::id()));
        let session = UpdateSession {
            account: "alice".into(),
            drafts: vec![SavedDraft {
                id: "conv".into(),
                thread: false,
                text: "unsent\n🙂".into(),
            }],
            ..Default::default()
        };
        save(&session, &path).unwrap();
        assert_eq!(load(&path), Some(session));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::write(&path, b"{unfinished").unwrap();
        assert!(load(&path).is_none());
        fs::remove_file(path).unwrap();
    }
}
