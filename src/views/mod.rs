pub mod app_window;
pub mod avatar;
pub mod calls;
pub mod chat_message_preview;
pub mod code_highlight;
pub mod composer;
pub mod conversation;
pub mod github_pr_preview;
pub mod home;
pub mod inbox;
pub mod inline_markdown;
pub mod input;
#[cfg(test)]
mod kit_tests;
pub mod main_panel;
pub mod menubar_panel;
pub mod overlays;
mod pane_window;
pub mod popup_menu;
pub mod preferences;
pub mod profile;
mod quick_react_placement;
pub mod right_pane;
pub mod scaled_image;
pub mod search;
pub mod select;
pub mod selectable_text;
pub mod settings_window;
pub mod sidebar;
pub mod splash;
pub mod timeline;
pub mod tooltip;
pub mod truncated_line;
pub mod typography;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Background, BoxShadow, FontWeight, Hsla, ImageSource, IntoElement, ParentElement,
    SharedString, Styled, div, linear_color_stop, linear_gradient, point, px, rgb, svg,
};
use std::{cell::Cell, path::PathBuf};

use crate::{
    app::theme::ThemeVariant,
    domain::attachment::{AttachmentKind, AttachmentSource, AttachmentSummary},
    domain::message::{LinkPreview, MessageFragment},
};

thread_local! {
    static ACTIVE_THEME: Cell<ThemeVariant> = const { Cell::new(ThemeVariant::Light) };
    /// Whether the window being rendered is key. Inactive windows drop accent
    /// selection and focus rings, like native macOS controls.
    static WINDOW_ACTIVE: Cell<bool> = const { Cell::new(true) };
}

#[derive(Clone, Copy)]
struct Palette {
    app_bg: u32,
    titlebar_bg: u32,
    titlebar_surface: u32,
    rail_bg: u32,
    rail_surface: u32,
    sidebar_bg: u32,
    main_bg: u32,
    panel_bg: u32,
    panel_alt_bg: u32,
    border: u32,
    text_primary: u32,
    text_secondary: u32,
    text_inverted: u32,
    text_muted_inverted: u32,
    accent: u32,
    link: u32,
    unread_indicator: u32,
    accent_soft: u32,
    accent_strong: u32,
    sidebar_selected_bg: u32,
    sidebar_hover_bg: u32,
    field_bg: u32,
    field_border: u32,
    new_messages: u32,
    success: u32,
    success_soft: u32,
    warning: u32,
    warning_soft: u32,
    danger: u32,
    danger_soft: u32,
    affinity_positive: u32,
    affinity_positive_soft: u32,
    affinity_broken: u32,
    affinity_broken_soft: u32,
    mention: u32,
    mention_soft: u32,
    mention_you: u32,
    mention_you_soft: u32,
    inline_code_foreground: u32,
    inline_code_background: u32,
    selection: u32,
}

// Light palette follows Keybase shared/styles/colors.tsx: white, blueGrey,
// black_on_white, blue/blueDark, and the matching semantic status colors.
const LIGHT_PALETTE: Palette = Palette {
    app_bg: 0xffffff,
    titlebar_bg: 0xffffff,
    titlebar_surface: 0xe6e6e6,
    rail_bg: 0x3663ea,
    rail_surface: 0x4c8eff,
    sidebar_bg: 0xf2f4f7,
    main_bg: 0xffffff,
    panel_bg: 0xffffff,
    panel_alt_bg: 0xf0f0f0,
    border: 0xe6e6e6,
    text_primary: 0x262626,
    text_secondary: 0x666666,
    text_inverted: 0xffffff,
    text_muted_inverted: 0xcccccc,
    accent: 0x4c8eff,
    link: 0x3060de,
    unread_indicator: 0xff6f21,
    accent_soft: 0xebf2fc,
    accent_strong: 0x3663ea,
    sidebar_selected_bg: 0x3663ea,
    sidebar_hover_bg: 0xe4e7ec,
    field_bg: 0xf5f6f8,
    field_border: 0xd6d9de,
    new_messages: 0xd92d3f,
    success: 0x37bd99,
    success_soft: 0xe8faf6,
    warning: 0x75574e,
    warning_soft: 0xfffdcc,
    danger: 0xeb253b,
    danger_soft: 0xfaf2ed,
    affinity_positive: 0x087f63,
    affinity_positive_soft: 0xe8faf6,
    affinity_broken: 0xc81e33,
    affinity_broken_soft: 0xfaf2ed,
    mention: 0x3663ea,
    mention_soft: 0xebf2fc,
    mention_you: 0x191919,
    mention_you_soft: 0xffffc0,
    inline_code_foreground: 0x3663ea,
    inline_code_background: 0xfaf2ed,
    selection: 0x4c8eff4d,
};

// Keybase desktop dark colors (shared/styles/colors.tsx): white, blueGrey,
// grey, black_on_white, blue, blueDark, greenDark and semantic status colors.
// Soft fills composite those accents over the neutral chat background.
const DARK_PALETTE: Palette = Palette {
    app_bg: 0x191919,
    titlebar_bg: 0x191919,
    titlebar_surface: 0x333333,
    rail_bg: 0x2d2d2d,
    rail_surface: 0x3f3f3f,
    sidebar_bg: 0x202020,
    main_bg: 0x191919,
    panel_bg: 0x202020,
    panel_alt_bg: 0x2d2d2d,
    border: 0x3b3b3b,
    text_primary: 0xd9d9d9,
    text_secondary: 0xa3a3a3,
    text_inverted: 0xffffff,
    text_muted_inverted: 0xcccccc,
    accent: 0x4c8eff,
    link: 0x6a9dff,
    unread_indicator: 0xff6f21,
    accent_soft: 0x192344,
    accent_strong: 0x3b6fd6,
    sidebar_selected_bg: 0x2b4a85,
    sidebar_hover_bg: 0x2c2c2c,
    field_bg: 0x202020,
    field_border: 0x3b3b3b,
    new_messages: 0xff5c6e,
    success: 0x37bd99,
    success_soft: 0x1f3a33,
    warning: 0xffb800,
    warning_soft: 0x3b3115,
    danger: 0xff4d61,
    danger_soft: 0x3b2124,
    affinity_positive: 0x58d5b2,
    affinity_positive_soft: 0x193b32,
    affinity_broken: 0xff6b7d,
    affinity_broken_soft: 0x3b1b20,
    mention: 0x73a6ff,
    mention_soft: 0x192344,
    mention_you: 0xffd75e,
    mention_you_soft: 0x4a3f12,
    inline_code_foreground: 0x73a6ff,
    inline_code_background: 0x2d2d2d,
    selection: 0x4c8eff4d,
};

