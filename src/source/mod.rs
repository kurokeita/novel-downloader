//! The source seam: everything the pipeline knows about a novel site.
//!
//! [`SiteAdapter`] is the one trait every site implements. The core pipeline
//! addresses chapters through opaque [`ChapterRef`] locators and never
//! constructs a URL itself, so adding a site touches only its own module and
//! the [`registry`].

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

pub mod khodocsach;
pub mod metruyenhot;
pub mod registry;

/// One chapter in a novel's index.
///
/// `locator` is whatever the owning source needs to fetch the chapter: a
/// full URL for metruyenhot, an opaque id elsewhere. Nothing outside the
/// source that produced it may interpret it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterRef {
    /// One-based position in the index. Drives the `chapter_NNNN.html`
    /// output file name and every progress event.
    pub number: u32,
    /// Chapter title when the index already carries it. metruyenhot learns
    /// the title only on fetch, so its refs leave this `None`.
    pub title: Option<String>,
    /// Source-owned address used to fetch this chapter.
    pub locator: String,
}

/// A novel's metadata plus its complete chapter index, as returned by
/// [`SiteAdapter::fetch_novel`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Novel {
    /// Display title, already cleaned of any site-specific suffix.
    pub title: String,
    /// Author name when the source publishes one.
    pub author: Option<String>,
    /// Short synopsis when the source publishes one.
    pub description: Option<String>,
    /// Publication status (e.g. "Đang ra", "Hoàn thành").
    pub status: Option<String>,
    /// Absolute cover image URL when the source publishes one.
    pub cover_url: Option<String>,
    /// Every chapter, in reading order, numbered from 1.
    pub chapters: Vec<ChapterRef>,
}

/// A fetched chapter's text, before it is written to disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterContent {
    /// Title of the parent novel (e.g. "Người Chồng Vô Dụng").
    pub novel_title: String,
    /// Title of this chapter (e.g. "Chương 12: ...").
    pub chapter_title: String,
    /// Ordered, deduplicated paragraphs of the chapter body.
    pub paragraphs: Vec<String>,
}

/// How hard the pipeline may push a source. Owned by the source rather than
/// asked of the user, who has no way to know a site's limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RatePolicy {
    /// Upper bound on concurrent chapter fetches. The runner clamps the
    /// user's `--workers` to this.
    pub max_concurrency: usize,
    /// Minimum spacing between requests.
    pub min_delay: Duration,
    /// How many times a rate-limited chapter is retried before it fails.
    pub max_retries: u32,
    /// First backoff step; later steps grow from it.
    pub backoff_base: Duration,
}

/// Failures a source reports in a form the pipeline can act on. Everything
/// else stays an opaque [`anyhow::Error`] behind [`SourceError::Other`].
#[derive(Debug, Error)]
pub enum SourceError {
    /// The site refused the request rate. The runner backs off the whole
    /// run rather than the single chapter, since limiters are per-client.
    #[error("rate limited by {source_name}: {message}")]
    RateLimited {
        /// Adapter that hit the limit, for the user-facing message.
        source_name: &'static str,
        /// Server-supplied detail, when there is any.
        message: String,
    },
    /// The novel or chapter does not exist at the given address.
    #[error("not found: {0}")]
    NotFound(String),
    /// The site rejected the client itself (missing User-Agent, blocked
    /// edge), not the request rate.
    #[error("request rejected by the site: {0}")]
    ClientRejected(String),
    /// The chapter exists but this client is not entitled to read it. The
    /// run continues; only this chapter is reported.
    #[error("chapter not available to this client: {0}")]
    Unentitled(String),
    /// Anything else, unchanged.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Result alias for adapter methods.
pub type SourceResult<T> = std::result::Result<T, SourceError>;

/// One novel site. Object-safe on purpose: the registry hands out
/// `&'static dyn SiteAdapter`, so no generic methods and no RPITIT.
#[async_trait]
pub trait SiteAdapter: Send + Sync {
    /// Stable machine id (`metruyenhot`), used in logs and errors.
    fn id(&self) -> &'static str;

    /// Human-readable name shown on the wizard summary screen.
    fn display_name(&self) -> &'static str;

    /// Hosts this adapter claims, lower-case and without `www.`.
    fn hosts(&self) -> &'static [&'static str];

    /// Request pacing this source tolerates.
    fn rate_policy(&self) -> RatePolicy;

    /// Resolve a novel main-page URL into metadata plus the full chapter
    /// index. The index is built once, upfront: a `--start`/`--end` range is
    /// a slice of it, never a computed sequence of URLs.
    async fn fetch_novel(&self, url: &str) -> SourceResult<Novel>;

    /// Resolve a novel main-page URL into metadata alone, leaving
    /// [`Novel::chapters`] empty. Required rather than defaulted to
    /// [`SiteAdapter::fetch_novel`] on purpose: `--epub-only` packages a
    /// directory that already exists and must not fail when the chapter index
    /// is unreachable, so every adapter has to answer for the cheap path.
    async fn fetch_metadata(&self, url: &str) -> SourceResult<Novel>;

    /// Fetch and parse one chapter named by a ref this adapter produced.
    async fn fetch_chapter(&self, chapter: &ChapterRef) -> SourceResult<ChapterContent>;
}
