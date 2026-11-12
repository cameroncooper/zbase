//! The status strip shown in place of a link preview for GitHub pull requests.

use crate::domain::github_pr::{
    GithubCheckCounts, GithubPrPreview, GithubPrState, GithubPrStatus, GithubReviewState,
};
use crate::views::typography;
use crate::views::{
    accent, affinity_broken, affinity_positive, danger, danger_soft, icon_asset, is_dark_theme,
    link, mono_font_family, panel_alt_bg, success, success_soft, text_primary, text_secondary,
    warning, warning_soft,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{Div, IntoElement, ParentElement, Styled, div, px, rgb};

const PR_ICON: &str = "assets/icons/git-pull-request.svg";
const DRAFT_ICON: &str = "assets/icons/git-pull-request-draft.svg";
const CLOSED_ICON: &str = "assets/icons/git-pull-request-closed.svg";
const MERGED_ICON: &str = "assets/icons/git-merge.svg";
const CHECK_ICON: &str = "assets/icons/check.svg";
const X_ICON: &str = "assets/icons/x.svg";
const RUNNING_ICON: &str = "assets/icons/loader.svg";
const EYE_ICON: &str = "assets/icons/eye.svg";
const CHANGES_ICON: &str = "assets/icons/file-diff.svg";
const COMMENT_ICON: &str = "assets/icons/message-square.svg";
const MAX_WIDTH: f32 = 520.;

#[derive(Clone, Copy)]
enum Tone {
    Good,
    Bad,
    Waiting,
    Neutral,
    Merged,
}

impl Tone {
    /// (foreground, background)
    fn colors(self) -> (u32, u32) {
        match self {
            Tone::Good => (affinity_positive(), success_soft()),
            Tone::Bad => (affinity_broken(), danger_soft()),
            Tone::Waiting => (warning(), warning_soft()),
            Tone::Neutral => (text_secondary(), panel_alt_bg()),
            Tone::Merged => merged_colors(),
        }
    }
}

fn merged_colors() -> (u32, u32) {
    if is_dark_theme() {
        (0xa371f7, 0x2c2240)
    } else {
        (0x8250df, 0xf3eefe)
    }
}

/// Callers add the element id, click and hover handlers.
pub fn render_github_pr_preview(preview: &GithubPrPreview) -> Div {
    let is_comment = preview.pr.comment.is_some();
    let status = preview.status.as_ref();
    let state = status.map(|status| status.state);
    let (bar_color, icon, icon_color) = match state {
        None if preview.needs_auth => (text_secondary(), PR_ICON, text_secondary()),
        None => (accent(), PR_ICON, text_secondary()),
        Some(GithubPrState::Open) => (success(), PR_ICON, success()),
        Some(GithubPrState::Draft) => (text_secondary(), DRAFT_ICON, text_secondary()),
        Some(GithubPrState::Merged) => (merged_colors().0, MERGED_ICON, merged_colors().0),
        Some(GithubPrState::Closed) => (danger(), CLOSED_ICON, danger()),
    };
    let state_pill = match state {
        Some(GithubPrState::Draft) => Some(("Draft", Tone::Neutral)),
        Some(GithubPrState::Merged) => Some(("Merged", Tone::Merged)),
        Some(GithubPrState::Closed) => Some(("Closed", Tone::Bad)),
        _ => None,
    };

    let site_row = div()
        .flex()
        .items_center()
        .gap_1p5()
        .text_size(typography::caption())
        .text_color(rgb(text_secondary()))
        .child(icon_asset(icon, 12., icon_color))
        .child(format!(
            "{} #{}",
            preview.pr.repo_label(),
            preview.pr.number
        ))
        .when_some(state_pill.filter(|_| !is_comment), |row, (label, tone)| {
            row.child(pill(label, tone))
        });

    let title_row = div()
        .text_size(typography::body())
        .text_color(rgb(text_primary()))
        .map(|row| match status {
            Some(status) => row.child(status.title.clone()),
            None if preview.needs_auth => row.child("Pull request"),
            None => row.py_0p5().child(skeleton(240., 12.)),
        });

    let chips_row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1p5()
        .mt_0p5()
        .map(|row| match status {
            Some(status) if status.details_loaded => status_chips(row, status, preview.refreshing),
            None if preview.needs_auth => row.child(
                div()
                    .text_size(typography::caption())
                    .text_color(rgb(link()))
                    .child("Run `gh auth login` to see checks and reviews"),
            ),
            _ => row
                .child(skeleton(110., 20.))
                .child(skeleton(80., 20.))
                .child(skeleton(90., 20.)),
        });

    div()
        .max_w(px(MAX_WIDTH))
        .border_l_2()
        .border_color(rgb(bar_color))
        .pl_2()
        .py_0p5()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(site_row)
        .child(title_row)
        .when(!is_comment, |card| card.child(chips_row))
        .when(
            is_comment && status.and_then(|status| status.comment.as_ref()).is_none(),
            |card| {
                card.child(div().text_xs().text_color(rgb(text_secondary())).child(
                    if preview.needs_auth {
                        "Run `gh auth login` to preview this comment"
                    } else if status.is_some() {
                        "Comment unavailable"
                    } else {
                        "Loading comment…"
                    },
                ))
            },
        )
        .when_some(
            status.and_then(|status| status.comment.as_ref()),
            |card, comment| {
                card.child(
                    div()
                        .mt_1()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(text_secondary()))
                                .child(format!("{} commented", comment.author)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(text_primary()))
                                .child(comment.body.clone()),
                        ),
                )
            },
        )
}

