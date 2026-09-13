use crate::{Priority, PriorityAssessment};
use thiserror::Error;

/// Public metadata needed to judge whether a repository is active and relevant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub stars: u64,
    pub forks: u64,
    pub pushed_at: Option<String>,
}

/// A repository selected by the crate's transparent activity-and-popularity heuristic.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendingRepository {
    pub repository: Repository,
    /// A relative score within this discovery result; higher means more active/popular.
    pub trend_score: f64,
    /// Human-readable inputs used to calculate `trend_score`.
    pub reasons: Vec<String>,
}

/// Direction used when returning repository-discovery results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrendOrder {
    /// Least trending eligible repository first.
    #[default]
    Ascending,
    /// Most trending eligible repository first.
    Descending,
}

/// Repository discovery criteria. GitHub has no public Trending API, so this
/// crate defines trending as recently updated eligible repositories, ranked by
/// their GitHub search activity position, stars, and forks.
#[derive(Debug, Clone)]
pub struct TrendingRepositoryQuery {
    pub language: String,
    pub min_stars: u64,
    pub pushed_after: Option<String>,
    pub order: TrendOrder,
    pub limit: usize,
}

/// Query for developers with publicly verifiable, merged fixes for difficult bugs.
///
/// Only `language` is required. The default scans active repositories from the
/// last 180 days and considers recognized High and Critical bug labels.
#[derive(Debug, Clone)]
pub struct DeveloperImpactQuery {
    pub language: String,
    pub min_repository_stars: u64,
    pub pushed_after: Option<String>,
    pub active_within_days: u64,
    pub priority_at_least: Priority,
    pub repositories_to_scan: usize,
    pub limit: usize,
}

impl DeveloperImpactQuery {
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            min_repository_stars: 0,
            pushed_after: None,
            active_within_days: 180,
            priority_at_least: Priority::High,
            repositories_to_scan: 25,
            limit: 20,
        }
    }

    pub fn min_repository_stars(mut self, stars: u64) -> Self {
        self.min_repository_stars = stars;
        self
    }
    /// Overrides the default rolling activity window with a GitHub search date.
    pub fn pushed_after(mut self, date: impl Into<String>) -> Self {
        self.pushed_after = Some(date.into());
        self
    }
    pub fn active_within_days(mut self, days: u64) -> Self {
        self.active_within_days = days;
        self
    }
    pub fn priority_at_least(mut self, priority: Priority) -> Self {
        self.priority_at_least = priority;
        self
    }
    pub fn repositories_to_scan(mut self, value: usize) -> Self {
        self.repositories_to_scan = value;
        self
    }
    pub fn limit(mut self, value: usize) -> Self {
        self.limit = value;
        self
    }
}

/// A single merged pull request that the crate has verified as a bug-fix candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedBugFix {
    pub repository: Repository,
    pub issue_number: u64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_labels: Vec<String>,
    pub priority: PriorityAssessment,
    pub pull_request_number: u64,
    pub pull_request_url: String,
    pub merged_at: String,
    pub score_contribution: f64,
    pub evidence: Vec<String>,
}

/// An evidence-backed developer leaderboard entry.
#[derive(Debug, Clone, PartialEq)]
pub struct DeveloperImpact {
    pub login: String,
    pub profile_url: String,
    pub impact_score: f64,
    pub critical_bugs_fixed: u32,
    pub high_priority_bugs_fixed: u32,
    pub repositories_contributed_to: u32,
    pub fixes: Vec<VerifiedBugFix>,
}

impl TrendingRepositoryQuery {
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            min_stars: 0,
            pushed_after: None,
            order: TrendOrder::Ascending,
            limit: 20,
        }
    }

    pub fn min_stars(mut self, stars: u64) -> Self {
        self.min_stars = stars;
        self
    }
    /// Accepts a GitHub search date, e.g. `"2026-06-01"`.
    pub fn pushed_after(mut self, date: impl Into<String>) -> Self {
        self.pushed_after = Some(date.into());
        self
    }
    pub fn order(mut self, order: TrendOrder) -> Self {
        self.order = order;
        self
    }
    pub fn limit(mut self, value: usize) -> Self {
        self.limit = value;
        self
    }
}

/// A discovered issue with its original GitHub labels and normalized priority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueMatch {
    pub repository: Repository,
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub labels: Vec<String>,
    pub comments: u64,
    pub created_at: String,
    pub updated_at: String,
    pub priority: PriorityAssessment,
    pub help_wanted: bool,
    pub good_first_issue: bool,
}

/// Criteria used for discovering repositories and their open issues.
#[derive(Debug, Clone)]
pub struct IssueQuery {
    pub language: String,
    pub min_stars: u64,
    pub pushed_after: Option<String>,
    pub priority_at_least: Priority,
    pub prefer_help_wanted: bool,
    pub limit: usize,
    pub repositories_to_scan: usize,
    pub max_concurrency: usize,
}

impl IssueQuery {
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            min_stars: 0,
            pushed_after: None,
            priority_at_least: Priority::Unknown,
            prefer_help_wanted: false,
            limit: 20,
            repositories_to_scan: 10,
            max_concurrency: 5,
        }
    }

    pub fn min_stars(mut self, stars: u64) -> Self {
        self.min_stars = stars;
        self
    }
    pub fn pushed_after(mut self, date: impl Into<String>) -> Self {
        self.pushed_after = Some(date.into());
        self
    }
    pub fn priority_at_least(mut self, priority: Priority) -> Self {
        self.priority_at_least = priority;
        self
    }
    pub fn prefer_help_wanted(mut self, value: bool) -> Self {
        self.prefer_help_wanted = value;
        self
    }
    pub fn limit(mut self, value: usize) -> Self {
        self.limit = value;
        self
    }
    pub fn repositories_to_scan(mut self, value: usize) -> Self {
        self.repositories_to_scan = value;
        self
    }
    pub fn max_concurrency(mut self, value: usize) -> Self {
        self.max_concurrency = value.max(1);
        self
    }
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("a GitHub token is required; set one with Finder::builder().github_token(...)")]
    MissingToken,
    #[error("GITHUB_TOKEN is not set or cannot be read from the environment: {0}")]
    Environment(#[from] std::env::VarError),
    #[error("GitHub API request failed: {0}")]
    Http(#[from] reqwest::Error),
}
