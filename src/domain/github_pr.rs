/// A pull request on github.com, identified by its URL path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GithubPrRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub comment: Option<GithubCommentRef>,
}

impl GithubPrRef {
    /// Parses `github.com/{owner}/{repo}/pull/{number}`, with or without a
    /// scheme or `www.`, ignoring trailing tabs (`/files`) and queries, preserving comment anchors.
    pub fn parse(url: &str) -> Option<Self> {
        let trimmed = url.trim();
        let lower = trimmed.to_ascii_lowercase();
        let after_scheme = ["https://", "http://"]
            .iter()
            .find_map(|scheme| lower.starts_with(scheme).then(|| &trimmed[scheme.len()..]))
            .unwrap_or(trimmed);
        let (host, path) = after_scheme.split_once('/')?;
        let host = host.to_ascii_lowercase();
        if host != "github.com" && host != "www.github.com" {
            return None;
        }
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let mut segments = path.split('/');
        let owner = segments.next().filter(|s| is_valid_name(s))?;
        let repo = segments.next().filter(|s| is_valid_name(s))?;
        if segments.next() != Some("pull") {
            return None;
        }
        let number = segments
            .next()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))?
            .parse::<u64>()
            .ok()
            // GraphQL `Int` is 32-bit; a larger number would fail the whole batch.
            .filter(|n| *n > 0 && *n <= i32::MAX as u64)?;
        Some(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            comment: trimmed
                .split_once('#')
                .and_then(|(_, anchor)| GithubCommentRef::parse(anchor)),
        })
    }

    pub fn repo_label(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

/// The two comment anchors GitHub uses on pull requests.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GithubCommentRef {
    Issue(u64),
    Review(u64),
}

impl GithubCommentRef {
    fn parse(anchor: &str) -> Option<Self> {
        let (kind, id) = if let Some(id) = anchor.strip_prefix("issuecomment-") {
            ("issuecomment", id)
        } else {
            ("discussion_r", anchor.strip_prefix("discussion_r")?)
        };
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let id = id.parse::<u64>().ok().filter(|id| *id > 0)?;
        match kind {
            "issuecomment" => Some(Self::Issue(id)),
            "discussion_r" => Some(Self::Review(id)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubComment {
    pub author: String,
    pub body: String,
}

fn is_valid_name(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubPrState {
    Open,
    Draft,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GithubCheckCounts {
    pub passed: u32,
    pub failed: u32,
    pub running: u32,
}

impl GithubCheckCounts {
    pub fn total(&self) -> u32 {
        self.passed + self.failed + self.running
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubReviewState {
    Approved,
    ChangesRequested,
    ReviewRequired,
    NoReviews,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubPrStatus {
    pub title: String,
    /// Checks and reviews have arrived after the initial summary.
    pub details_loaded: bool,
    pub comment: Option<GithubComment>,
    pub state: GithubPrState,
    pub merged_by: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub checks: GithubCheckCounts,
    pub review: GithubReviewState,
    /// Counted among the first page of review threads only.
    pub unresolved_threads: u32,
    pub total_threads: u32,
    /// Whether every review thread was counted, so "all resolved" can be trusted.
    pub threads_complete: bool,
}

/// What a PR link preview shows right now. Fetched per viewer and never persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubPrPreview {
    pub pr: GithubPrRef,
    pub status: Option<GithubPrStatus>,
    /// No GitHub token is available, so status cannot be fetched.
    pub needs_auth: bool,
    /// Shown status is old and a refresh is in flight.
    pub refreshing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(owner: &str, repo: &str, number: u64) -> Option<GithubPrRef> {
        Some(GithubPrRef {
            owner: owner.into(),
            repo: repo.into(),
            number,
            comment: None,
        })
    }

    #[test]
    fn parses_pull_request_urls() {
        assert_eq!(
            GithubPrRef::parse("https://github.com/cameroncooper/zbase/pull/44"),
            pr("cameroncooper", "zbase", 44)
        );
        assert_eq!(
            GithubPrRef::parse("github.com/a-b/c.d_e/pull/7/files?w=1#diff-123"),
            pr("a-b", "c.d_e", 7)
        );
        assert_eq!(
            GithubPrRef::parse("HTTPS://WWW.GitHub.com/o/r/pull/3#issuecomment-9"),
            Some(GithubPrRef {
                comment: Some(GithubCommentRef::Issue(9)),
                ..pr("o", "r", 3).unwrap()
            })
        );
        assert_eq!(
            GithubPrRef::parse("https://github.com/o/r/pull/12/commits/abc123"),
            pr("o", "r", 12)
        );
    }

    #[test]
    fn comment_anchors_are_distinct_cache_keys() {
        let plain = GithubPrRef::parse("https://github.com/o/r/pull/7").unwrap();
        let issue = GithubPrRef::parse("https://github.com/o/r/pull/7#issuecomment-42").unwrap();
        let review = GithubPrRef::parse("https://github.com/o/r/pull/7#discussion_r42").unwrap();
        assert_eq!(issue.comment, Some(GithubCommentRef::Issue(42)));
        assert_eq!(review.comment, Some(GithubCommentRef::Review(42)));
        assert_ne!(plain, issue);
        assert_ne!(issue, review);
        for anchor in [
            "issuecomment-0",
            "issuecomment-nope",
            "discussion_r",
            "diff-42",
        ] {
            assert_eq!(
                GithubPrRef::parse(&format!("https://github.com/o/r/pull/7#{anchor}")).unwrap(),
                plain
            );
        }
    }

    #[test]
    fn rejects_other_urls() {
        for url in [
            "https://github.com/o/r/pulls",
            "https://github.com/o/r/pull/",
            "https://github.com/o/r/pull/0",
            "https://github.com/o/r/pull/12abc",
            "https://github.com/o/r/pull/99999999999",
            "https://github.com/o/r/issues/12",
            "https://gist.github.com/o/r/pull/12",
            "https://github.com.evil.io/o/r/pull/12",
            "https://github.com/o/r",
            "https://example.com/o/r/pull/1",
        ] {
            assert_eq!(GithubPrRef::parse(url), None, "{url}");
        }
    }
}
