# issueharvest

`issueharvest` is an independent Rust library for discovering and ranking public,
open GitHub issues across active open-source repositories. It uses GitHub's REST
API directly; it is not built on `octocrab`.

```rust,no_run
use issueharvest::{Finder, IssueQuery, Priority};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
// Reads `GITHUB_TOKEN`; do not hard-code credentials in source code.
let finder = Finder::from_env()?;
// Kotlin repository discovery defaults to ascending trend-score order.
let repositories = finder.trending_repositories(
    issueharvest::TrendingRepositoryQuery::new("Kotlin")
        .min_stars(500)
        .pushed_after("2026-06-01")
        .limit(25),
).await?;

let issues = finder.search(
    IssueQuery::new("Kotlin")
        .min_stars(500)
        .priority_at_least(Priority::High)
        .prefer_help_wanted(true)
        .repositories_to_scan(20)
        .limit(10),
).await?;
# Ok(()) }
```

Priority is derived from explicit labels such as `P0`, `blocker`, `priority: high`,
and `P2`. Each result retains its original labels and the classification reasons.

## Current scope

- Discovers public repositories by language, stars, recent activity, and archive status.
- Finds trending repositories with ascending or descending trend-score ordering.
- Retrieves open issues directly from GitHub, excluding pull requests.
- Uses bounded concurrency for repository scans.
- Normalizes common priority and contributor-friendly labels.
- Ranks results by priority, maintainer invitation, comment activity, and stars.

Version 0.1 retrieves the most recently updated 100 open issues per selected
repository. Full pagination, response caching, and GitHub rate-limit backoff are
planned extensions rather than silently incomplete behavior.

## Trending definition

GitHub does not offer a public Trending API. `trending_repositories` first asks
GitHub for eligible repositories ordered by most recently updated, then applies
a transparent score based on that activity position, stars, and forks. The
default `TrendOrder::Ascending` returns the lowest qualifying score first;
choose `TrendOrder::Descending` to return the strongest matches first.
