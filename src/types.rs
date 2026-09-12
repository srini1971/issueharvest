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
