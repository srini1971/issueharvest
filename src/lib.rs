//! `issueharvest` discovers and ranks public, open GitHub issues.
//!
//! It is intentionally a focused discovery library, not a general-purpose GitHub SDK.
//! The crate talks to GitHub's REST API directly and has no dependency on `octocrab`.

mod finder;
mod github;
mod priority;
mod types;

pub use finder::{Finder, FinderBuilder};
pub use priority::{Confidence, Priority, PriorityAssessment};
pub use types::{
    DeveloperImpact, DeveloperImpactQuery, IssueMatch, IssueQuery, Repository, SearchError,
    TrendOrder, TrendingRepository, TrendingRepositoryQuery, VerifiedBugFix,
};