fn palette() -> Palette {
    ACTIVE_THEME.with(|theme| match theme.get() {
        ThemeVariant::Light => LIGHT_PALETTE,
        ThemeVariant::Dark => DARK_PALETTE,
    })
}

pub fn with_theme<T>(theme: ThemeVariant, f: impl FnOnce() -> T) -> T {
    ACTIVE_THEME.with(|active| {
        let previous = active.replace(theme);
        let result = f();
        active.set(previous);
        result
    })
}

/// Renders `f` as for a key (`true`) or background (`false`) window.
pub fn with_window_active<T>(active: bool, f: impl FnOnce() -> T) -> T {
    WINDOW_ACTIVE.with(|current| {
        let previous = current.replace(active);
        let result = f();
        current.set(previous);
        result
    })
}

pub fn window_is_active() -> bool {
    WINDOW_ACTIVE.with(Cell::get)
}

pub fn current_theme() -> ThemeVariant {
    ACTIVE_THEME.with(|theme| theme.get())
}

pub fn is_dark_theme() -> bool {
    current_theme() == ThemeVariant::Dark
}

pub fn app_bg() -> u32 {
    palette().app_bg
}

pub fn titlebar_bg() -> u32 {
    palette().titlebar_bg
}

pub fn titlebar_surface() -> u32 {
    palette().titlebar_surface
}

pub fn rail_bg() -> u32 {
    palette().rail_bg
}

pub fn rail_surface() -> u32 {
    palette().rail_surface
}

pub fn sidebar_bg() -> u32 {
    palette().sidebar_bg
}

pub fn main_bg() -> u32 {
    palette().main_bg
}

pub fn panel_bg() -> u32 {
    palette().panel_bg
}

pub fn panel_alt_bg() -> u32 {
    palette().panel_alt_bg
}

pub fn panel_surface() -> Hsla {
    tint(panel_bg(), if is_dark_theme() { 0.55 } else { 1.0 })
}

pub fn panel_alt_surface() -> Hsla {
    tint(panel_alt_bg(), if is_dark_theme() { 0.50 } else { 1.0 })
}

pub fn border() -> u32 {
    palette().border
}

pub fn text_primary() -> u32 {
    palette().text_primary
}

pub fn mono_font_family() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "monospace"
    }
}

pub fn mono_font_size() -> gpui_kit::Rems {
    typography::code()
}

pub fn text_secondary() -> u32 {
    palette().text_secondary
}

pub fn text_inverted() -> u32 {
    palette().text_inverted
}

pub fn text_muted_inverted() -> u32 {
    palette().text_muted_inverted
}

pub fn accent() -> u32 {
    palette().accent
}

pub fn link() -> u32 {
    palette().link
}

pub fn unread_indicator() -> u32 {
    palette().unread_indicator
}

pub fn accent_soft() -> u32 {
    palette().accent_soft
}

/// Accent dark enough to carry white text (>= 4.5:1): selected rows, badges, send.
pub fn accent_strong() -> u32 {
    palette().accent_strong
}

/// Selected sidebar row: accent while the window is key, neutral grey behind
/// other windows (as Finder and Mail do).
pub fn sidebar_selected_bg() -> u32 {
    match (window_is_active(), is_dark_theme()) {
        (true, _) => palette().sidebar_selected_bg,
        (false, false) => 0xdcdfe4,
        (false, true) => 0x3a3a3a,
    }
}

pub fn sidebar_selected_fg() -> u32 {
    if window_is_active() {
        0xffffff
    } else {
        text_primary()
    }
}

/// Secondary text on the selected sidebar row (team suffix, glyphs).
pub fn sidebar_selected_secondary() -> Hsla {
    match (window_is_active(), is_dark_theme()) {
        (true, _) => tint(sidebar_selected_fg(), 0.78),
        // One step darker than `text_secondary` to keep 4.5:1 on the grey fill.
        (false, false) => rgb(0x595959).into(),
        (false, true) => rgb(text_secondary()).into(),
    }
}

pub fn sidebar_hover_bg() -> u32 {
    palette().sidebar_hover_bg
}

/// Resting fill and border of text fields such as the composer.
pub fn field_bg() -> u32 {
    palette().field_bg
}

pub fn field_border() -> u32 {
    palette().field_border
}

/// Rule and label marking where unread messages begin in a timeline.
pub fn new_messages() -> u32 {
    palette().new_messages
}

/// Soft ring drawn around a focused field, outside its accent border.
pub fn focus_ring() -> BoxShadow {
    BoxShadow {
        inset: false,
        color: tint(accent(), if is_dark_theme() { 0.30 } else { 0.22 }),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(3.),
    }
}

/// Team role marker color; the same in both themes so the crown reads as one symbol.
pub fn role_crown() -> u32 {
    0xb7891d
}

pub fn success() -> u32 {
    palette().success
}

pub fn success_soft() -> u32 {
    palette().success_soft
}

pub fn warning() -> u32 {
    palette().warning
}

pub fn warning_soft() -> u32 {
    palette().warning_soft
}

/// Opaque fill of a warning strip (offline, connecting): amber, not error red.
pub fn warning_banner_bg() -> u32 {
    if is_dark_theme() { 0x3a2f1a } else { 0xfff7e6 }
}

/// Text and glyphs on [`warning_banner_bg`].
pub fn warning_banner_fg() -> u32 {
    if is_dark_theme() { 0xf5c46b } else { 0x8a5a00 }
}

pub fn danger() -> u32 {
    palette().danger
}

pub fn danger_soft() -> u32 {
    palette().danger_soft
}

pub fn affinity_positive() -> u32 {
    palette().affinity_positive
}

pub fn affinity_positive_soft() -> u32 {
    palette().affinity_positive_soft
}

pub fn affinity_broken() -> u32 {
    palette().affinity_broken
}

pub fn affinity_broken_soft() -> u32 {
    palette().affinity_broken_soft
}

pub fn mention() -> u32 {
    palette().mention
}

pub fn mention_soft() -> u32 {
    palette().mention_soft
}

pub fn mention_you() -> u32 {
    palette().mention_you
}

pub fn mention_you_soft() -> u32 {
    palette().mention_you_soft
}

