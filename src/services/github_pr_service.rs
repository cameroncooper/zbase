//! Live status for GitHub pull request links, fetched per viewer with their
//! own token. Nothing here is persisted: status changes too often and depends
//! on who is looking.

use crate::domain::github_pr::{
    GithubCheckCounts, GithubComment, GithubCommentRef, GithubPrPreview, GithubPrRef,
    GithubPrState, GithubPrStatus, GithubReviewState,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Only the highest-priority PR links keep refreshing; others are fetched once.
pub const MAX_REFRESHED_PRS: usize = 25;
const MAX_BATCH: usize = 4;
const RUNNING_INTERVAL: Duration = Duration::from_secs(30);
const OPEN_INTERVAL: Duration = Duration::from_secs(180);
const DRAFT_INTERVAL: Duration = Duration::from_secs(300);
/// On conversation switch or window focus, refresh anything older than this.
const ATTENTION_MAX_AGE: Duration = Duration::from_secs(30);
const TOKEN_RETRY: Duration = Duration::from_secs(300);
/// A refresh of status older than this shows "updating…".
const STALE_AFTER: Duration = Duration::from_secs(120);
const BASE_BACKOFF: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(600);
const DEFAULT_RATE_LIMIT_WAIT: Duration = Duration::from_secs(60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// `gh auth token` can hang on a keychain or secret-service prompt.
const GH_TIMEOUT: Duration = Duration::from_secs(5);
const GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// A GitHub token. `Debug` never prints the secret.
#[derive(Clone)]
struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

#[derive(Debug)]
pub struct GithubPrFetchResult {
    refs: Vec<GithubPrRef>,
    outcome: FetchOutcome,
}

#[derive(Debug)]
enum FetchOutcome {
    NoToken,
    Unauthorized,
    RateLimited {
        retry_after: Duration,
    },
    Failed,
    Fetched {
        token: Token,
        statuses: Vec<(GithubPrRef, PrFetch)>,
    },
}

#[derive(Debug)]
enum PrFetch {
    Found(GithubPrStatus),
    /// The PR does not exist or this token cannot see it.
    Inaccessible,
    /// GitHub failed on this PR only; try again later.
    Transient,
}

#[derive(Default)]
struct Entry {
    status: Option<GithubPrStatus>,
    fetched_at: Option<Instant>,
    discovered_at: Option<Instant>,
    in_flight: bool,
    no_access: bool,
    failed: bool,
}

pub struct GithubPrService {
    entries: HashMap<GithubPrRef, Entry>,
    token: Option<Token>,
    token_retry_at: Option<Instant>,
    backoff_until: Option<Instant>,
    failures: u32,
    active_batches: usize,
    attention: bool,
}

impl Default for GithubPrService {
    fn default() -> Self {
        Self::new()
    }
}

impl GithubPrService {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            token: None,
            token_retry_at: None,
            backoff_until: None,
            failures: 0,
            active_batches: 0,
            attention: false,
        }
    }

    /// The user switched conversations or refocused the window.
    pub fn note_attention(&mut self) {
        self.attention = true;
    }

    /// Whether this URL gets a PR card, so the generic Open Graph fetch can be skipped.
    pub fn handles_url(&self, url: &str, now: Instant) -> bool {
        GithubPrRef::parse(url).is_some_and(|pr| self.preview(&pr, now).is_some())
    }

    /// Token lookup failed and is waiting to be retried.
    fn token_missing(&self) -> bool {
        self.token.is_none() && self.token_retry_at.is_some()
    }

    /// `None` means fall back to the ordinary link preview.
    pub fn preview(&self, pr: &GithubPrRef, now: Instant) -> Option<GithubPrPreview> {
        let Some(entry) = self.entries.get(pr) else {
            return Some(GithubPrPreview {
                pr: pr.clone(),
                status: None,
                needs_auth: self.token_missing(),
                refreshing: false,
            });
        };
        if entry.no_access || (entry.failed && entry.status.is_none()) {
            return None;
        }
        let stale = entry
            .fetched_at
            .is_some_and(|at| now.saturating_duration_since(at) >= STALE_AFTER);
        Some(GithubPrPreview {
            pr: pr.clone(),
            status: entry.status.clone(),
            needs_auth: entry.status.is_none() && self.token_missing(),
            refreshing: entry.in_flight && entry.status.is_some() && stale,
        })
    }

    /// `refs` are PRs in the open conversation, ordered by viewport priority.
    /// Returns whether any preview may have changed.
    pub fn poll(
        &mut self,
        refs: &[GithubPrRef],
        window_focused: bool,
        now: Instant,
        sender: &Sender<GithubPrFetchResult>,
    ) -> bool {
        let mut changed = false;
        for pr in refs {
            if !self.entries.contains_key(pr) {
                self.entries.insert(
                    pr.clone(),
                    Entry {
                        discovered_at: Some(now),
                        ..Entry::default()
                    },
                );
                changed = true;
            }
        }
        if self.active_batches >= 2
            || (self.active_batches > 0 && self.token.is_none())
            || self.backoff_until.is_some_and(|until| now < until)
        {
            return changed;
        }
        if self.token.is_none() && self.token_retry_at.is_some_and(|at| now < at) {
            return changed;
        }
        let attention = std::mem::take(&mut self.attention);
        let due = self.select_due(refs, window_focused, attention, now);
        if due.is_empty() {
            return changed;
        }
        for pr in &due {
            if let Some(entry) = self.entries.get_mut(pr) {
                entry.in_flight = true;
            }
        }
        let include_details = self.entries[&due[0]].status.is_some();
        self.active_batches += 1;
        let token = self.token.clone();
        let sender = sender.clone();
        std::thread::spawn(move || {
            let outcome = fetch_batch(token, &due, include_details);
            let _ = sender.send(GithubPrFetchResult { refs: due, outcome });
        });
        true
    }

    fn select_due(
        &self,
        refs: &[GithubPrRef],
        window_focused: bool,
        attention: bool,
        now: Instant,
    ) -> Vec<GithubPrRef> {
        let mut candidates: Vec<_> = refs
            .iter()
            .enumerate()
            .filter(|(index, pr)| {
                let Some(entry) = self.entries.get(*pr) else {
                    return false;
                };
                if entry.no_access || entry.in_flight {
                    return false;
                }
                match (&entry.status, entry.fetched_at) {
                    (Some(status), Some(fetched_at)) => {
                        // Reserve the second slot for initial loads so an older
                        // conversation or background refresh cannot block them.
                        self.active_batches == 0
                            && (!status.details_loaded
                                || (window_focused
                                    && *index < MAX_REFRESHED_PRS
                                    && refresh_due(
                                        status,
                                        now.saturating_duration_since(fetched_at),
                                        attention,
                                    )))
                    }
                    _ => true,
                }
            })
            .map(|(_, pr)| pr.clone())
            .collect();
        // Stable ordering keeps viewport priority within each group. Initial
        // loads must not sit behind refreshes of already usable cards.
        candidates.sort_by_key(|pr| self.entries[pr].status.is_some());
        // Keep summary and detail requests separate. A lightweight title query
        // should never inherit the cost of checks/reviews from another card.
        if let Some(first) = candidates.first() {
            let initial = self.entries[first].status.is_none();
            candidates.retain(|pr| self.entries[pr].status.is_none() == initial);
        }
        candidates.truncate(MAX_BATCH);
        candidates
    }

    /// Returns PRs that now fall back to the ordinary link preview.
    pub fn apply_result(&mut self, result: GithubPrFetchResult, now: Instant) -> Vec<GithubPrRef> {
        self.active_batches = self.active_batches.saturating_sub(1);
        for pr in &result.refs {
            if let Some(entry) = self.entries.get_mut(pr) {
                entry.in_flight = false;
            }
        }
        match result.outcome {
            FetchOutcome::NoToken | FetchOutcome::Unauthorized => {
                self.token = None;
                self.token_retry_at = Some(now + TOKEN_RETRY);
                Vec::new()
            }
            FetchOutcome::RateLimited { retry_after } => {
                self.backoff_until = Some(now + retry_after);
                self.mark_failed(&result.refs)
            }
            FetchOutcome::Failed => {
                self.note_failure(now);
                self.mark_failed(&result.refs)
            }
            FetchOutcome::Fetched { token, statuses } => {
                self.token = Some(token);
                self.token_retry_at = None;
                let mut fallback = Vec::new();
                let mut transient = Vec::new();
                for (pr, fetch) in statuses {
                    let Some(entry) = self.entries.get_mut(&pr) else {
                        continue;
                    };
                    match fetch {
                        PrFetch::Found(status) => {
                            if entry.status.is_none()
                                && let Some(discovered_at) = entry.discovered_at
                            {
                                tracing::debug!(
                                    load_ms =
                                        now.saturating_duration_since(discovered_at).as_millis()
                                            as u64,
                                    "github pr initial preview ready"
                                );
                            }
                            entry.failed = false;
                            entry.status = Some(status);
                            entry.fetched_at = Some(now);
                        }
                        PrFetch::Inaccessible => {
                            entry.no_access = true;
                            fallback.push(pr);
                        }
                        PrFetch::Transient => transient.push(pr),
                    }
                }
                if transient.is_empty() {
                    // A concurrent success must not erase another batch's backoff.
                    if self.backoff_until.is_none_or(|until| now >= until) {
                        self.backoff_until = None;
                        self.failures = 0;
                    }
                } else {
                    self.note_failure(now);
                    fallback.extend(self.mark_failed(&transient));
                }
                fallback
            }
        }
    }

    fn note_failure(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        let backoff = BASE_BACKOFF
            .saturating_mul(1 << self.failures.saturating_sub(1).min(10))
            .min(MAX_BACKOFF);
        self.backoff_until = Some(now + backoff);
    }

    /// PRs that never loaded fall back to the ordinary preview until a retry succeeds.
    fn mark_failed(&mut self, refs: &[GithubPrRef]) -> Vec<GithubPrRef> {
        let mut fallback = Vec::new();
        for pr in refs {
            if let Some(entry) = self.entries.get_mut(pr)
                && entry.status.is_none()
                && !entry.failed
            {
                entry.failed = true;
                fallback.push(pr.clone());
            }
        }
        fallback
    }
}

