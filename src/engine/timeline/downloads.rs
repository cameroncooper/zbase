//! Save / open for attachment download controls (views/app_window/downloads.rs).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use super::present::normalize_local_source_path;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    Downloading,
    Saved {
        path: String,
    },
    Failed {
        error: String,
    },
    /// No application could open the file; the control shows it in Finder.
    Reveal {
        path: String,
    },
}

pub fn key(source: &str, name: &str) -> String {
    format!("{source}\n{name}")
}

/// A download that ended: its key and outcome.
type Finished = (String, DownloadState);

pub struct Downloads {
    pub states: HashMap<String, DownloadState>,
    results: (Sender<Finished>, Receiver<Finished>),
}

impl Default for Downloads {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            results: mpsc::channel(),
        }
    }
}

impl Downloads {
    /// Drops states whose file was removed since, as `downloads::state` does.
    fn current(&mut self, key: &str) -> Option<DownloadState> {
        let state = self.states.get(key)?.clone();
        if let DownloadState::Saved { path } | DownloadState::Reveal { path } = &state
            && !Path::new(path).exists()
        {
            self.states.remove(key);
            return None;
        }
        Some(state)
    }

    /// `save_or_open_attachment`.
    pub fn activate(&mut self, app: &AppHandle, source: &str, name: &str) {
        let download_key = key(source, name);
        match self.current(&download_key) {
            Some(DownloadState::Downloading) => return,
            Some(DownloadState::Saved { path }) => {
                if app.opener().open_path(&path, None::<&str>).is_err() {
                    self.states
                        .insert(download_key, DownloadState::Reveal { path });
                }
                return;
            }
            Some(DownloadState::Reveal { path }) => {
                let _ = app.opener().reveal_item_in_dir(&path);
                return;
            }
            _ => {}
        }
        self.states
            .insert(download_key.clone(), DownloadState::Downloading);
        let sender = self.results.0.clone();
        let source = source.to_string();
        let name = name.to_string();
        let _ = thread::Builder::new()
            .name("zbase-download".into())
            .spawn(move || {
                let started = Instant::now();
                let result = save_copy(&source, &name);
                thread::sleep(Duration::from_millis(500).saturating_sub(started.elapsed()));
                let state = match result {
                    Ok(path) => DownloadState::Saved {
                        path: path.to_string_lossy().into_owned(),
                    },
                    Err(error) => DownloadState::Failed { error },
                };
                let _ = sender.send((download_key, state));
            });
    }

    /// Applies finished downloads; true when any changed.
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok((key, state)) = self.results.1.try_recv() {
            self.states.insert(key, state);
            changed = true;
        }
        changed
    }
}

fn home_dir() -> Option<std::ffi::OsString> {
    let home = std::env::var_os("HOME");
    #[cfg(target_os = "windows")]
    let home = home.or_else(|| std::env::var_os("USERPROFILE"));
    home
}

fn save_copy(source: &str, name: &str) -> Result<PathBuf, String> {
    let home = home_dir().ok_or("Unable to locate Downloads")?;
    save_copy_to(source, name, &PathBuf::from(home).join("Downloads"))
}

fn unique_copy_destination(downloads_dir: &Path, preferred_name: &str) -> PathBuf {
    let candidate = downloads_dir.join(preferred_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(preferred_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("attachment");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    for index in 1..500 {
        let numbered = if ext.is_empty() {
            format!("{stem} ({index})")
        } else {
            format!("{stem} ({index}).{ext}")
        };
        let path = downloads_dir.join(numbered);
        if !path.exists() {
            return path;
        }
    }
    candidate
}

fn save_copy_to(source: &str, name: &str, directory: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("attachment");
    let mut attempts = 0;
    let destination = loop {
        attempts += 1;
        if attempts > 500 {
            return Err("Too many copies of this filename in Downloads".into());
        }
        let path = unique_copy_destination(directory, name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let result = if source.starts_with("https://") || source.starts_with("http://") {
        std::process::Command::new("curl")
            .args([
                "-L",
                "--fail",
                "--silent",
                "--show-error",
                "--connect-timeout",
                "30",
                "--max-time",
                "1800",
                "--output",
            ])
            .arg(&destination)
            .arg(source)
            .output()
            .map_err(|e| e.to_string())
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
                }
            })
    } else {
        std::fs::copy(normalize_local_source_path(source), &destination)
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    if let Err(error) = result {
        let _ = std::fs::remove_file(&destination);
        return Err(error);
    }
    Ok(destination)
}
