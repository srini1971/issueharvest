use crate::types::Repository;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct RepositorySearchResponse {
    pub items: Vec<GithubRepository>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubRepository {
    pub owner: GithubOwner,
    pub name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub pushed_at: Option<String>,
}

impl From<GithubRepository> for Repository {
    fn from(value: GithubRepository) -> Self {
        Self {
            owner: value.owner.login,
            name: value.name,
            html_url: value.html_url,
            description: value.description,
            stars: value.stargazers_count,
            forks: value.forks_count,
            pushed_at: value.pushed_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubOwner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubIssue {
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub labels: Vec<GithubLabel>,
    pub comments: u64,
    pub created_at: String,
    pub updated_at: String,
    // GitHub's list-issues endpoint also returns pull requests. This field is the
    // reliable discriminator; title or labels must never be used to guess.
    pub pull_request: Option<serde::de::IgnoredAny>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubLabel {
    pub name: String,
}