/// Code-block syntax colors, tuned per theme to read at 4.5:1 or better on the
/// code block fill (`panel_alt_bg`). Light mode can't reuse the UI accents: the
/// bright blue and green are too pale on #f0f0f0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxColors {
    pub keyword: u32,
    pub function: u32,
    pub string: u32,
    pub number: u32,
    pub type_name: u32,
    pub comment: u32,
    pub punctuation: u32,
}

pub fn syntax_colors() -> SyntaxColors {
    if is_dark_theme() {
        SyntaxColors {
            keyword: text_primary(),
            function: 0x6a9dff,
            string: 0x37bd99,
            number: 0xffb800,
            type_name: 0x73a6ff,
            comment: text_secondary(),
            punctuation: text_secondary(),
        }
    } else {
        SyntaxColors {
            keyword: 0x2456bf,
            function: 0x2456bf,
            string: 0x17753f,
            number: 0x9c5700,
            type_name: 0x7c3aed,
            comment: text_secondary(),
            punctuation: text_secondary(),
        }
    }
}

pub fn inline_code_font_size() -> gpui_kit::Rems {
    typography::inline_code()
}

pub fn inline_code_foreground() -> u32 {
    palette().inline_code_foreground
}

pub fn inline_code_background() -> u32 {
    palette().inline_code_background
}

pub fn selection() -> u32 {
    palette().selection
}

pub fn mention_colors_for_user(
    affinity_index: &std::collections::HashMap<
        crate::domain::ids::UserId,
        crate::domain::affinity::Affinity,
    >,
    current_user_id: Option<&crate::domain::ids::UserId>,
    user_id: &crate::domain::ids::UserId,
) -> (u32, u32) {
    if current_user_id.is_some_and(|me| me.0.eq_ignore_ascii_case(&user_id.0)) {
        return (mention_you(), mention_you_soft());
    }
    let affinity = affinity_index
        .get(user_id)
        .copied()
        .or_else(|| {
            let lower = user_id.0.to_ascii_lowercase();
            if lower == user_id.0 {
                None
            } else {
                affinity_index
                    .get(&crate::domain::ids::UserId::new(lower))
                    .copied()
            }
        })
        .or_else(|| {
            // Some backend sources preserve original casing in user IDs, but we treat them
            // case-insensitively in the UI.
            affinity_index.iter().find_map(|(key, affinity)| {
                key.0.eq_ignore_ascii_case(&user_id.0).then_some(*affinity)
            })
        })
        .unwrap_or(crate::domain::affinity::Affinity::None);
    match affinity {
        crate::domain::affinity::Affinity::None => (mention(), mention_soft()),
        crate::domain::affinity::Affinity::Positive => {
            (affinity_positive(), affinity_positive_soft())
        }
        crate::domain::affinity::Affinity::Broken => (affinity_broken(), affinity_broken_soft()),
    }
}

pub const WINDOW_MIN_WIDTH_PX: f32 = 800.0;
pub const WINDOW_MIN_HEIGHT_PX: f32 = 680.0;
pub const SHELL_HORIZONTAL_PADDING_PX: f32 = 0.0;
pub const SHELL_GAP_PX: f32 = 0.0;
pub const SIDEBAR_WIDTH_PX: f32 = 236.0;
/// Previews put a 32px avatar, a time and a message beside each name; they get
/// more room so common names still fit before anything truncates.
pub const PREVIEWS_SIDEBAR_WIDTH_PX: f32 = 280.0;
pub const MAIN_PANEL_MIN_WIDTH_PX: f32 = 560.0;
pub const RIGHT_PANE_WIDTH_PX: f32 = 320.0;
pub const RIGHT_PANE_RESIZE_HANDLE_WIDTH_PX: f32 = 4.0;
pub const CUSTOM_EMOJI_REACTION_SIZE_PX: f32 = 16.0;
pub const CUSTOM_EMOJI_INLINE_SIZE_PX: f32 = 20.0;
pub const CUSTOM_EMOJI_EMOJI_ONLY_SIZE_PX: f32 = 48.0;

/// Default sidebar width for a row style.
pub fn sidebar_width_for_style(style: crate::models::settings_model::SidebarStyle) -> f32 {
    match style {
        crate::models::settings_model::SidebarStyle::Compact => SIDEBAR_WIDTH_PX,
        crate::models::settings_model::SidebarStyle::Previews => PREVIEWS_SIDEBAR_WIDTH_PX,
    }
}

pub fn tint(color: u32, opacity: f32) -> Hsla {
    Hsla::from(rgb(color)).opacity(opacity)
}

pub fn app_backdrop() -> Background {
    linear_gradient(
        135.,
        linear_color_stop(tint(app_bg(), 1.0), 0.0),
        linear_color_stop(tint(main_bg(), 1.0), 1.0),
    )
}

pub fn glass_surface() -> Hsla {
    rgb(panel_bg()).into()
}

pub fn glass_surface_strong() -> Hsla {
    rgb(panel_alt_bg()).into()
}

/// Base of the right pane (thread, members, profile). In dark it sits one step
/// above the conversation so the timeline stays dominant; raised controls inside
/// it keep `panel_alt_bg`.
pub fn right_pane_surface() -> Hsla {
    if is_dark_theme() {
        rgb(0x242424).into()
    } else {
        glass_surface_strong()
    }
}

/// Sidebar fill. Opaque: the desktop never shows through any app surface.
pub fn sidebar_surface() -> Hsla {
    rgb(sidebar_bg()).into()
}

pub fn glass_surface_dark() -> Hsla {
    rgb(sidebar_bg()).into()
}

pub fn modal_surface() -> Hsla {
    rgb(panel_bg()).into()
}

pub fn content_surface() -> Hsla {
    rgb(main_bg()).into()
}

pub fn subtle_surface() -> Hsla {
    tint(panel_alt_bg(), if is_dark_theme() { 0.88 } else { 1.0 })
}

pub fn shell_border() -> Hsla {
    if is_dark_theme() {
        tint(0xffffff, 0.10)
    } else {
        tint(0x000000, 0.10)
    }
}

pub fn shell_border_strong() -> Hsla {
    if is_dark_theme() {
        tint(0xffffff, 0.20)
    } else {
        tint(0x000000, 0.20)
    }
}

pub fn sidebar_splitter_bg() -> u32 {
    if is_dark_theme() { 0x181818 } else { 0xe9ecf0 }
}

