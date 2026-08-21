//! The xtruyen.vn adapter.
//!
//! A WordPress site on the Madara theme. Novel pages and chapter pages are both
//! served as HTML, but the chapter prose is not: it travels in the chapter page
//! as an encoded string that the site's own script decodes in the browser. See
//! [`payload`] for the recovery, [`discovery`] for why the chapter index has to
//! be walked rather than synthesized, and [`Xtruyen::rate_policy`] for the
//! measured request limit.

use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};

use crate::source::{
    ChapterContent, ChapterRef, Novel, RatePolicy, SiteAdapter, SourceError, SourceResult,
};
use crate::utils::{clean_text, http_client};

mod discovery;
mod metadata;
mod parser;
mod payload;

/// Hosts this adapter claims.
const HOSTS: &[&str] = &["xtruyen.vn"];

/// Stable machine id.
const ID: &str = "xtruyen";

/// Name shown to the user.
const DISPLAY_NAME: &str = "xtruyen";

/// Per-request timeout. Chapter pages run to roughly 180 KB, so this is
/// generous rather than tight.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The novel title as written on a chapter page. The visible heading is
/// upper-cased in the markup, so the anchor's title attribute is the only place
/// the chapter page carries the title in its published casing.
static NOVEL_TITLE_ON_CHAPTER: Lazy<Selector> =
    Lazy::new(|| Selector::parse("#chapter-heading a[title]").unwrap());

/// The xtruyen.vn adapter.
pub struct Xtruyen;

/// GET `url` and return the page body, mapping the statuses observed against
/// the live host into the pipeline's vocabulary. Anything else stays opaque
/// rather than being guessed at.
async fn fetch_page(url: &str) -> SourceResult<String> {
    let client = http_client(REQUEST_TIMEOUT, None).map_err(SourceError::Other)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| SourceError::Other(anyhow!("failed to fetch {url}: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            429 => SourceError::RateLimited {
                source_name: ID,
                message: format!("HTTP 429 from {url}"),
            },
            403 => SourceError::ClientRejected(format!("HTTP 403 from {url}")),
            404 => SourceError::NotFound(url.to_string()),
            code => SourceError::Other(anyhow!("HTTP {code} from {url}")),
        });
    }

    response
        .text()
        .await
        .map_err(|e| SourceError::Other(anyhow!("failed to read body from {url}: {e}")))
}

/// Read the novel's title off a chapter page, for the output directory name.
fn novel_title_on_chapter(page_html: &str) -> Option<String> {
    Html::parse_document(page_html)
        .select(&NOVEL_TITLE_ON_CHAPTER)
        .next()
        .and_then(|anchor| anchor.value().attr("title"))
        .map(clean_text)
        .filter(|title| !title.is_empty())
}

/// Fetch and parse the novel page, which is the cheap path both metadata
/// methods share.
async fn fetch_novel_page(url: &str) -> SourceResult<metadata::NovelPage> {
    parser::novel_slug_from_url(url)?;
    let html = fetch_page(url).await?;
    metadata::parse_novel_page(&html).map_err(SourceError::Other)
}

#[async_trait]
impl SiteAdapter for Xtruyen {
    /// Stable machine id.
    fn id(&self) -> &'static str {
        ID
    }

    /// Name shown to the user.
    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    /// Hosts this adapter claims.
    fn hosts(&self) -> &'static [&'static str] {
        HOSTS
    }

    /// Request pacing, measured against the live host on 2026-08-20.
    ///
    /// Sequential requests at 2 per second ran 50 for 50 clean. At 4 per second
    /// the site began answering `429` between the sixteenth and nineteenth
    /// request, and eight concurrent workers were refused almost immediately.
    /// Refusals clear in under a second and do not extend themselves, so a
    /// short backoff recovers rather than compounding.
    ///
    /// Rotating the `User-Agent` per request, which is what lets the khodocsach
    /// policy stay unconstrained, does **not** move this limit: a fixed header
    /// managed 18 requests before `429` and a rotating one 17, at the same rate.
    /// The site's edge buckets by client address, so no header will widen it and
    /// these numbers are a real ceiling rather than a starting point.
    ///
    /// What would invalidate them is a change in how that edge buckets clients.
    /// Retest the same way: a fixed-rate sequential run at 2 and at 4 requests
    /// per second, then a small parallel burst.
    fn rate_policy(&self) -> RatePolicy {
        RatePolicy {
            max_concurrency: 2,
            min_delay: Duration::from_millis(500),
            max_retries: 3,
            backoff_base: Duration::from_secs(2),
        }
    }

    /// Resolve a novel URL into metadata plus the walked chapter index.
    async fn fetch_novel(&self, url: &str) -> SourceResult<Novel> {
        let page = fetch_novel_page(url).await?;
        let first = page
            .first_chapter_href
            .as_deref()
            .ok_or_else(|| SourceError::Other(anyhow!("novel page links to no first chapter")))?;
        let chapters =
            discovery::walk_index(url, first, page.latest_chapter_href.as_deref()).await?;

        Ok(Novel {
            title: page.title,
            author: page.author,
            description: page.description,
            status: page.status,
            cover_url: page.cover_url,
            chapters,
        })
    }

    /// Resolve a novel URL into metadata alone, without walking the index.
    async fn fetch_metadata(&self, url: &str) -> SourceResult<Novel> {
        let page = fetch_novel_page(url).await?;
        Ok(Novel {
            title: page.title,
            author: page.author,
            description: page.description,
            status: page.status,
            cover_url: page.cover_url,
            chapters: Vec::new(),
        })
    }

    /// Fetch one chapter and recover its prose from the page's encoded payload.
    async fn fetch_chapter(&self, chapter: &ChapterRef) -> SourceResult<ChapterContent> {
        let html = fetch_page(&chapter.locator).await?;
        let paragraphs = payload::chapter_paragraphs(&html).map_err(SourceError::Other)?;
        if paragraphs.is_empty() {
            return Err(SourceError::Other(anyhow!(
                "chapter {} at {} decoded to no text",
                chapter.number,
                chapter.locator
            )));
        }

        Ok(ChapterContent {
            novel_title: novel_title_on_chapter(&html).unwrap_or_default(),
            chapter_title: chapter
                .title
                .clone()
                .or_else(|| discovery::parse_chapter_title(&html))
                .unwrap_or_else(|| format!("Chương {}", chapter.number)),
            paragraphs,
        })
    }
}
