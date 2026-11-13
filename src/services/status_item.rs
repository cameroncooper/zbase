//! The menu bar icon: zBase's icon with the Dock's unread badge. It reports
//! clicks, so the recent-conversations panel can open under it. Only macOS has
//! one; other builds do nothing.

use gpui_kit::{Bounds, DisplayId, Pixels, point, px, size};
use std::sync::OnceLock;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone, Copy, Debug)]
pub enum StatusItemEvent {
    /// The icon was clicked; where it sits, from the top-left of `display`.
    Clicked {
        display: DisplayId,
        icon: Bounds<Pixels>,
    },
    /// While the panel is open, a press landed somewhere other than the panel
    /// or the icon.
    ClickedOutside,
}

static EVENTS: OnceLock<UnboundedSender<StatusItemEvent>> = OnceLock::new();

/// Prepares the icon once and returns its events, or `None` on platforms
/// without a menu bar. It appears once `set_visible(true)` asks for it.
pub fn start() -> Option<UnboundedReceiver<StatusItemEvent>> {
    let (sender, receiver) = unbounded_channel();
    EVENTS.set(sender).ok()?;
    platform::start().then_some(receiver)
}

/// Adds the icon to the menu bar or removes it.
pub fn set_visible(visible: bool) {
    platform::set_visible(visible);
}

/// Shows `count`, the Dock badge's number, on the icon; 0 shows the icon alone.
pub fn set_unread(count: u32) {
    platform::set_unread(count);
}

/// Report presses outside the panel while it is open.
pub fn watch_outside_clicks(watch: bool) {
    platform::watch_outside_clicks(watch);
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn send(event: StatusItemEvent) {
    if let Some(sender) = EVENTS.get() {
        let _ = sender.send(event);
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn clicked(display: u32, x: f64, y: f64, width: f64, height: f64) {
    send(StatusItemEvent::Clicked {
        display: DisplayId::new(u64::from(display)),
        icon: Bounds::new(
            point(px(x as f32), px(y as f32)),
            size(px(width as f32), px(height as f32)),
        ),
    });
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{StatusItemEvent, clicked, send};

    unsafe extern "C" {
        fn zbase_status_item_start(
            on_click: extern "C" fn(u32, f64, f64, f64, f64),
            on_outside_click: extern "C" fn(),
            icon_png: *const u8,
            icon_len: usize,
        );
        fn zbase_status_item_set_visible(visible: bool);
        fn zbase_status_item_set_unread(count: u32);
        fn zbase_status_item_watch_outside_clicks(watch: bool);
    }

    extern "C" fn on_click(display: u32, x: f64, y: f64, width: f64, height: f64) {
        clicked(display, x, y, width, height);
    }

    extern "C" fn on_outside_click() {
        send(StatusItemEvent::ClickedOutside);
    }

    /// zBase's icon, cropped to its rounded square and sized for the menu bar.
    const ICON_PNG: &[u8] = include_bytes!("../../assets/macos/MenuBarIcon.png");

    pub fn start() -> bool {
        // SAFETY: the callbacks match the bridge's types and live for the
        // program; the bridge copies the icon's bytes.
        unsafe {
            zbase_status_item_start(
                on_click,
                on_outside_click,
                ICON_PNG.as_ptr(),
                ICON_PNG.len(),
            )
        };
        true
    }

    pub fn set_visible(visible: bool) {
        // SAFETY: a no-op before `start`.
        unsafe { zbase_status_item_set_visible(visible) }
    }

    pub fn set_unread(count: u32) {
        // SAFETY: stores the count; draws it once the icon exists.
        unsafe { zbase_status_item_set_unread(count) }
    }

    pub fn watch_outside_clicks(watch: bool) {
        // SAFETY: a no-op before `start`.
        unsafe { zbase_status_item_watch_outside_clicks(watch) }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub fn start() -> bool {
        false
    }

    pub fn set_visible(_: bool) {}

    pub fn set_unread(_: u32) {}

    pub fn watch_outside_clicks(_: bool) {}
}