/// The selected sidebar row: a card raised off the sidebar, white in light mode
/// like the conversation beside it, one step lighter than the sidebar in dark.
pub fn sidebar_card_bg() -> u32 {
    if is_dark_theme() { 0x2d2d2d } else { 0xffffff }
}

pub fn sidebar_card_shadow() -> Vec<BoxShadow> {
    let edge = |color: Hsla| BoxShadow {
        inset: false,
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
    };
    if is_dark_theme() {
        vec![edge(tint(0xffffff, 0.06))]
    } else {
        vec![
            BoxShadow {
                inset: false,
                color: tint(0x000000, 0.08),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
            },
            edge(tint(0x000000, 0.04)),
        ]
    }
}

pub fn floating_shadow() -> Vec<BoxShadow> {
    if is_dark_theme() {
        vec![
            BoxShadow {
                inset: false,
                color: tint(0x000000, 0.28),
                offset: point(px(0.), px(18.)),
                blur_radius: px(42.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: tint(0x000000, 0.14),
                offset: point(px(0.), px(6.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
            },
        ]
    } else {
        vec![
            BoxShadow {
                inset: false,
                color: tint(0x2b3440, 0.07),
                offset: point(px(0.), px(18.)),
                blur_radius: px(42.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: tint(0x2b3440, 0.03),
                offset: point(px(0.), px(6.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
            },
        ]
    }
}

pub fn card_shadow() -> Vec<BoxShadow> {
    if is_dark_theme() {
        vec![
            BoxShadow {
                inset: false,
                color: tint(0x000000, 0.20),
                offset: point(px(0.), px(10.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: tint(0x000000, 0.10),
                offset: point(px(0.), px(2.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
            },
        ]
    } else {
        vec![
            BoxShadow {
                inset: false,
                color: tint(0x2b3440, 0.05),
                offset: point(px(0.), px(10.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
            },
            BoxShadow {
                inset: false,
                color: tint(0x2b3440, 0.02),
                offset: point(px(0.), px(2.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
            },
        ]
    }
}

pub fn badge(label: impl Into<SharedString>, background: u32, foreground: u32) -> AnyElement {
    div()
        .px_2p5()
        .py_1()
        .rounded_full()
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .text_size(typography::caption())
        .font_weight(FontWeight::MEDIUM)
        .child(label.into())
        .into_any_element()
}

pub fn attachment_kind_tag(kind: &AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Image => "IMG",
        AttachmentKind::Video => "VID",
        AttachmentKind::Audio => "AUD",
        AttachmentKind::File => "FILE",
    }
}

pub fn attachment_size_text(size_bytes: u64) -> Option<String> {
    if size_bytes == 0 {
        return None;
    }
    if size_bytes < 1024 {
        return Some(format!("{size_bytes} B"));
    }
    const UNITS: &[&str] = &["KB", "MB", "GB", "TB"];
    let mut value = size_bytes as f64 / 1024.0;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }
    let unit = UNITS[unit_index];
    if value >= 10.0 {
        Some(format!("{value:.0} {unit}"))
    } else {
        Some(format!("{value:.1} {unit}"))
    }
}

/// "Uploading · 1.7 / 4.0 MB · 42%": the direction, both sizes in the total's
/// unit, and the share sent. Without a known total, just "Uploading…".
pub fn upload_status_label(sent_bytes: u64, total_bytes: u64) -> String {
    if total_bytes == 0 {
        return "Uploading…".to_string();
    }
    let sent_bytes = sent_bytes.min(total_bytes);
    let percent = (sent_bytes as f64 / total_bytes as f64 * 100.).floor() as u64;
    let Some(total) = attachment_size_text(total_bytes) else {
        return "Uploading…".to_string();
    };
    // Express the sent amount in the total's unit: "1.7 / 4.0 MB".
    let (number, unit) = total.split_once(' ').unwrap_or((total.as_str(), ""));
    let divisor = match unit {
        "KB" => 1024f64,
        "MB" => 1024f64.powi(2),
        "GB" => 1024f64.powi(3),
        "TB" => 1024f64.powi(4),
        _ => 1.,
    };
    let sent_value = sent_bytes as f64 / divisor;
    let decimals = if unit == "B" || !number.contains('.') {
        0
    } else {
        1
    };
    format!("Uploading · {sent_value:.decimals$} / {total} · {percent}%")
}

pub fn attachment_display_label(attachment: &AttachmentSummary) -> String {
    let prefix = attachment_kind_tag(&attachment.kind);
    if let Some(size_label) = attachment_size_text(attachment.size_bytes) {
        format!("[{prefix}] {} ({size_label})", attachment.name)
    } else {
        format!("[{prefix}] {}", attachment.name)
    }
}

pub fn attachment_is_animated_gif(attachment: &AttachmentSummary) -> bool {
    let name_lower = attachment.name.to_ascii_lowercase();
    if name_lower.ends_with(".gif") {
        return true;
    }
    if let Some(ref mime) = attachment.mime_type
        && mime.eq_ignore_ascii_case("image/gif")
    {
        return true;
    }
    false
}

pub fn attachment_image_source(attachment: &AttachmentSummary) -> Option<ImageSource> {
    if attachment_is_animated_gif(attachment) {
        attachment
            .source
            .as_ref()
            .cloned()
            .or_else(|| attachment.preview.as_ref().map(|p| p.source.clone()))
            .and_then(|source| image_source_from_attachment_source(&source))
    } else {
        attachment
            .preview
            .as_ref()
            .map(|preview| preview.source.clone())
            .or_else(|| attachment.source.as_ref().cloned())
            .and_then(|source| image_source_from_attachment_source(&source))
    }
}

/// `source` for an attachment drawn in a `width` by `height` point box,
/// pre-scaled to the device pixels it covers (see [`scaled_image`]). Animated
/// GIFs keep the plain loader, which animates their frames.
/// A link preview's image: a downloaded file, or a remote URL.
pub fn link_preview_image_source(source: &str) -> ImageSource {
    if source.starts_with("http://") || source.starts_with("https://") {
        ImageSource::from(SharedString::from(source.to_string()))
    } else {
        ImageSource::from(std::path::PathBuf::from(source))
    }
}

pub fn display_image_source(
    attachment: &AttachmentSummary,
    source: ImageSource,
    width: f32,
    height: f32,
    fit: scaled_image::Fit,
) -> ImageSource {
    if attachment_is_animated_gif(attachment) {
        source
    } else {
        scaled_image::scaled_for_box(source, width, height, fit)
    }
}

pub fn attachment_lightbox_source(attachment: &AttachmentSummary) -> Option<AttachmentSource> {
    attachment
        .source
        .as_ref()
        .cloned()
        .or_else(|| {
            attachment
                .preview
                .as_ref()
                .map(|preview| preview.source.clone())
        })
        .and_then(|source| {
            attachment_source_to_image_source(&source)
                .is_some()
                .then_some(source)
        })
}

pub fn image_source_from_attachment_source(source: &AttachmentSource) -> Option<ImageSource> {
    attachment_source_to_image_source(source)
}

pub fn attachment_open_target(attachment: &AttachmentSummary) -> Option<String> {
    attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|preview| &preview.source))
        .and_then(attachment_source_to_open_target)
}

pub fn attachment_local_path(attachment: &AttachmentSummary) -> Option<String> {
    attachment
        .source
        .as_ref()
        .or_else(|| attachment.preview.as_ref().map(|p| &p.source))
        .and_then(|source| match source {
            AttachmentSource::LocalPath(p) if !p.trim().is_empty() => Some(p.clone()),
            _ => None,
        })
}

fn attachment_source_to_image_source(source: &AttachmentSource) -> Option<ImageSource> {
    match source {
        AttachmentSource::Url(url) => {
            (!is_unrenderable_keybase_asset_url(url)).then(|| ImageSource::from(url.clone()))
        }
        AttachmentSource::LocalPath(path) => {
            let normalized = normalize_local_source_path(path);
            (!normalized.is_empty()).then(|| ImageSource::from(PathBuf::from(normalized)))
        }
    }
}

fn attachment_source_to_open_target(source: &AttachmentSource) -> Option<String> {
    match source {
        AttachmentSource::Url(url) => {
            (!is_unrenderable_keybase_asset_url(url)).then(|| url.clone())
        }
        AttachmentSource::LocalPath(_) => None,
    }
}

fn is_unrenderable_keybase_asset_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://127.0.0.1:") || lower.starts_with("http://localhost:") {
        return true;
    }
    let prefix = if lower.starts_with("https://s3.amazonaws.com/") {
        "https://s3.amazonaws.com/"
    } else if lower.starts_with("http://s3.amazonaws.com/") {
        "http://s3.amazonaws.com/"
    } else {
        return false;
    };
    if lower.contains("x-amz-signature=") || lower.contains("awsaccesskeyid=") {
        return false;
    }
    let path = &url[prefix.len()..];
    let Some(first_segment) = path.split('/').next() else {
        return false;
    };
    first_segment.len() == 64 && first_segment.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn normalize_local_source_path(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        return String::new();
    }
    let Some(path) = value.strip_prefix("file://") else {
        return value.to_string();
    };
    let normalized = if let Some(rest) = path.strip_prefix("localhost/") {
        format!("/{rest}")
    } else {
        path.to_string()
    };
    percent_decode(&normalized)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push((hi << 4 | lo) as char);
            index += 3;
            continue;
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn hex(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'a'..=b'f' => Some(ch - b'a' + 10),
        b'A'..=b'F' => Some(ch - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn icon_asset(path: &'static str, size: f32, color: u32) -> AnyElement {
    svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .min_w(px(size))
        .min_h(px(size))
        .flex_shrink_0()
        .text_color(rgb(color))
        .into_any_element()
}

pub fn home_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/home.svg", 18., color)
}

pub fn dm_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/dm.svg", 18., color)
}

pub fn activity_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/activity.svg", 18., color)
}

pub fn history_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/history.svg", 18., color)
}

pub fn search_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/search.svg", 18., color)
}

pub fn sliders_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/sliders.svg", 18., color)
}

pub fn plus_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/plus.svg", 18., color)
}

pub fn emoji_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/emoji.svg", 18., color)
}

pub fn paperclip_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/paperclip.svg", 18., color)
}

pub fn mention_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/mention.svg", 18., color)
}

pub fn format_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/format.svg", 18., color)
}