fn refresh_due(status: &GithubPrStatus, age: Duration, attention: bool) -> bool {
    let interval = match status.state {
        GithubPrState::Merged | GithubPrState::Closed => return false,
        _ if status.checks.running > 0 => RUNNING_INTERVAL,
        GithubPrState::Draft => DRAFT_INTERVAL,
        GithubPrState::Open => OPEN_INTERVAL,
    };
    let interval = if attention {
        interval.min(ATTENTION_MAX_AGE)
    } else {
        interval
    };
    age >= interval
}

fn fetch_batch(token: Option<Token>, refs: &[GithubPrRef], include_details: bool) -> FetchOutcome {
    let started = Instant::now();
    let resolving_token = token.is_none();
    let Some(token) = token.or_else(|| resolve_token().map(Token)) else {
        tracing::debug!(
            auth_ms = started.elapsed().as_millis() as u64,
            "github pr authentication unavailable"
        );
        return FetchOutcome::NoToken;
    };
    tracing::debug!(
        auth_ms = started.elapsed().as_millis() as u64,
        resolving_token,
        "github pr authentication ready"
    );
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return FetchOutcome::Failed;
    };
    let outcome = runtime.block_on(fetch_batch_async(token, refs, include_details));
    let outcome_label = match &outcome {
        FetchOutcome::NoToken => "no_token",
        FetchOutcome::Unauthorized => "unauthorized",
        FetchOutcome::RateLimited { .. } => "rate_limited",
        FetchOutcome::Failed => "failed",
        FetchOutcome::Fetched { .. } => "fetched",
    };
    tracing::debug!(
        batch_ms = started.elapsed().as_millis() as u64,
        count = refs.len(),
        outcome = outcome_label,
        include_details,
        "github pr batch finished"
    );
    outcome
}

