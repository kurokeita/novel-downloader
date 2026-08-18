//! The metruyenhot adapter: `metruyenhotne.com` and `metruyenhotvn.com`.
//!
//! Both hosts serve the same backend template, so one adapter covers them.
//! Chapters live at `<novel>/chuong-<N>/`, and the site publishes no chapter
//! index, so [`SiteAdapter::fetch_novel`] discovers the highest chapter
//! number and synthesizes refs `1..=N` from it.

mod discovery;
mod metadata;
mod parser;

use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;

use crate::source::{ChapterContent, ChapterRef, Novel, RatePolicy, SiteAdapter, SourceResult};
use crate::utils::fetch_html;

pub use discovery::{
    discover_last_chapter_number_from_html, discover_last_chapter_number_from_main_page,
    find_last_page_url, max_chapter_in_html,
};
/// Main-page extractors, public only until PR 5 moves `Novel` metadata into
/// `BuildEpubParams` and the EPUB layer stops reading HTML.
pub use metadata::{
    extract_author_from_main_page, extract_cover_image_url,
    extract_novel_description_from_main_page, extract_novel_status_from_main_page,
    extract_novel_title_from_main_page,
};
pub use parser::extract_full_chapter_text;

/// Hosts this adapter claims, alphabetical so error messages are stable.
const HOSTS: &[&str] = &["metruyenhotne.com", "metruyenhotvn.com"];

/// The metruyenhot adapter. Stateless: one shared instance serves the whole
/// run, so the registry can hand out a `&'static` reference.
pub struct Metruyenhot;

/// Normalize a novel URL to its main-page form (exactly one trailing slash),
/// which every relative link on the page resolves against.
fn main_page_url(url: &str) -> String {
    format!("{}/", url.trim_end_matches('/'))
}

/// Build the canonical chapter URL for a main-page URL and chapter number,
/// e.g. `https://metruyenhotvn.com/foo/` + `7` ->
/// `https://metruyenhotvn.com/foo/chuong-7/`.
fn chapter_url(main_url: &str, number: u32) -> String {
    format!("{main_url}chuong-{number}/")
}

/// Run the five main-page extractors over an already-fetched page, leaving
/// the chapter index empty for the caller to fill in.
fn metadata_from_main_page(main_url: &str, html: &str) -> Novel {
    Novel {
        title: metadata::extract_novel_title_from_main_page(html),
        author: metadata::extract_author_from_main_page(html),
        description: metadata::extract_novel_description_from_main_page(html),
        status: metadata::extract_novel_status_from_main_page(html),
        cover_url: metadata::extract_cover_image_url(main_url, html),
        chapters: Vec::new(),
    }
}

#[async_trait]
impl SiteAdapter for Metruyenhot {
    /// Stable machine id used in logs and errors.
    fn id(&self) -> &'static str {
        "metruyenhot"
    }

    /// Name shown on the wizard summary screen.
    fn display_name(&self) -> &'static str {
        "metruyenhot"
    }

    /// The two hosts sharing this template.
    fn hosts(&self) -> &'static [&'static str] {
        HOSTS
    }

    /// Permissive by construction: the site imposes no limit this crawler
    /// has ever hit, so the clamp is a no-op for existing users.
    fn rate_policy(&self) -> RatePolicy {
        RatePolicy {
            max_concurrency: usize::MAX,
            min_delay: Duration::ZERO,
            max_retries: 0,
            backoff_base: Duration::ZERO,
        }
    }

    /// Fetch the main page once for metadata, follow the chapter-list
    /// pagination for the highest chapter number, then synthesize the index
    /// `1..=N` from the URLs the site serves chapters at.
    async fn fetch_novel(&self, url: &str) -> SourceResult<Novel> {
        let main_url = main_page_url(url);
        let html = fetch_html(&main_url).await?;
        let last = discovery::discover_last_chapter_number_from_main_page(&html, &main_url).await?;
        let mut novel = metadata_from_main_page(&main_url, &html);
        novel.chapters = (1..=last)
            .map(|number| ChapterRef {
                number,
                title: None,
                locator: chapter_url(&main_url, number),
            })
            .collect();
        Ok(novel)
    }

    /// One main-page fetch and the five extractors, with no pagination walk.
    async fn fetch_metadata(&self, url: &str) -> SourceResult<Novel> {
        let main_url = main_page_url(url);
        let html = fetch_html(&main_url).await?;
        Ok(metadata_from_main_page(&main_url, &html))
    }

    /// Fetch and parse one chapter page. An empty body fails here rather
    /// than reaching disk as an empty chapter file.
    async fn fetch_chapter(&self, chapter: &ChapterRef) -> SourceResult<ChapterContent> {
        let url = &chapter.locator;
        let html = fetch_html(url).await?;
        let content = parser::extract_full_chapter_text(&html)?;
        if content.paragraphs.is_empty() {
            return Err(anyhow!("No chapter content extracted from {url}").into());
        }
        Ok(content)
    }
}