fn status_chips(row: Div, status: &GithubPrStatus, refreshing: bool) -> Div {
    let row = match status.state {
        GithubPrState::Merged => row.child(chip(
            MERGED_ICON,
            match status.merged_by.as_deref() {
                Some(login) => format!("Merged by {login}"),
                None => "Merged".to_string(),
            },
            Tone::Merged,
        )),
        GithubPrState::Closed => {
            row.child(chip(CLOSED_ICON, "Closed without merging", Tone::Neutral))
        }
        GithubPrState::Open | GithubPrState::Draft => row
            .when_some(checks_chip(&status.checks), ParentElement::child)
            .child(review_chip(status.review))
            .child(comments_chip(status)),
    };
    row.child(diff_stat(status.additions, status.deletions))
        .when(refreshing, |row| {
            row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(typography::caption())
                    .text_color(rgb(text_secondary()))
                    .child(icon_asset(RUNNING_ICON, 11., text_secondary()))
                    .child("updating…"),
            )
        })
}

fn checks_chip(checks: &GithubCheckCounts) -> Option<Div> {
    let total = checks.total();
    if total == 0 {
        return None;
    }
    Some(if checks.failed > 0 {
        chip(
            X_ICON,
            format!("{} of {total} checks failed", checks.failed),
            Tone::Bad,
        )
    } else if checks.running > 0 {
        let label = if checks.passed > 0 {
            format!("{} running · {} passed", checks.running, checks.passed)
        } else {
            plural(checks.running, "check running", "checks running")
        };
        chip(RUNNING_ICON, label, Tone::Waiting)
    } else {
        chip(
            CHECK_ICON,
            plural(checks.passed, "check passed", "checks passed"),
            Tone::Good,
        )
    })
}

fn review_chip(review: GithubReviewState) -> Div {
    match review {
        GithubReviewState::Approved => chip(CHECK_ICON, "Approved", Tone::Good),
        GithubReviewState::ChangesRequested => chip(CHANGES_ICON, "Changes requested", Tone::Bad),
        GithubReviewState::ReviewRequired => chip(EYE_ICON, "Review required", Tone::Neutral),
        GithubReviewState::NoReviews => chip(EYE_ICON, "No reviews", Tone::Neutral),
    }
}

fn comments_chip(status: &GithubPrStatus) -> Div {
    let unresolved = status.unresolved_threads;
    let more = if status.threads_complete { "" } else { "+" };
    if unresolved > 0 {
        chip(
            COMMENT_ICON,
            format!("{unresolved}{more} unresolved"),
            Tone::Waiting,
        )
    } else if !status.threads_complete {
        chip(
            COMMENT_ICON,
            format!("{} threads", status.total_threads),
            Tone::Neutral,
        )
    } else if status.total_threads > 0 {
        chip(COMMENT_ICON, "All resolved", Tone::Neutral)
    } else {
        chip(COMMENT_ICON, "No comments", Tone::Neutral)
    }
}

fn diff_stat(additions: u32, deletions: u32) -> Div {
    div()
        .flex()
        .gap_1()
        .ml_0p5()
        .text_size(typography::caption())
        .font_family(mono_font_family())
        .child(
            div()
                .text_color(rgb(affinity_positive()))
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .text_color(rgb(affinity_broken()))
                .child(format!("−{deletions}")),
        )
}

fn chip(icon: &'static str, label: impl Into<gpui_kit::SharedString>, tone: Tone) -> Div {
    let (fg, bg) = tone.colors();
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(px(5.))
        .bg(rgb(bg))
        .text_size(typography::caption())
        .text_color(rgb(fg))
        .child(icon_asset(icon, 11., fg))
        .child(label.into())
}

fn pill(label: &'static str, tone: Tone) -> Div {
    let (fg, bg) = tone.colors();
    div()
        .px_1p5()
        .rounded_full()
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .child(label)
}

fn skeleton(width: f32, height: f32) -> impl IntoElement {
    div()
        .w(px(width))
        .h(px(height))
        .rounded(px(5.))
        .bg(rgb(panel_alt_bg()))
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}
