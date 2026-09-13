use crate::{
    DeveloperImpact, DeveloperImpactQuery, IssueMatch, IssueQuery, Priority, Repository,
    SearchError, TrendOrder, TrendingRepository, TrendingRepositoryQuery, VerifiedBugFix,
    github::{GithubIssue, GithubPullRequest, RepositorySearchResponse, TimelineEvent},
    priority,
};
use chrono::{Duration, Utc};
use futures_util::{StreamExt, TryStreamExt, stream};
use reqwest::{Client, header};
use std::collections::{HashMap, HashSet};

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

    /// Ranks developers by merged pull requests that explicitly reference recognized
    /// high-severity bug issues. Results contain the underlying issue/PR evidence.
    pub async fn developer_impact(
        &self,
        query: DeveloperImpactQuery,
    ) -> Result<Vec<DeveloperImpact>, SearchError> {
        let default_since = (Utc::now() - Duration::days(query.active_within_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let since = query.pushed_after.as_deref().unwrap_or(&default_since);
        let repositories = self
            .search_repositories_raw(
                &query.language,
                query.min_repository_stars,
                Some(since),
                query.repositories_to_scan,
            )
            .await?;

        let mut developers: HashMap<String, (DeveloperImpact, HashSet<String>)> = HashMap::new();
        for repository in repositories {
            for fix in self.verified_bug_fixes(&repository, &query).await? {
                let login = fix.pull_request_author.clone();
                let profile_url = fix.pull_request_author_url.clone();
                let score = fix.fix.score_contribution;
                let impact = developers.entry(login.clone()).or_insert_with(|| {
                    (
                        DeveloperImpact {
                            login,
                            profile_url,
                            impact_score: 0.0,
                            critical_bugs_fixed: 0,
                            high_priority_bugs_fixed: 0,
                            repositories_contributed_to: 0,
                            fixes: Vec::new(),
                        },
                        HashSet::new(),
                    )
                });
                impact.0.impact_score += score;
                if fix.fix.priority.level == Priority::Critical {
                    impact.0.critical_bugs_fixed += 1;
                } else if fix.fix.priority.level == Priority::High {
                    impact.0.high_priority_bugs_fixed += 1;
                }
                impact.1.insert(format!(
                    "{}/{}",
                    fix.fix.repository.owner, fix.fix.repository.name
                ));
                impact.0.fixes.push(fix.fix);
            }
        }

        let mut results: Vec<DeveloperImpact> = developers
            .into_values()
            .map(|(mut impact, repositories)| {
                impact.repositories_contributed_to = repositories.len() as u32;
                impact.fixes.sort_by(|left, right| {
                    right.score_contribution.total_cmp(&left.score_contribution)
                });
                impact
            })
            .collect();
        results.sort_by(|left, right| right.impact_score.total_cmp(&left.impact_score));
        results.truncate(query.limit);
        Ok(results)
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

    async fn verified_bug_fixes(
        &self,
        repository: &Repository,
        query: &DeveloperImpactQuery,
    ) -> Result<Vec<AttributedFix>, SearchError> {
        let response = self
            .client
            .get(format!(
                "{API_ROOT}/repos/{}/{}/issues",
                repository.owner, repository.name
            ))
            .query(&[
                ("state", "closed"),
                ("per_page", "100"),
                ("sort", "updated"),
                ("direction", "desc"),
            ])
            .send()
            .await?
            .error_for_status()?;
        let issues: Vec<GithubIssue> = response.json().await?;
        let mut fixes = Vec::new();

        for issue in issues
            .into_iter()
            .filter(|issue| issue.pull_request.is_none())
        {
            let labels: Vec<String> = issue.labels.into_iter().map(|label| label.name).collect();
            let priority = priority::assess(&labels);
            if !is_bug(&labels) || priority.level.rank() < query.priority_at_least.rank() {
                continue;
            }

            let timeline = self.issue_timeline(repository, issue.number).await?;
            let candidates = timeline.into_iter().filter_map(|event| {
                (event.event == "cross-referenced")
                    .then_some(event.source)
                    .flatten()
                    .filter(|source| source.issue.pull_request.is_some())
                    .map(|source| source.issue.number)
            });
            for pull_number in candidates {
                let pull = self.pull_request(repository, pull_number).await?;
                if pull.merged_at.is_none() || !references_closure(&pull, issue.number) {
                    continue;
                }
                let score =
                    priority_weight(priority.level) + ((repository.stars as f64 + 1.0).ln() * 2.0);
                fixes.push(AttributedFix {
                    pull_request_author: pull.user.login,
                    pull_request_author_url: pull.user.html_url,
                    fix: VerifiedBugFix {
                        repository: repository.clone(),
                        issue_number: issue.number,
                        issue_title: issue.title.clone(),
                        issue_url: issue.html_url.clone(),
                        issue_labels: labels.clone(),
                        priority: priority.clone(),
                        pull_request_number: pull.number,
                        pull_request_url: pull.html_url,
                        merged_at: pull.merged_at.expect("checked merged pull request"),
                        score_contribution: score,
                        evidence: vec![
                            "closed issue carries recognized bug and priority labels".into(),
                            "merged pull request cross-references the issue".into(),
                            "pull request text contains an explicit closing reference".into(),
                        ],
                    },
                });
            }
        }
        Ok(fixes)
    }

    async fn issue_timeline(
        &self,
        repository: &Repository,
        issue_number: u64,
    ) -> Result<Vec<TimelineEvent>, SearchError> {
        let response = self
            .client
            .get(format!(
                "{API_ROOT}/repos/{}/{}/issues/{issue_number}/timeline",
                repository.owner, repository.name
            ))
            .query(&[("per_page", "100")])
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn pull_request(
        &self,
        repository: &Repository,
        pull_number: u64,
    ) -> Result<GithubPullRequest, SearchError> {
        let response = self
            .client
            .get(format!(
                "{API_ROOT}/repos/{}/{}/pulls/{pull_number}",
                repository.owner, repository.name
            ))
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
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

struct AttributedFix {
    pull_request_author: String,
    pull_request_author_url: String,
    fix: VerifiedBugFix,
}

fn is_bug(labels: &[String]) -> bool {
    labels
        .iter()
        .any(|label| matches_label(label, &["bug", "defect", "type: bug", "type:bug"]))
}

fn priority_weight(priority: Priority) -> f64 {
    match priority {
        Priority::Critical => 50.0,
        Priority::High => 25.0,
        Priority::Medium => 10.0,
        Priority::Low => 3.0,
        Priority::Unknown => 0.0,
    }
}

fn references_closure(pull: &GithubPullRequest, issue_number: u64) -> bool {
    let text = format!(
        "{}\n{}",
        pull.title,
        pull.body.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let issue_reference = format!("#{issue_number}");
    ["close", "fix", "resolve"]
        .iter()
        .any(|verb| text.contains(verb) && text.contains(&issue_reference))
}

#[cfg(test)]
mod finder_tests {
    use super::*;

    #[test]
    fn recognizes_explicit_closing_reference() {
        let pull = GithubPullRequest {
            number: 7,
            html_url: String::new(),
            title: "Fixes #42: prevent crash".into(),
            body: None,
            merged_at: Some(String::new()),
            user: crate::github::GithubOwner {
                login: "contributor".into(),
                html_url: String::new(),
            },
        };
        assert!(references_closure(&pull, 42));
        assert!(!references_closure(&pull, 43));
    }
}