async fn fetch_batch_async(
    token: Token,
    refs: &[GithubPrRef],
    include_details: bool,
) -> FetchOutcome {
    let Ok(client) = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build() else {
        return FetchOutcome::Failed;
    };
    let body = build_batch_query(refs, include_details).to_string();
    let started = Instant::now();
    let response = match client
        .post(GRAPHQL_URL)
        .header("Authorization", format!("bearer {}", token.0))
        .header("User-Agent", "zBase")
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!("github pr fetch failed: {error}");
            return FetchOutcome::Failed;
        }
    };
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return FetchOutcome::Unauthorized;
    }
    if let Some(retry_after) = rate_limit_wait(&response) {
        return FetchOutcome::RateLimited { retry_after };
    }
    let reset_wait = rate_limit_reset_wait(response.headers());
    if !status.is_success() {
        tracing::debug!("github pr fetch returned {status}");
        return FetchOutcome::Failed;
    }
    let Ok(bytes) = response.bytes().await else {
        return FetchOutcome::Failed;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return FetchOutcome::Failed;
    };
    tracing::debug!(
        request_ms = started.elapsed().as_millis() as u64,
        include_details,
        count = refs.len(),
        "github pr response decoded"
    );
    let mut outcome = parse_batch_response(&value, refs, token, reset_wait);
    if let FetchOutcome::Fetched { token, statuses } = &mut outcome {
        let mut tasks = tokio::task::JoinSet::new();
        for (index, (pr, result)) in statuses.iter().enumerate() {
            if matches!(result, PrFetch::Found(_)) && pr.comment.is_some() {
                let client = client.clone();
                let token = token.clone();
                let pr = pr.clone();
                tasks.spawn(async move { (index, fetch_comment(&client, &token, &pr).await) });
            }
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok((index, comment)) = result
                && let PrFetch::Found(status) = &mut statuses[index].1
            {
                status.comment = comment;
            }
        }
    }
    outcome
}

async fn fetch_comment(
    client: &reqwest::Client,
    token: &Token,
    pr: &GithubPrRef,
) -> Option<GithubComment> {
    let endpoint = match pr.comment.as_ref()? {
        GithubCommentRef::Issue(id) => format!("issues/comments/{id}"),
        GithubCommentRef::Review(id) => format!("pulls/comments/{id}"),
    };
    let response = client
        .get(format!(
            "https://api.github.com/repos/{}/{}/{endpoint}",
            pr.owner, pr.repo
        ))
        .bearer_auth(&token.0)
        .header("User-Agent", "zBase")
        .header("Accept", "application/vnd.github.text+json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    parse_comment(&value, pr)
}

fn parse_comment(value: &Value, pr: &GithubPrRef) -> Option<GithubComment> {
    // A comment ID is repository-wide: verify it belongs to the linked PR.
    let linked = GithubPrRef::parse(value.get("html_url")?.as_str()?)?;
    if &linked != pr {
        return None;
    }
    let body = value
        .get("body_text")
        .or_else(|| value.get("body"))?
        .as_str()?;
    let mut excerpt: String = body.chars().take(1200).collect();
    if body.chars().count() > 1200 {
        excerpt.push('…');
    }
    Some(GithubComment {
        author: value
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("ghost")
            .to_string(),
        body: excerpt,
    })
}

fn rate_limit_wait(response: &reqwest::Response) -> Option<Duration> {
    let status = response.status();
    if status != reqwest::StatusCode::FORBIDDEN && status != reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return None;
    }
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    };
    if let Some(seconds) = header("retry-after") {
        return Some(Duration::from_secs(seconds.max(1)));
    }
    if header("x-ratelimit-remaining") == Some(0) {
        return Some(rate_limit_reset_wait(response.headers()).unwrap_or(DEFAULT_RATE_LIMIT_WAIT));
    }
    // Secondary rate limits are 403/429 without the headers above.
    (status == reqwest::StatusCode::TOO_MANY_REQUESTS).then_some(DEFAULT_RATE_LIMIT_WAIT)
}

