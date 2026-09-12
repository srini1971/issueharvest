use crate::{
    IssueMatch, IssueQuery, Repository, SearchError, TrendOrder, TrendingRepository,
    TrendingRepositoryQuery,
    github::{GithubIssue, RepositorySearchResponse},
    priority,
};
use futures_util::{StreamExt, TryStreamExt, stream};
use reqwest::{Client, header};

const API_ROOT: &str = "https://api.github.com";
const USER_AGENT: &str = "issueharvest/0.1";

/// Entry point for direct, authenticated GitHub issue discovery.
#[derive(Clone)]
pub struct Finder {
    client: Client,
}

pub struct FinderBuilder {
    token: Option<String>,
}

impl Finder {
    pub fn builder() -> FinderBuilder {
        FinderBuilder { token: None }
    }

    /// Builds a finder using the `GITHUB_TOKEN` environment variable.
    ///
    /// This avoids putting a credential in source code. The builder remains
    /// available for callers using a different secret store or token name.
    pub fn from_env() -> Result<Self, SearchError> {
        Self::builder()
            .github_token(std::env::var("GITHUB_TOKEN")?)
            .build()
    }

    /// Searches candidate repositories, fetches their open issues, then returns ranked matches.
    pub async fn search(&self, query: IssueQuery) -> Result<Vec<IssueMatch>, SearchError> {
        let repositories = self.search_repositories(&query).await?;
        let concurrency = query.max_concurrency;
        let per_repository: Vec<Vec<IssueMatch>> =
            stream::iter(repositories.into_iter().map(|repository| {
                let finder = self.clone();
                async move { finder.open_issues(repository).await }
            }))
            .buffer_unordered(concurrency)
            .try_collect()
            .await?;
        let mut matches: Vec<IssueMatch> = per_repository
            .into_iter()
            .flatten()
            .filter(|issue| {
                issue.priority.level.rank() >= query.priority_at_least.rank()
                    && (!query.prefer_help_wanted || issue.help_wanted)
            })
            .collect();

        // Stable, explainable ordering: urgency first, then maintainer invitation,
        // recent discussion, and finally repository popularity.
        matches.sort_by(|left, right| {
            right
                .priority
                .level
                .rank()
                .cmp(&left.priority.level.rank())
                .then_with(|| right.help_wanted.cmp(&left.help_wanted))
                .then_with(|| right.comments.cmp(&left.comments))
                .then_with(|| right.repository.stars.cmp(&left.repository.stars))
        });
        matches.truncate(query.limit);
        Ok(matches)
    }

    /// Discovers public repositories for a language and returns them in the requested
    /// trend-score order. The default is ascending, as requested by the query API.
    pub async fn trending_repositories(
        &self,
        query: TrendingRepositoryQuery,
    ) -> Result<Vec<TrendingRepository>, SearchError> {
        let repositories = self
            .search_repositories_raw(
                &query.language,
                query.min_stars,
                query.pushed_after.as_deref(),
                query.limit,
            )
            .await?;
        let count = repositories.len();
        let mut matches: Vec<TrendingRepository> = repositories
            .into_iter()
            .enumerate()
            .map(|(index, repository)| {
                // GitHub returns this search sorted by most recently updated. We preserve that
                // activity signal without claiming that GitHub itself exposes a Trending API.
                let activity_score = (count.saturating_sub(index)) as f64;
                let popularity_score = (repository.stars as f64 + 1.0).ln() * 10.0
                    + (repository.forks as f64 + 1.0).ln() * 4.0;
                let trend_score = activity_score + popularity_score;
                let mut reasons = vec![
                    format!("GitHub search activity position: {} of {count}", index + 1),
                    format!("{} stars", repository.stars),
                    format!("{} forks", repository.forks),
                ];
                if let Some(pushed_at) = &repository.pushed_at {
                    reasons.push(format!("last pushed: {pushed_at}"));
                }
                TrendingRepository {
                    repository,
                    trend_score,
                    reasons,
                }
            })
            .collect();
        matches.sort_by(|left, right| {
            left.trend_score
                .partial_cmp(&right.trend_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if query.order == TrendOrder::Descending {
            matches.reverse();
        }
        Ok(matches)
    }

    async fn search_repositories(
        &self,
        query: &IssueQuery,
    ) -> Result<Vec<Repository>, SearchError> {
        self.search_repositories_raw(
            &query.language,
            query.min_stars,
            query.pushed_after.as_deref(),
            query.repositories_to_scan,
        )
        .await
    }

    async fn search_repositories_raw(
        &self,
        language: &str,
        min_stars: u64,
        pushed_after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Repository>, SearchError> {
        let mut text = format!("language:{language} stars:>={min_stars} archived:false");
        if let Some(date) = pushed_after {
            text.push_str(&format!(" pushed:>{date}"));
        }
        let response = self
            .client
            .get(format!("{API_ROOT}/search/repositories"))
            .query(&[
                ("q", text),
                ("sort", "updated".to_owned()),
                ("order", "desc".to_owned()),
                ("per_page", limit.clamp(1, 100).to_string()),
            ])
            .send()
            .await?
            .error_for_status()?;
        let payload: RepositorySearchResponse = response.json().await?;
        Ok(payload.items.into_iter().map(Repository::from).collect())
    }

    async fn open_issues(&self, repository: Repository) -> Result<Vec<IssueMatch>, SearchError> {
        let response = self
            .client
            .get(format!(
                "{API_ROOT}/repos/{}/{}/issues",
                repository.owner, repository.name
            ))
            .query(&[
                ("state", "open"),
                ("per_page", "100"),
                ("sort", "updated"),
                ("direction", "desc"),
            ])
            .send()
            .await?
            .error_for_status()?;
        let issues: Vec<GithubIssue> = response.json().await?;
        Ok(issues
            .into_iter()
            .filter(|issue| issue.pull_request.is_none())
            .map(|issue| {
                let labels: Vec<String> =
                    issue.labels.into_iter().map(|label| label.name).collect();
                let priority = priority::assess(&labels);
                let help_wanted = labels
                    .iter()
                    .any(|label| matches_label(label, &["help wanted", "up-for-grabs"]));
                let good_first_issue = labels
                    .iter()
                    .any(|label| matches_label(label, &["good first issue", "beginner-friendly"]));
                IssueMatch {
                    repository: repository.clone(),
                    number: issue.number,
                    title: issue.title,
                    html_url: issue.html_url,
                    labels,
                    comments: issue.comments,
                    created_at: issue.created_at,
                    updated_at: issue.updated_at,
                    priority,
                    help_wanted,
                    good_first_issue,
                }
            })
            .collect())
    }
}

impl FinderBuilder {
    pub fn github_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn build(self) -> Result<Finder, SearchError> {
        let token = self.token.ok_or(SearchError::MissingToken)?;
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(USER_AGENT),
        );
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {token}"))
                .expect("GitHub token must form a valid HTTP header"),
        );
        Ok(Finder {
            client: Client::builder().default_headers(headers).build()?,
        })
    }
}

fn matches_label(label: &str, aliases: &[&str]) -> bool {
    let value = label.trim().to_ascii_lowercase();
    aliases.contains(&value.as_str())
}