pub fn link_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/link.svg", 18., color)
}

pub fn copy_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/copy.svg", 18., color)
}

pub fn clipboard_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/clipboard.svg", 18., color)
}

pub fn pencil_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/pencil.svg", 18., color)
}

pub fn trash_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/trash.svg", 18., color)
}

pub fn video_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/video.svg", 18., color)
}

pub fn members_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/members.svg", 18., color)
}

pub fn pin_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/pin.svg", 18., color)
}

pub fn files_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/files.svg", 18., color)
}

pub fn thread_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/thread.svg", 18., color)
}

pub fn call_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/call.svg", 18., color)
}

pub fn more_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/more-horizontal.svg", 16., color)
}

pub fn arrow_left_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/arrow-left.svg", 16., color)
}

pub fn arrow_right_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/arrow-right.svg", 16., color)
}

pub fn chevron_right_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/chevron-right.svg", 14., color)
}

pub fn chevron_down_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/chevron-down.svg", 14., color)
}

pub fn hash_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/hash.svg", 16., color)
}

pub fn at_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/at-sign.svg", 16., color)
}

pub fn close_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/x.svg", 14., color)
}

pub fn crown_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/crown.svg", 14., color)
}

/// Small marker for a Keybase identity relationship, for use beside a person's
/// name: a person-check for someone you follow (distinct from the circled check
/// that marks a verified proof), a warning for a broken identity. Names
/// themselves stay neutral; the marker carries the meaning and explains it.
pub fn identity_marker(
    id: impl Into<gpui_kit::ElementId>,
    affinity: crate::domain::affinity::Affinity,
    display_name: &str,
    include_following: bool,
) -> Option<AnyElement> {
    identity_marker_tinted(id, affinity, display_name, include_following, None)
}

/// [`identity_marker`] drawn in `color` instead of its semantic color, for use
/// on an accent-filled selected row.
pub fn identity_marker_tinted(
    id: impl Into<gpui_kit::ElementId>,
    affinity: crate::domain::affinity::Affinity,
    display_name: &str,
    include_following: bool,
    override_color: Option<u32>,
) -> Option<AnyElement> {
    use crate::domain::affinity::Affinity;
    use gpui_kit::{InteractiveElement, StatefulInteractiveElement};
    let (icon, color, tooltip) = match affinity {
        Affinity::None => return None,
        Affinity::Positive if !include_following => return None,
        Affinity::Positive => (
            "assets/icons/user-check.svg",
            affinity_positive(),
            format!("You follow {display_name}"),
        ),
        Affinity::Broken => (
            "assets/icons/alert-triangle.svg",
            affinity_broken(),
            format!("Identity broken: {display_name}'s proofs changed since you followed them"),
        ),
    };
    Some(
        div()
            .id(id)
            .flex()
            .items_center()
            .flex_shrink_0()
            .child(icon_asset(icon, 13., override_color.unwrap_or(color)))
            .tooltip(tooltip::TextTooltip::single_line(tooltip))
            .into_any_element(),
    )
}

