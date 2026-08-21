//! Reading the chapter index off the site.
//!
//! Chapter addresses cannot be synthesized, because a chapter published as an
//! extension of an earlier one carries a suffix beyond that chapter's number
//! and a numeric range would omit it. They cannot be read off a chapter page
//! either: the chapter select there returns an arbitrary hundred-chapter slice
//! that need not contain the chapter being viewed, so it enumerates nothing.
//!
//! The index instead comes from the endpoint the site's own reader uses, paged
//! by chapter position. Positions are a contiguous `1..N` run, so the novel's
//! own chapter groups are not needed to drive the paging.

use std::collections::HashSet;

use anyhow::anyhow;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};

use crate::source::{ChapterRef, RatePolicy, SourceError, SourceResult};
use crate::utils::clean_text;

use super::{api, parser};

/// Chapter positions requested per call.
///
/// Measured against the live endpoint on 2026-08-21: a width of 400 returns 400
/// entries and 401 returns 401, but 500 collapses to 201, so the server honors
/// a few hundred and silently clamps beyond that. 400 sits inside the honored
/// range with margin.
const PAGE_SIZE: usize = 400;

/// Upper bound on pages read, so a site that never returns a short page cannot
/// spin forever. At [`PAGE_SIZE`] per page this allows 200,000 chapters, far
/// beyond anything the site publishes.
const MAX_PAGES: usize = 500;

/// The chapter's own label, as shown above the prose.
static CHAPTER_LABEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".entry-header h2").unwrap());

/// Fallback label, carried by the reader's floating player.
static MINI_LABEL: Lazy<Selector> =
    Lazy::new(|| Selector::parse("#tts-mini-chapter-title").unwrap());

/// Read this chapter's own label, preferring the heading above the prose and
/// falling back to the reader's floating player. Only used when the index did
/// not supply a title.
pub(super) fn parse_chapter_title(page_html: &str) -> Option<String> {
    let document = Html::parse_document(page_html);
    document
        .select(&CHAPTER_LABEL)
        .map(|heading| clean_text(&heading.text().collect::<String>()))
        .find(|label| !label.is_empty())
        .or_else(|| {
            document
                .select(&MINI_LABEL)
                .next()
                .map(|label| clean_text(&label.text().collect::<String>()))
                .filter(|label| !label.is_empty())
        })
}

/// Read the whole chapter index, one page of positions at a time.
///
/// Paging stops at the first page shorter than [`PAGE_SIZE`], which the
/// endpoint returns for the tail; past the end it answers an empty list, so a
/// novel whose length is an exact multiple of the page size costs one extra
/// call rather than looping.
///
/// A page that cannot be retrieved fails the whole index rather than truncating
/// it, because a short index is indistinguishable from a short novel and would
/// silently produce an incomplete book.
pub(super) async fn fetch_index(
    novel_url: &str,
    manga_id: &str,
    policy: RatePolicy,
) -> SourceResult<Vec<ChapterRef>> {
    let endpoint = format!(
        "{}{}",
        parser::origin_of(novel_url)?,
        api::CHAPTERS_ENDPOINT
    );
    let mut chapters: Vec<ChapterRef> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut from = 1usize;

    for page in 0..MAX_PAGES {
        if page > 0 {
            tokio::time::sleep(policy.min_delay).await;
        }

        let to = from + PAGE_SIZE - 1;
        let body = api::group_form_body(manga_id, &from.to_string(), &to.to_string());
        let json = super::post_form_retrying(&endpoint, &body, novel_url, policy).await?;
        let entries: Vec<api::ChapterEntry> = serde_json::from_str(&json).map_err(|e| {
            SourceError::Other(anyhow!(
                "unexpected chapter payload for positions {from}-{to}: {e}"
            ))
        })?;

        let received = entries.len();
        for entry in entries {
            let locator = parser::chapter_url(novel_url, &entry.slug)?;
            if seen.insert(locator.clone()) {
                chapters.push(ChapterRef {
                    number: chapters.len() as u32 + 1,
                    title: Some(entry.display_title()),
                    locator,
                });
            }
        }

        if received < PAGE_SIZE {
            break;
        }
        from += PAGE_SIZE;
    }

    if chapters.is_empty() {
        return Err(SourceError::Other(anyhow!(
            "no chapters found for {novel_url}"
        )));
    }
    Ok(chapters)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAPTER_HTML: &str = include_str!("../../../tests/fixtures/xtruyen_chapter.html");

    #[test]
    fn parse_chapter_title_reads_the_label() {
        assert_eq!(
            parse_chapter_title(CHAPTER_HTML).as_deref(),
            Some("Chương 1: Nhan đề thử nghiệm một")
        );
    }

    #[test]
    fn parse_chapter_title_falls_back_to_the_players_label() {
        let without_heading =
            CHAPTER_HTML.replace("<h2>Chương 1: Nhan đề thử nghiệm một</h2>", "<h2></h2>");
        assert_eq!(
            parse_chapter_title(&without_heading).as_deref(),
            Some("Chương 1")
        );
    }

    #[test]
    fn parse_chapter_title_is_none_when_the_page_carries_no_label() {
        assert_eq!(parse_chapter_title("<html><body></body></html>"), None);
    }
}