/// Time until `x-ratelimit-reset`, the epoch second when the quota refills.
fn rate_limit_reset_wait(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let reset = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    Some(Duration::from_secs(reset.saturating_sub(now).max(1)))
}

const SUMMARY_FRAGMENT: &str = "fragment Pr on PullRequest {
  title state isDraft additions deletions
  mergedBy { login }
}";

const PR_FRAGMENT: &str = "fragment Pr on PullRequest {
  title state isDraft additions deletions
  mergedBy { login }
  reviewDecision
  latestOpinionatedReviews(first: 20) { nodes { state } }
  reviewThreads(first: 100) { totalCount nodes { isResolved } }
  commits(last: 1) { nodes { commit { statusCheckRollup { contexts {
    checkRunCountsByState { state count }
    statusContextCountsByState { state count }
  } } } } }
}";

fn build_batch_query(refs: &[GithubPrRef], include_details: bool) -> Value {
    let fragment = if include_details {
        PR_FRAGMENT
    } else {
        SUMMARY_FRAGMENT
    };
    let mut params = Vec::with_capacity(refs.len());
    let mut fields = String::new();
    let mut variables = serde_json::Map::new();
    for (index, pr) in refs.iter().enumerate() {
        params.push(format!(
            "$o{index}: String!, $r{index}: String!, $n{index}: Int!"
        ));
        fields.push_str(&format!(
            "p{index}: repository(owner: $o{index}, name: $r{index}) {{ pullRequest(number: $n{index}) {{ ...Pr }} }}\n"
        ));
        variables.insert(format!("o{index}"), json!(pr.owner));
        variables.insert(format!("r{index}"), json!(pr.repo));
        variables.insert(format!("n{index}"), json!(pr.number));
    }
    let query = format!("query({}) {{\n{fields}}}\n{fragment}", params.join(", "));
    json!({ "query": query, "variables": variables })
}

fn parse_batch_response(
    value: &Value,
    refs: &[GithubPrRef],
    token: Token,
    reset_wait: Option<Duration>,
) -> FetchOutcome {
    let errors = value
        .get("errors")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    fn error_type(error: &Value) -> Option<&str> {
        error.get("type").and_then(Value::as_str)
    }
    if errors
        .iter()
        .any(|error| error_type(error) == Some("RATE_LIMITED"))
    {
        return FetchOutcome::RateLimited {
            retry_after: reset_wait.unwrap_or(DEFAULT_RATE_LIMIT_WAIT),
        };
    }
    let Some(data) = value.get("data").filter(|data| data.is_object()) else {
        tracing::debug!("github pr fetch returned no data: {errors:?}");
        return FetchOutcome::Failed;
    };
    let statuses = refs
        .iter()
        .enumerate()
        .map(|(index, pr)| {
            let alias = format!("p{index}");
            let fetch = match data
                .get(&alias)
                .and_then(|repo| repo.get("pullRequest"))
                .filter(|pr| pr.is_object())
            {
                Some(pr) => parse_pr_status(pr).map_or(PrFetch::Transient, PrFetch::Found),
                None => {
                    // Only a not-found or forbidden error for this alias means no access;
                    // anything else (timeouts, server errors) is worth retrying.
                    let denied = errors.iter().any(|error| {
                        error.pointer("/path/0").and_then(Value::as_str) == Some(alias.as_str())
                            && matches!(error_type(error), Some("NOT_FOUND" | "FORBIDDEN"))
                    });
                    if denied {
                        PrFetch::Inaccessible
                    } else {
                        PrFetch::Transient
                    }
                }
            };
            (pr.clone(), fetch)
        })
        .collect();
    FetchOutcome::Fetched { token, statuses }
}

fn parse_pr_status(pr: &Value) -> Option<GithubPrStatus> {
    let title = pr.get("title")?.as_str()?.to_string();
    let is_draft = pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false);
    let state = match pr.get("state").and_then(Value::as_str)? {
        "MERGED" => GithubPrState::Merged,
        "CLOSED" => GithubPrState::Closed,
        _ if is_draft => GithubPrState::Draft,
        _ => GithubPrState::Open,
    };
    let count = |key: &str| pr.get(key).and_then(Value::as_u64).unwrap_or(0) as u32;
    let merged_by = pr
        .pointer("/mergedBy/login")
        .and_then(Value::as_str)
        .map(str::to_string);

    let threads = pr
        .pointer("/reviewThreads/nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let unresolved_threads = threads
        .iter()
        .filter(|thread| thread.get("isResolved").and_then(Value::as_bool) == Some(false))
        .count() as u32;
    let total_threads = pr
        .pointer("/reviewThreads/totalCount")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(threads.len() as u32);
    let threads_complete = total_threads as usize <= threads.len();

    Some(GithubPrStatus {
        title,
        details_loaded: pr.get("reviewThreads").is_some(),
        comment: None,
        state,
        merged_by,
        additions: count("additions"),
        deletions: count("deletions"),
        checks: parse_checks(pr),
        review: parse_review(pr),
        unresolved_threads,
        total_threads,
        threads_complete,
    })
}