/// Team role crown with an explanatory tooltip ("Team owner", "Team admin").
pub fn role_crown_badge(id: impl Into<gpui_kit::ElementId>, label: &'static str) -> AnyElement {
    use gpui_kit::{InteractiveElement, StatefulInteractiveElement};
    div()
        .id(id)
        .flex()
        .items_center()
        .flex_shrink_0()
        .child(icon_asset("assets/icons/crown.svg", 13., role_crown()))
        .tooltip(tooltip::TextTooltip::single_line(label.to_string()))
        .into_any_element()
}

pub fn send_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/send.svg", 16., color)
}

pub fn play_icon(color: u32) -> AnyElement {
    icon_asset("assets/icons/play.svg", 24., color)
}

pub fn paperclip_icon_small(color: u32) -> AnyElement {
    icon_asset("assets/icons/paperclip.svg", 14., color)
}

fn trim_url_token(token: &str) -> Option<&str> {
    let mut t = token.trim();
    if t.is_empty() {
        return None;
    }
    while let Some(ch) = t.as_bytes().first().copied()
        && matches!(ch, b'(' | b'[' | b'<')
    {
        t = &t[1..];
    }
    while let Some(ch) = t.as_bytes().last().copied()
        && matches!(ch, b'.' | b',' | b')' | b']' | b'>' | b';')
    {
        t = &t[..t.len().saturating_sub(1)];
    }
    (!t.is_empty()).then_some(t)
}

pub fn is_media_link_only_message(fragments: &[MessageFragment], previews: &[LinkPreview]) -> bool {
    if fragments.is_empty() || previews.is_empty() {
        return false;
    }

    let matches_media_preview = |url: &str| {
        previews.iter().any(|p| {
            if !p.is_media {
                return false;
            }
            crate::views::app_window::urls_match(&p.url, url)
                || p.video_url
                    .as_deref()
                    .is_some_and(|v| crate::views::app_window::urls_match(v, url))
        })
    };

    let mut saw_any_url = false;
    for fragment in fragments {
        match fragment {
            MessageFragment::Link { url, .. } => {
                if !matches_media_preview(url) {
                    return false;
                }
                saw_any_url = true;
            }
            MessageFragment::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let mut local_saw = false;
                for raw in trimmed.split_whitespace() {
                    let Some(token) = trim_url_token(raw) else {
                        continue;
                    };
                    if !(token.starts_with("http://") || token.starts_with("https://")) {
                        return false;
                    }
                    if !matches_media_preview(token) {
                        return false;
                    }
                    local_saw = true;
                }
                if !local_saw {
                    return false;
                }
                saw_any_url = true;
            }
            _ => return false,
        }
    }
    saw_any_url
}

pub fn attachment_header_row(attachment: &AttachmentSummary) -> AnyElement {
    let name = attachment.name.clone();
    let size = attachment_size_text(attachment.size_bytes);
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(paperclip_icon_small(text_secondary()))
        .child(
            div()
                .min_w(px(0.))
                .truncate()
                .text_size(typography::caption())
                .text_color(rgb(text_primary()))
                .child(name),
        )
        .when_some(size, |row, size_label| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(size_label),
            )
        })
        .into_any_element()
}

/// Soft fade under the header so messages scroll out of view instead of being
/// cut off at the hairline.
pub fn scroll_edge_fade(surface: Hsla) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(16.))
        .bg(gpui_kit::linear_gradient(
            180.,
            gpui_kit::linear_color_stop(surface, 0.),
            gpui_kit::linear_color_stop(gpui_kit::Hsla { a: 0., ..surface }, 1.),
        ))
        .into_any_element()
}

/// The bottom counterpart of [`scroll_edge_fade`]: content fades into the
/// surface where more of it continues below.
pub fn scroll_edge_fade_bottom(surface: Hsla) -> AnyElement {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(16.))
        .bg(gpui_kit::linear_gradient(
            0.,
            gpui_kit::linear_color_stop(surface, 0.),
            gpui_kit::linear_color_stop(gpui_kit::Hsla { a: 0., ..surface }, 1.),
        ))
        .into_any_element()
}

/// A file attachment that has no inline preview: a compact 32px row with a
/// document icon, the file name and its size.
pub fn attachment_file_row(attachment: &AttachmentSummary) -> AnyElement {
    let size = attachment_size_text(attachment.size_bytes);
    div()
        .min_h(typography::font_px(32.))
        .max_w(px(360.))
        .pl(px(8.))
        .pr(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(field_border()))
        .bg(rgb(field_bg()))
        .flex()
        .items_center()
        .gap_2()
        .child(icon_asset("assets/icons/files.svg", 16., text_secondary()))
        .child(
            div()
                .min_w(px(0.))
                .text_size(typography::label())
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(rgb(text_primary()))
                .truncate()
                .child(attachment.name.clone()),
        )
        .when_some(size, |row, size_label| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(size_label),
            )
        })
        .into_any_element()
}

/// A file attachment still uploading: the file row with "Uploading · 1.7 / 4.0
/// MB · 42%" and a 3px progress track along its bottom edge. There is no
/// cancel: Keybase's attachment upload has no cancel call to stop it.
pub fn attachment_upload_row(
    attachment: &AttachmentSummary,
    progress: crate::state::state::UploadProgress,
) -> AnyElement {
    let total = if progress.total_bytes > 0 {
        progress.total_bytes
    } else {
        attachment.size_bytes
    };
    let status = upload_status_label(progress.sent_bytes, total);
    let fraction = progress.fraction();
    div()
        .relative()
        .h(px(36.))
        .w(px(360.))
        .max_w_full()
        .pl(px(8.))
        .pr(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(field_border()))
        .bg(rgb(field_bg()))
        .overflow_hidden()
        .flex()
        .items_center()
        .gap_2()
        .child(icon_asset("assets/icons/files.svg", 16., text_secondary()))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(typography::label())
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(rgb(text_primary()))
                .truncate()
                .child(attachment.name.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(typography::font_px(12.))
                .text_color(rgb(text_secondary()))
                .child(status),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(3.))
                .bg(tint(text_primary(), 0.08))
                .child(
                    div()
                        .h_full()
                        .w(gpui_kit::relative(fraction))
                        .bg(rgb(accent_strong())),
                ),
        )
        .into_any_element()
}

