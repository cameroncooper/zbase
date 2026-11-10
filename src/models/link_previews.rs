//! Merges fetched OpenGraph and GitHub PR previews into the models' messages.
//! Store syncs replace messages with copies that lack them, so each sync reapplies them.
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use super::{AppModels, timeline_model::TimelineRow};
use crate::domain::{
    github_pr::GithubPrRef,
    message::{LinkPreview, MessageFragment, MessageRecord},
};
use crate::services::{github_pr_service::GithubPrService, og_service::OgService};

fn row_messages_mut(rows: &mut [TimelineRow]) -> impl Iterator<Item = &mut MessageRecord> {
    rows.iter_mut().filter_map(|row| {
        if let TimelineRow::Message(message_row) = row {
            Some(&mut message_row.message)
        } else {
            None
        }
    })
}

/// Applies the cached OpenGraph and GitHub PR previews to the timeline and thread.
pub fn apply_og_previews(
    models: &mut AppModels,
    og_service: &OgService,
    github_pr_service: &GithubPrService,
) {
    apply_og_previews_to_messages(
        og_service,
        row_messages_mut(&mut models.timeline.rows).collect(),
    );
    apply_og_previews_to_messages(og_service, models.thread_pane.replies.iter_mut().collect());
    apply_github_pr_previews(models, github_pr_service, Instant::now());
}

/// Applies the cached GitHub PR previews to the timeline and thread.
/// Returns whether any PR preview changed.
pub fn apply_github_pr_previews(
    models: &mut AppModels,
    github_pr_service: &GithubPrService,
    now: Instant,
) -> bool {
    let messages = row_messages_mut(&mut models.timeline.rows)
        .chain(models.thread_pane.replies.iter_mut())
        .collect();
    apply_github_pr_previews_to_messages(github_pr_service, now, messages)
}

/// Copies the store's link previews onto the timeline rows without rebuilding them.
pub fn sync_timeline_row_link_previews(
    models: &mut AppModels,
    source_messages: &[MessageRecord],
    og_service: &OgService,
    github_pr_service: &GithubPrService,
) {
    let previews_by_message_id = source_messages
        .iter()
        .map(|message| (&message.id, &message.link_previews))
        .collect::<HashMap<_, _>>();
    for message in row_messages_mut(&mut models.timeline.rows) {
        if let Some(next_previews) = previews_by_message_id.get(&message.id) {
            if message.link_previews != **next_previews {
                message.link_previews = (*next_previews).clone();
            }
        } else if !message.link_previews.is_empty() {
            message.link_previews.clear();
        }
    }
    apply_og_previews(models, og_service, github_pr_service);
}

fn apply_og_previews_to_messages(og_service: &OgService, messages: Vec<&mut MessageRecord>) {
    for message in messages {
        let urls = extract_link_urls_from_fragments(&message.fragments);
        if urls.is_empty() {
            continue;
        }
        for url in &urls {
            let Some(Some(og)) = og_service.lookup(url) else {
                continue;
            };
            if let Some(existing) = message
                .link_previews
                .iter_mut()
                .find(|p| urls_match(&p.url, url))
            {
                if existing.title.is_none() {
                    existing.title = og.title.clone();
                }
                if existing.site.is_none() {
                    existing.site = og.site.clone();
                }
                if existing.description.is_none() {
                    existing.description = og.description.clone();
                }
                if existing.thumbnail_asset.is_none() {
                    existing.thumbnail_asset = og.thumbnail_asset.clone();
                }
            } else {
                message.link_previews.push(og.clone());
            }
        }
    }
}

/// Store syncs replace thread replies with copies that lack fetched previews.
pub fn apply_link_previews_to_thread_replies(
    og_service: &OgService,
    github_pr_service: &GithubPrService,
    replies: &mut [MessageRecord],
) {
    apply_og_previews_to_messages(og_service, replies.iter_mut().collect());
    apply_github_pr_previews_to_messages(
        github_pr_service,
        Instant::now(),
        replies.iter_mut().collect(),
    );
}

/// Returns whether any preview changed.
fn apply_github_pr_previews_to_messages(
    service: &GithubPrService,
    now: Instant,
    messages: Vec<&mut MessageRecord>,
) -> bool {
    let mut changed = false;
    for message in messages {
        let urls = extract_link_urls_from_fragments(&message.fragments);
        let mut seen = HashSet::new();
        for url in &urls {
            let Some(pr) = GithubPrRef::parse(url) else {
                continue;
            };
            if !seen.insert(pr.clone()) {
                continue;
            }
            let existing = message
                .link_previews
                .iter()
                .position(|preview| GithubPrRef::parse(&preview.url).as_ref() == Some(&pr));
            match (existing, service.preview(&pr, now)) {
                (Some(index), Some(pr_preview)) => {
                    let preview = &mut message.link_previews[index];
                    if preview.github_pr.as_ref() != Some(&pr_preview) {
                        preview.github_pr = Some(pr_preview);
                        changed = true;
                    }
                }
                (None, Some(pr_preview)) => {
                    message.link_previews.push(LinkPreview {
                        url: url.clone(),
                        site: Some("github.com".to_string()),
                        github_pr: Some(pr_preview),
                        ..LinkPreview::default()
                    });
                    changed = true;
                }
                (Some(index), None) => {
                    let preview = &mut message.link_previews[index];
                    let is_empty_placeholder = preview.title.is_none()
                        && preview.description.is_none()
                        && preview.thumbnail_asset.is_none()
                        && !preview.is_media;
                    if is_empty_placeholder {
                        message.link_previews.remove(index);
                        changed = true;
                    } else if preview.github_pr.take().is_some() {
                        changed = true;
                    }
                }
                (None, None) => {}
            }
        }
    }
    changed
}

pub fn urls_match(a: &str, b: &str) -> bool {
    let normalize = |url: &str| {
        let trimmed = url.trim();
        let base = trimmed.split('#').next().unwrap_or(trimmed);
        base.to_ascii_lowercase().trim_end_matches('/').to_string()
    };
    normalize(a) == normalize(b)
}

pub fn extract_link_urls_from_fragments(fragments: &[MessageFragment]) -> Vec<String> {
    let mut urls = Vec::new();
    for fragment in fragments {
        match fragment {
            MessageFragment::Link { url, .. } => {
                let lower = url.to_ascii_lowercase();
                if lower.starts_with("http://")
                    || lower.starts_with("https://")
                    || (!lower.starts_with("zbase-") && lower.contains('.'))
                {
                    urls.push(url.clone());
                }
            }
            MessageFragment::Text(text) => {
                extract_urls_from_text(text, &mut urls);
            }
            _ => {}
        }
    }
    urls
}

fn extract_urls_from_text(text: &str, out: &mut Vec<String>) {
    let mut search_from = 0;
    while search_from < text.len() {
        let remaining = &text[search_from..];
        let prefix_pos = remaining
            .find("https://")
            .or_else(|| remaining.find("http://"));
        let Some(pos) = prefix_pos else {
            break;
        };
        let url_start = search_from + pos;
        let mut url_end = url_start;
        for ch in text[url_start..].chars() {
            if ch.is_whitespace() {
                break;
            }
            url_end += ch.len_utf8();
        }
        while url_end > url_start
            && matches!(
                text.as_bytes()[url_end - 1],
                b'.' | b',' | b')' | b']' | b';'
            )
        {
            url_end -= 1;
        }
        if url_end > url_start + 8 {
            out.push(text[url_start..url_end].to_string());
        }
        search_from = url_end;
    }
}