fn parse_checks(pr: &Value) -> GithubCheckCounts {
    let mut counts = GithubCheckCounts::default();
    let Some(contexts) = pr.pointer("/commits/nodes/0/commit/statusCheckRollup/contexts") else {
        return counts;
    };
    for key in ["checkRunCountsByState", "statusContextCountsByState"] {
        let buckets = contexts
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for bucket in buckets {
            let n = bucket.get("count").and_then(Value::as_u64).unwrap_or(0) as u32;
            match bucket
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "SUCCESS" | "NEUTRAL" | "COMPLETED" => counts.passed += n,
                "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
                | "STARTUP_FAILURE" | "STALE" => counts.failed += n,
                "PENDING" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "REQUESTED" | "EXPECTED" => {
                    counts.running += n
                }
                // SKIPPED and unknown states do not count either way.
                _ => {}
            }
        }
    }
    counts
}

fn parse_review(pr: &Value) -> GithubReviewState {
    match pr.get("reviewDecision").and_then(Value::as_str) {
        Some("APPROVED") => return GithubReviewState::Approved,
        Some("CHANGES_REQUESTED") => return GithubReviewState::ChangesRequested,
        Some("REVIEW_REQUIRED") => return GithubReviewState::ReviewRequired,
        _ => {}
    }
    // Repos without required reviews have no decision; use the reviews themselves.
    let reviews = pr
        .pointer("/latestOpinionatedReviews/nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let has = |state: &str| {
        reviews
            .iter()
            .any(|review| review.get("state").and_then(Value::as_str) == Some(state))
    };
    if has("CHANGES_REQUESTED") {
        GithubReviewState::ChangesRequested
    } else if has("APPROVED") {
        GithubReviewState::Approved
    } else {
        GithubReviewState::NoReviews
    }
}

/// `GH_TOKEN` / `GITHUB_TOKEN`, then the GitHub CLI's stored login.
fn resolve_token() -> Option<String> {
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(value) = std::env::var(var) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    gh_candidates().into_iter().find_map(gh_auth_token)
}

/// Apps launched from Finder or the Start menu do not inherit the shell PATH.
fn gh_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("gh")];
    if cfg!(target_os = "macos") {
        candidates.push("/opt/homebrew/bin/gh".into());
        candidates.push("/usr/local/bin/gh".into());
    } else if cfg!(windows) {
        if let Some(dir) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(dir).join("GitHub CLI").join("gh.exe"));
        }
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(dir)
                    .join("Programs")
                    .join("GitHub CLI")
                    .join("gh.exe"),
            );
        }
    } else {
        candidates.push("/usr/local/bin/gh".into());
        candidates.push("/usr/bin/gh".into());
    }
    candidates
}

