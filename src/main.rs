//! zBase: a desktop Keybase chat client.
//!
//! The engine thread (`engine`) owns the store, the Keybase backend and the
//! view models, and publishes model sections to the web view in `ui/`, which
//! renders them and sends user intents back.

// Release builds on Windows are GUI apps, so no console window opens with them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// Models and services keep helpers that only some views reach.
#![allow(dead_code)]

mod domain;
mod engine;
mod models;
mod services;
mod state;
mod util;

use std::sync::mpsc::Sender;

use tauri::{Manager, State};

use engine::Intent;

struct Engine(Sender<Intent>);

/// The frontend's only entry point: every user action is an [`Intent`].
#[tauri::command]
fn intent(engine: State<'_, Engine>, intent: Intent) -> Result<(), String> {
    engine
        .0
        .send(intent)
        .map_err(|_| "the engine thread has stopped".to_string())
}

fn parse_log_level(raw: &str) -> Option<tracing::Level> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "trace" => Some(tracing::Level::TRACE),
        "debug" => Some(tracing::Level::DEBUG),
        "info" => Some(tracing::Level::INFO),
        "warn" | "warning" => Some(tracing::Level::WARN),
        "error" => Some(tracing::Level::ERROR),
        _ => None,
    }
}

fn main() {
    let max_level = std::env::var("ZBASE_LOG_LEVEL")
        .ok()
        .and_then(|raw| parse_log_level(&raw))
        .unwrap_or(tracing::Level::ERROR);
    tracing_subscriber::fmt()
        .with_max_level(max_level)
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            for directory in engine::asset_directories() {
                app.asset_protocol_scope()
                    .allow_directory(&directory, true)?;
            }
            app.manage(Engine(engine::spawn(app.handle().clone())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![intent])
        .run(tauri::generate_context!())
        .expect("failed to run zBase");
}
