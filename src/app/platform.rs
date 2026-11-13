//! Opening a downloaded file needs a launch result to offer Reveal on failure.
use std::path::Path;

#[cfg(target_os = "windows")]
pub fn open_file(path: &Path) -> bool {
    use windows::{
        Win32::{
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
            UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWDEFAULT},
        },
        core::{HSTRING, w},
    };
    // Called on a background worker. Shell extensions can require COM; balance
    // only our successful initialization, preserving any existing apartment.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWDEFAULT,
        );
        if initialized {
            CoUninitialize();
        }
        result.0 as isize > 32
    }
}

#[cfg(not(target_os = "windows"))]
pub fn open_file(path: &Path) -> bool {
    std::process::Command::new("open")
        .arg(path)
        .status()
        .is_ok_and(|status| status.success())
}

/// Shifts a native window horizontally so a pane can expand on screen.
///
/// `dx` is in logical pixels and positive moves right. Returns whether the
/// platform applied the move; callers keep the window inside its display when
/// it cannot.
#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
pub fn move_window_by_x(window: &gpui_kit::Window, dx: f32) -> bool {
    use cocoa::{
        appkit::NSWindow,
        base::{YES, id},
    };
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if dx.abs() < 0.5 {
        return true;
    }
    let Ok(handle) = <gpui_kit::Window as HasWindowHandle>::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return false;
    };
    // SAFETY: The handle came from a live GPUI window. Resolve the NSWindow and
    // retain it for the deferred block.
    let ns_window = unsafe {
        let view = appkit.ns_view.as_ptr() as id;
        let ns_window: id = msg_send![view, window];
        if ns_window.is_null() {
            return false;
        }
        let _: id = msg_send![ns_window, retain];
        ns_window as usize
    };
    // AppKit reports the resulting move synchronously. GPUI holds a mutable App
    // borrow while running frame callbacks, so move after this callback returns
    // to avoid re-entering the app.
    dispatch::Queue::main().exec_async(move || {
        // SAFETY: The window was retained for this block and AppKit calls are on
        // the main thread.
        unsafe {
            let ns_window = ns_window as id;
            let mut frame = NSWindow::frame(ns_window);
            frame.origin.x += f64::from(dx);
            NSWindow::setFrame_display_(ns_window, frame, YES);
            let _: () = msg_send![ns_window, release];
        }
    });
    true
}

#[cfg(not(target_os = "macos"))]
pub fn move_window_by_x(_window: &gpui_kit::Window, _dx: f32) -> bool {
    false
}

/// Takes a window off screen without closing it or hiding the app, so zBase's
/// other windows (the menu bar panel) can still appear on their own.
/// Activating the window shows it again.
#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
pub fn hide_window(window: &gpui_kit::Window) {
    use cocoa::base::{id, nil};
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = <gpui_kit::Window as HasWindowHandle>::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: The handle came from a live GPUI window. Resolve the NSWindow and
    // retain it for the deferred block.
    let ns_window = unsafe {
        let view = appkit.ns_view.as_ptr() as id;
        let ns_window: id = msg_send![view, window];
        if ns_window.is_null() {
            return;
        }
        let _: id = msg_send![ns_window, retain];
        ns_window as usize
    };
    // Ordering out reports focus changes synchronously; do it after this
    // callback returns so they don't re-enter GPUI's App borrow.
    dispatch::Queue::main().exec_async(move || {
        // SAFETY: The window was retained for this block and AppKit calls are on
        // the main thread.
        unsafe {
            let ns_window = ns_window as id;
            let _: () = msg_send![ns_window, orderOut: nil];
            let _: () = msg_send![ns_window, release];
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn hide_window(_window: &gpui_kit::Window) {}