fn gh_auth_token(program: PathBuf) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(["auth", "token", "--hostname", "github.com"])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.stdout(Stdio::piped()).spawn().ok()?;
    let deadline = Instant::now() + GH_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::debug!("gh auth token did not finish in time");
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    // The token is a single short line, so it fits in the pipe buffer while we wait.
    let mut stdout = String::new();
    child.stdout.take()?.read_to_string(&mut stdout).ok()?;
    let token = stdout.trim().to_string();
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn pr(number: u64) -> GithubPrRef {
        GithubPrRef {
            owner: "o".into(),
            repo: "r".into(),
            number,
            comment: None,
        }
    }

    fn status(state: GithubPrState, running: u32) -> GithubPrStatus {
        GithubPrStatus {
            title: "t".into(),
            details_loaded: true,
            comment: None,
            state,
            merged_by: None,
            additions: 0,
            deletions: 0,
            checks: GithubCheckCounts {
                passed: 1,
                failed: 0,
                running,
            },
            review: GithubReviewState::NoReviews,
            unresolved_threads: 0,
            total_threads: 0,
            threads_complete: true,
        }
    }

    fn fetched(
        service: &mut GithubPrService,
        refs: &[GithubPrRef],
        s: GithubPrStatus,
        now: Instant,
    ) {
        for r in refs {
            service.entries.entry(r.clone()).or_default();
        }
        service.apply_result(
            GithubPrFetchResult {
                refs: refs.to_vec(),
                outcome: FetchOutcome::Fetched {
                    token: Token("t".into()),
                    statuses: refs
                        .iter()
                        .map(|r| (r.clone(), PrFetch::Found(s.clone())))
                        .collect(),
                },
            },
            now,
        );
    }

    #[test]
    fn parses_full_status() {
        let value = json!({
            "title": "Add Windows support", "state": "OPEN", "isDraft": false,
            "additions": 1552, "deletions": 551, "mergedBy": null,
            "reviewDecision": null,
            "latestOpinionatedReviews": { "nodes": [{ "state": "APPROVED" }, { "state": "COMMENTED" }] },
            "reviewThreads": { "totalCount": 3, "nodes": [
                { "isResolved": true }, { "isResolved": false }, { "isResolved": false }
            ] },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": {
                "checkRunCountsByState": [
                    { "state": "SUCCESS", "count": 8 }, { "state": "FAILURE", "count": 1 },
                    { "state": "IN_PROGRESS", "count": 2 }, { "state": "SKIPPED", "count": 4 }
                ],
                "statusContextCountsByState": [{ "state": "PENDING", "count": 1 }]
            } } } }] }
        });
        let status = parse_pr_status(&value).unwrap();
        assert_eq!(status.state, GithubPrState::Open);
        assert_eq!(status.review, GithubReviewState::Approved);
        assert_eq!(
            status.checks,
            GithubCheckCounts {
                passed: 8,
                failed: 1,
                running: 3
            }
        );
        assert_eq!((status.unresolved_threads, status.total_threads), (2, 3));
        assert!(status.threads_complete);
        assert_eq!((status.additions, status.deletions), (1552, 551));
    }

    #[test]
    fn review_decision_wins_and_missing_rollup_means_no_checks() {
        let value = json!({
            "title": "x", "state": "OPEN", "isDraft": true,
            "reviewDecision": "CHANGES_REQUESTED",
            "latestOpinionatedReviews": { "nodes": [{ "state": "APPROVED" }] },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": null } }] }
        });
        let status = parse_pr_status(&value).unwrap();
        assert_eq!(status.state, GithubPrState::Draft);
        assert_eq!(status.review, GithubReviewState::ChangesRequested);
        assert_eq!(status.checks.total(), 0);
    }

    #[test]
    fn missing_repository_means_no_access() {
        let refs = [pr(1), pr(2)];
        let value = json!({
            "data": { "p0": { "pullRequest": { "title": "x", "state": "MERGED" } }, "p1": null },
            "errors": [{ "type": "NOT_FOUND", "path": ["p1"] }]
        });
        let FetchOutcome::Fetched { statuses, .. } =
            parse_batch_response(&value, &refs, Token("t".into()), None)
        else {
            panic!("expected fetched");
        };
        assert!(matches!(&statuses[0].1, PrFetch::Found(s) if s.state == GithubPrState::Merged));
        assert!(matches!(statuses[1].1, PrFetch::Inaccessible));
    }

    #[test]
    fn rate_limited_graphql_error() {
        let value = json!({ "data": null, "errors": [{ "type": "RATE_LIMITED" }] });
        assert!(matches!(
            parse_batch_response(&value, &[pr(1)], Token("t".into()), None),
            FetchOutcome::RateLimited { .. }
        ));
    }

    #[test]
    fn comment_preview_matches_target_and_limits_excerpt() {
        let target = GithubPrRef::parse("https://github.com/o/r/pull/7#issuecomment-42").unwrap();
        let mut value = json!({
            "html_url": "https://github.com/o/r/pull/7#issuecomment-42",
            "user": { "login": "octocat" },
            "body_text": "é".repeat(1201)
        });
        let comment = parse_comment(&value, &target).unwrap();
        assert_eq!(comment.author, "octocat");
        assert_eq!(comment.body.chars().count(), 1201);
        assert!(comment.body.ends_with('…'));
        value["html_url"] = json!("https://github.com/o/r/pull/8#issuecomment-42");
        assert!(parse_comment(&value, &target).is_none());
    }

    #[test]
    fn batch_query_uses_variables() {
        let query = build_batch_query(&[pr(7), pr(9)], true);
        assert_eq!(query["variables"]["n1"], json!(9));
        assert_eq!(query["variables"]["o0"], json!("o"));
        assert!(
            query["query"]
                .as_str()
                .unwrap()
                .contains("p1: repository(owner: $o1")
        );
    }

    #[test]
    fn summary_query_omits_expensive_details() {
        let query = build_batch_query(&[pr(1)], false);
        let query = query["query"].as_str().unwrap();
        assert!(query.contains("title state isDraft"));
        for field in [
            "reviewThreads",
            "latestOpinionatedReviews",
            "commits",
            "reviewDecision",
        ] {
            assert!(!query.contains(field));
        }
        let summary = parse_pr_status(&json!({ "title": "Ready first", "state": "OPEN" })).unwrap();
        assert!(!summary.details_loaded);
    }

    #[test]
    fn summaries_receive_details_even_for_merged_prs_and_survive_detail_failures() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        let mut summary = status(GithubPrState::Merged, 0);
        summary.details_loaded = false;
        fetched(&mut service, &[pr(1)], summary, now);
        assert_eq!(service.select_due(&[pr(1)], true, false, now), vec![pr(1)]);
        service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(1)],
                outcome: FetchOutcome::Failed,
            },
            now,
        );
        let preview = service.preview(&pr(1), now).unwrap();
        assert!(!preview.status.unwrap().details_loaded);
        assert!(service.backoff_until.is_some());
        fetched(
            &mut service,
            &[pr(1)],
            status(GithubPrState::Merged, 0),
            now + BASE_BACKOFF,
        );
        assert!(
            service
                .preview(&pr(1), now)
                .unwrap()
                .status
                .unwrap()
                .details_loaded
        );
        assert!(
            service
                .select_due(&[pr(1)], true, true, now + OPEN_INTERVAL)
                .is_empty()
        );
    }

    #[test]
    fn refresh_intervals_follow_state() {
        let open = status(GithubPrState::Open, 0);
        let running = status(GithubPrState::Draft, 2);
        let draft = status(GithubPrState::Draft, 0);
        let merged = status(GithubPrState::Merged, 0);
        let secs = Duration::from_secs;
        assert!(!refresh_due(&running, secs(29), false));
        assert!(refresh_due(&running, secs(30), false));
        assert!(!refresh_due(&open, secs(179), false));
        assert!(refresh_due(&open, secs(180), false));
        assert!(!refresh_due(&draft, secs(299), false));
        assert!(refresh_due(&open, secs(30), true));
        assert!(!refresh_due(&merged, secs(100_000), true));
    }

    #[test]
    fn only_recent_prs_refresh_and_only_when_focused() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        let refs: Vec<_> = (1..=30).map(pr).collect();
        fetched(&mut service, &refs, status(GithubPrState::Open, 0), now);
        let later = now + OPEN_INTERVAL;
        assert_eq!(
            service.select_due(&refs, true, false, later).len(),
            MAX_BATCH
        );
        assert!(service.select_due(&refs, false, false, later).is_empty());

        // New links are fetched once even when unfocused or beyond the refresh cap.
        let mut with_new = refs.clone();
        with_new.push(pr(99));
        service.entries.insert(pr(99), Entry::default());
        assert_eq!(
            service.select_due(&with_new, false, false, later),
            vec![pr(99)]
        );
    }

    #[test]
    fn initial_loads_precede_refreshes_and_keep_viewport_order() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        let cached: Vec<_> = (1..=25).map(pr).collect();
        fetched(&mut service, &cached, status(GithubPrState::Open, 0), now);
        service.entries.insert(pr(99), Entry::default());
        service.entries.insert(pr(98), Entry::default());
        let mut refs = cached;
        refs.extend([pr(99), pr(98)]);
        let due = service.select_due(&refs, true, false, now + OPEN_INTERVAL);
        assert_eq!(&due[..2], &[pr(99), pr(98)]);
        assert_eq!(due.len(), 2);
        service.active_batches = 1;
        assert_eq!(
            service.select_due(&refs, true, false, now + OPEN_INTERVAL),
            vec![pr(99), pr(98)]
        );
    }

    #[test]
    fn concurrent_success_preserves_backoff_and_other_inflight_entries() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        service.active_batches = 2;
        for number in [1, 2] {
            service.entries.insert(
                pr(number),
                Entry {
                    in_flight: true,
                    ..Entry::default()
                },
            );
        }
        service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(1)],
                outcome: FetchOutcome::Failed,
            },
            now,
        );
        let backoff = service.backoff_until;
        assert_eq!(service.active_batches, 1);
        assert!(service.entries[&pr(2)].in_flight);
        service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(2)],
                outcome: FetchOutcome::Fetched {
                    token: Token("t".into()),
                    statuses: vec![(pr(2), PrFetch::Found(status(GithubPrState::Open, 0)))],
                },
            },
            now,
        );
        assert_eq!(service.active_batches, 0);
        assert_eq!(service.backoff_until, backoff);
        assert!(!service.entries[&pr(2)].in_flight);
    }

    #[test]
    fn missing_token_waits_before_retrying() {
        let mut service = GithubPrService::new();
        let (sender, _receiver) = mpsc::channel();
        let now = Instant::now();
        service.entries.insert(pr(1), Entry::default());
        service.active_batches = 1;
        service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(1)],
                outcome: FetchOutcome::NoToken,
            },
            now,
        );
        assert!(service.preview(&pr(1), now).unwrap().needs_auth);
        assert!(!service.poll(&[pr(1)], true, now + Duration::from_secs(10), &sender));
        assert_eq!(service.active_batches, 0);
    }

    #[test]
    fn inaccessible_and_failed_prs_fall_back() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        service.entries.insert(pr(1), Entry::default());
        service.entries.insert(pr(2), Entry::default());
        let fallback = service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(1)],
                outcome: FetchOutcome::Fetched {
                    token: Token("t".into()),
                    statuses: vec![(pr(1), PrFetch::Inaccessible)],
                },
            },
            now,
        );
        assert_eq!(fallback, vec![pr(1)]);
        assert!(service.preview(&pr(1), now).is_none());
        assert!(!service.handles_url("https://github.com/o/r/pull/1", now));

        let fallback = service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(2)],
                outcome: FetchOutcome::Failed,
            },
            now,
        );
        assert_eq!(fallback, vec![pr(2)]);
        assert!(service.preview(&pr(2), now).is_none());
        assert!(service.backoff_until.is_some());
    }

    #[test]
    #[ignore = "calls api.github.com with the local GitHub CLI login"]
    fn fetches_real_pull_requests() {
        let merged = GithubPrRef {
            owner: "cameroncooper".into(),
            repo: "zbase".into(),
            number: 44,
            comment: None,
        };
        let missing = GithubPrRef {
            number: 999_999,
            ..merged.clone()
        };
        let outcome = fetch_batch(None, &[merged, missing], true);
        let FetchOutcome::Fetched { statuses, .. } = outcome else {
            panic!("unexpected outcome: {outcome:?}");
        };
        let PrFetch::Found(status) = &statuses[0].1 else {
            panic!("PR #44: {:?}", statuses[0].1);
        };
        eprintln!("{status:#?}");
        assert_eq!(status.state, GithubPrState::Merged);
        assert_eq!(status.title, "Add Windows support");
        assert!(status.checks.passed > 0);
        assert!(matches!(statuses[1].1, PrFetch::Inaccessible));
    }

    #[test]
    fn stale_status_shows_refreshing() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        fetched(&mut service, &[pr(1)], status(GithubPrState::Open, 0), now);
        service.entries.get_mut(&pr(1)).unwrap().in_flight = true;
        assert!(
            !service
                .preview(&pr(1), now + Duration::from_secs(10))
                .unwrap()
                .refreshing
        );
        assert!(
            service
                .preview(&pr(1), now + STALE_AFTER)
                .unwrap()
                .refreshing
        );
    }

    #[test]
    fn transient_alias_errors_retry_instead_of_disabling() {
        let refs = [pr(1), pr(2)];
        let value = json!({
            "data": { "p0": { "pullRequest": null }, "p1": { "pullRequest": null } },
            "errors": [
                { "type": "INTERNAL", "path": ["p0", "pullRequest", "reviewThreads"] },
                { "type": "NOT_FOUND", "path": ["p1", "pullRequest"] }
            ]
        });
        let outcome = parse_batch_response(&value, &refs, Token("t".into()), None);
        let FetchOutcome::Fetched { statuses, .. } = &outcome else {
            panic!("expected fetched");
        };
        assert!(matches!(statuses[0].1, PrFetch::Transient));
        assert!(matches!(statuses[1].1, PrFetch::Inaccessible));

        let mut service = GithubPrService::new();
        let now = Instant::now();
        fetched(&mut service, &[pr(1)], status(GithubPrState::Open, 0), now);
        service.entries.insert(pr(2), Entry::default());
        let fallback = service.apply_result(
            GithubPrFetchResult {
                refs: refs.to_vec(),
                outcome,
            },
            now,
        );
        assert_eq!(fallback, vec![pr(2)]);
        // The PR that errored keeps its last status and is retried after the backoff.
        assert!(service.preview(&pr(1), now).unwrap().status.is_some());
        assert!(!service.entries[&pr(1)].no_access);
        assert!(service.backoff_until.is_some());
    }

    #[test]
    fn truncated_threads_are_not_complete() {
        let nodes: Vec<_> = (0..100).map(|_| json!({ "isResolved": true })).collect();
        let value = json!({
            "title": "x", "state": "OPEN",
            "reviewThreads": { "totalCount": 101, "nodes": nodes }
        });
        let status = parse_pr_status(&value).unwrap();
        assert_eq!(status.unresolved_threads, 0);
        assert!(!status.threads_complete);
    }

    #[test]
    fn missing_token_shows_auth_hint_for_every_unloaded_pr() {
        let mut service = GithubPrService::new();
        let (sender, _receiver) = mpsc::channel();
        let now = Instant::now();
        let refs: Vec<_> = (1..=30).map(pr).collect();
        for r in &refs {
            service.entries.insert(r.clone(), Entry::default());
        }
        service.active_batches = 1;
        service.apply_result(
            GithubPrFetchResult {
                refs: refs[..25].to_vec(),
                outcome: FetchOutcome::NoToken,
            },
            now,
        );
        assert!(service.preview(&pr(30), now).unwrap().needs_auth);
        // Links that show up later get the hint without waiting for the retry.
        service.poll(&[pr(31)], true, now, &sender);
        assert!(service.preview(&pr(31), now).unwrap().needs_auth);
    }

    #[test]
    fn rate_limit_falls_back_for_unloaded_prs() {
        let mut service = GithubPrService::new();
        let now = Instant::now();
        service.entries.insert(pr(1), Entry::default());
        let fallback = service.apply_result(
            GithubPrFetchResult {
                refs: vec![pr(1)],
                outcome: FetchOutcome::RateLimited {
                    retry_after: Duration::from_secs(600),
                },
            },
            now,
        );
        assert_eq!(fallback, vec![pr(1)]);
        assert!(service.preview(&pr(1), now).is_none());
    }

    #[test]
    fn token_is_redacted_in_debug_output() {
        let outcome = FetchOutcome::Fetched {
            token: Token("ghp_secret".into()),
            statuses: Vec::new(),
        };
        assert!(!format!("{outcome:?}").contains("ghp_secret"));
    }
}