pub fn format_duration_ms(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes >= 60 {
        let hours = minutes / 60;
        let mins = minutes % 60;
        format!("{hours}:{mins:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Fill behind query matches in search and switcher snippets. Deliberately
/// stronger than the self-mention yellow, and snippets draw mentions without a
/// fill, so a mention never reads as a match.
pub fn search_match_bg() -> u32 {
    if is_dark_theme() { 0x5c4a12 } else { 0xffe58f }
}

/// Text on [`search_match_bg`].
pub fn search_match_fg() -> u32 {
    if is_dark_theme() {
        0xffd666
    } else {
        text_primary()
    }
}

/// Title row of a main-pane page such as Search: 56px with a hairline, matching
/// the conversation header, title aligned with the 24px content inset. `leading`
/// (a back button) sits in front of the title and pulls the row 8px left so the
/// button's icon, not its hover box, lines up with the content.
pub fn page_header(
    title: &str,
    leading: Option<AnyElement>,
    trailing: Option<AnyElement>,
) -> AnyElement {
    let has_leading = leading.is_some();
    div()
        .h(px(56.))
        .flex_shrink_0()
        .pl(px(if has_leading { 16. } else { 24. }))
        .pr(px(24.))
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .border_b_1()
        .border_color(shell_border())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .when_some(leading, |row, leading| row.child(leading))
                .child(
                    div()
                        .text_size(typography::font_px(17.))
                        .line_height(typography::font_px(22.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(text_primary()))
                        .child(title.to_string()),
                ),
        )
        .when_some(trailing, |row, trailing| row.child(trailing))
        .into_any_element()
}

/// 28px "Back" button for utility pages, with its ⌘[ shortcut in the tooltip.
pub fn back_button(id: impl Into<gpui_kit::ElementId>) -> gpui_kit::Stateful<gpui_kit::Div> {
    use gpui_kit::{InteractiveElement, StatefulInteractiveElement};
    div()
        .id(id)
        .size(px(28.))
        .flex_shrink_0()
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .cursor(gpui_kit::CursorStyle::PointingHand)
        .hover(|style| style.bg(tint(text_primary(), 0.07)))
        .tooltip(tooltip::TextTooltip::with_shortcut(
            "Back".to_string(),
            tooltip::shortcut_label("⌘["),
        ))
        .child(icon_asset(
            "assets/icons/chevron-left.svg",
            16.,
            text_secondary(),
        ))
}

/// Group label above a list section ("Owners & admins", "Unread"), the same
/// 12px semibold secondary style as the sidebar's "Recent".
pub fn section_label(text: impl Into<SharedString>) -> gpui_kit::Div {
    div()
        .text_size(typography::font_px(12.))
        .line_height(typography::font_px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(text_secondary()))
        .child(text.into())
}

/// A key drawn as a keycap, for keyboard hints: `↵`, `esc`, `⌘K`.
pub fn keycap(label: impl Into<SharedString>) -> AnyElement {
    div()
        .min_h(typography::font_px(18.))
        .min_w(px(18.))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(field_border()))
        .bg(rgb(field_bg()))
        .text_size(typography::font_px(11.))
        .line_height(typography::font_px(14.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(text_secondary()))
        .child(label.into())
        .into_any_element()
}

/// Centered placeholder for a surface with nothing to show: an icon in a soft
/// circle, a one-line title and an optional explanation.
pub fn empty_state(
    icon: &'static str,
    title: impl Into<SharedString>,
    body: Option<String>,
) -> gpui_kit::Div {
    div()
        .w_full()
        .py(px(32.))
        .px(px(24.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .child(
            // A neutral wash rather than a surface token, so the circle shows on
            // whichever pane hosts it.
            div()
                .size(px(40.))
                .mb(px(8.))
                .rounded_full()
                .bg(tint(
                    text_primary(),
                    if is_dark_theme() { 0.08 } else { 0.06 },
                ))
                .flex()
                .items_center()
                .justify_center()
                .child(icon_asset(icon, 18., text_secondary())),
        )
        .child(
            div()
                .text_size(typography::font_px(14.))
                .line_height(typography::font_px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(text_primary()))
                .text_center()
                .child(title.into()),
        )
        .when_some(body, |state, body| {
            state.child(
                div()
                    .max_w(px(300.))
                    .text_size(typography::label())
                    .line_height(typography::font_px(18.))
                    .text_color(rgb(text_secondary()))
                    .text_center()
                    .child(body),
            )
        })
}

/// Bordered single-line field with a leading magnifier, for search and filter
/// inputs. Same fill, border and focus treatment as the composer.
pub fn search_field_shell(
    id: &'static str,
    input: AnyElement,
    focused: bool,
    height: f32,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    use gpui_kit::InteractiveElement;
    div()
        .id(id)
        .min_h(typography::font_px(height))
        .w_full()
        .pl(px(10.))
        .pr(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(if focused { accent() } else { field_border() }))
        .when(focused, |field| field.shadow(vec![focus_ring()]))
        .bg(rgb(field_bg()))
        .cursor(gpui_kit::CursorStyle::IBeam)
        .text_size(typography::label())
        .line_height(typography::font_px(18.))
        .text_color(rgb(text_primary()))
        .child(icon_asset("assets/icons/search.svg", 14., text_secondary()))
        .child(div().flex_1().min_w(px(0.)).child(input))
}

/// Accent-filled button for the one primary action of a surface ("Message").
pub fn primary_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    use gpui_kit::InteractiveElement;
    div()
        .id(id)
        .min_h(typography::font_px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(rgb(accent_strong()))
        .hover(|style| style.bg(rgb(accent())))
        .cursor(gpui_kit::CursorStyle::PointingHand)
        .text_size(typography::label())
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(0xffffff))
        .child(label.into())
}

/// Bordered neutral button for secondary actions ("Unfollow", "Close").
pub fn secondary_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    use gpui_kit::InteractiveElement;
    div()
        .id(id)
        .min_h(typography::font_px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(field_border()))
        .bg(rgb(field_bg()))
        .hover(|style| style.bg(tint(text_primary(), 0.07)))
        .cursor(gpui_kit::CursorStyle::PointingHand)
        .text_size(typography::label())
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(text_primary()))
        .child(label.into())
}

pub fn composer_tool(icon: AnyElement, color: u32) -> AnyElement {
    div()
        .w(px(28.))
        .min_h(typography::font_px(28.))
        .rounded_md()
        .bg(subtle_surface())
        .border_1()
        .border_color(shell_border())
        .flex()
        .items_center()
        .justify_center()
        .child(div().text_color(rgb(color)).child(icon))
        .into_any_element()
}

#[cfg(test)]
mod upload_label_tests {
    use super::upload_status_label;

    #[test]
    fn names_the_direction_both_sizes_and_the_share_sent() {
        let mb = 1024 * 1024;
        assert_eq!(
            upload_status_label(17 * mb / 10, 4 * mb),
            "Uploading · 1.7 / 4.0 MB · 42%"
        );
        assert_eq!(
            upload_status_label(0, 4 * mb),
            "Uploading · 0.0 / 4.0 MB · 0%"
        );
        assert_eq!(
            upload_status_label(5 * mb, 4 * mb),
            "Uploading · 4.0 / 4.0 MB · 100%"
        );
        assert_eq!(
            upload_status_label(256 * 1024, 20 * mb),
            "Uploading · 0 / 20 MB · 1%"
        );
        assert_eq!(
            upload_status_label(300, 900),
            "Uploading · 300 / 900 B · 33%"
        );
    }

    #[test]
    fn unknown_total_still_says_uploading() {
        assert_eq!(upload_status_label(10, 0), "Uploading…");
    }
}

#[cfg(test)]
mod highlight_color_tests {
    use super::*;
    use crate::domain::{affinity::Affinity, ids::UserId};
    use std::collections::HashMap;

    fn contrast(foreground: u32, background: u32) -> f64 {
        let luminance = |color: u32| {
            let channel = |shift: u32| {
                let value = ((color >> shift) & 255u32) as f64 / 255.;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
        };
        let a = luminance(foreground);
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn inline_highlights_are_readable_and_self_mentions_are_distinct_in_both_themes() {
        for theme in [ThemeVariant::Light, ThemeVariant::Dark] {
            with_theme(theme, || {
                let me = UserId::new("AliceExample");
                let mentioned_me = UserId::new("aliceexample");
                let other = UserId::new("someone_else");
                let following = HashMap::from([(mentioned_me.clone(), Affinity::Positive)]);
                let own = mention_colors_for_user(&following, Some(&me), &mentioned_me);
                let ordinary = mention_colors_for_user(&following, Some(&me), &other);
                assert_ne!(own, ordinary);
                assert_eq!(own, (mention_you(), mention_you_soft()));
                for (foreground, background) in [
                    own,
                    ordinary,
                    (inline_code_foreground(), inline_code_background()),
                ] {
                    assert!(
                        contrast(foreground, background) >= 4.5,
                        "insufficient contrast: {foreground:06x} on {background:06x}"
                    );
                }
                assert_ne!(inline_code_background(), mention_soft());
            });
        }
    }

    #[test]
    fn small_text_on_every_surface_meets_aa_in_both_themes() {
        for theme in [ThemeVariant::Light, ThemeVariant::Dark] {
            with_theme(theme, || {
                let surfaces = [
                    main_bg(),
                    panel_bg(),
                    panel_alt_bg(),
                    sidebar_bg(),
                    field_bg(),
                ];
                for background in surfaces {
                    for foreground in [text_primary(), text_secondary(), link()] {
                        assert!(
                            contrast(foreground, background) >= 4.5,
                            "{theme:?}: {foreground:06x} on {background:06x}"
                        );
                    }
                }
                for (foreground, background) in [
                    // Selected sidebar row, mention badges and the send button.
                    (sidebar_selected_fg(), sidebar_selected_bg()),
                    (0xffffff, accent_strong()),
                    (affinity_positive(), affinity_positive_soft()),
                    (affinity_broken(), affinity_broken_soft()),
                    (affinity_positive(), main_bg()),
                    (affinity_broken(), main_bg()),
                    (mention(), mention_soft()),
                    (new_messages(), main_bg()),
                    (search_match_fg(), search_match_bg()),
                    (text_secondary(), field_bg()),
                ] {
                    assert!(
                        contrast(foreground, background) >= 4.5,
                        "{theme:?}: {foreground:06x} on {background:06x}"
                    );
                }
            });
        }
    }

    #[test]
    fn window_surfaces_are_opaque_in_both_themes() {
        for theme in [ThemeVariant::Light, ThemeVariant::Dark] {
            with_theme(theme, || {
                for surface in [
                    sidebar_surface(),
                    content_surface(),
                    glass_surface(),
                    glass_surface_strong(),
                    glass_surface_dark(),
                    modal_surface(),
                    right_pane_surface(),
                ] {
                    assert_eq!(surface.a, 1.0, "{theme:?}: {surface:?}");
                }
            });
        }
    }

    #[test]
    fn inactive_window_selection_stays_readable() {
        for theme in [ThemeVariant::Light, ThemeVariant::Dark] {
            with_theme(theme, || {
                with_window_active(false, || {
                    assert_eq!(sidebar_selected_fg(), text_primary());
                    assert!(contrast(sidebar_selected_fg(), sidebar_selected_bg()) >= 4.5);
                    let secondary = sidebar_selected_secondary().to_rgb();
                    let secondary = ((secondary.r * 255.).round() as u32) << 16
                        | ((secondary.g * 255.).round() as u32) << 8
                        | (secondary.b * 255.).round() as u32;
                    assert!(contrast(secondary, sidebar_selected_bg()) >= 4.5);
                });
                assert_ne!(
                    sidebar_selected_bg(),
                    with_window_active(false, sidebar_selected_bg)
                );
            });
        }
    }

    #[test]
    fn team_marks_carry_white_initials_and_stand_out_from_the_sidebar() {
        for name in ["lighthouse", "bookclub", "keybase", "a", "zbase.design"] {
            let (fill, initial) = avatar::team_colors(name);
            assert!(contrast(initial, fill) >= 4.5, "{name}: {fill:06x}");
            for sidebar in [LIGHT_PALETTE.sidebar_bg, DARK_PALETTE.sidebar_bg] {
                assert!(contrast(fill, sidebar) >= 2.0, "{name}: {fill:06x}");
            }
        }
        assert_eq!(
            avatar::team_colors("Lighthouse"),
            avatar::team_colors("lighthouse")
        );
    }
}
