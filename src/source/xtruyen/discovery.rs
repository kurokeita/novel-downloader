//! Reading the chapter index off the site.
//!
//! Chapter addresses cannot be synthesized, because a chapter published as an
//! extension of an earlier one carries a suffix beyond that chapter's number
//! and a numeric range would omit it. They cannot be read off a chapter page
//! either: the chapter select there returns an arbitrary hundred-chapter slice
//! that need not contain the chapter being viewed, so it enumerates nothing.
//!
//! The index therefore comes from the site's own chapter list. The novel page
//! publishes an accordion of groups whose `data-value` holds position bounds,
//! and [`api`] turns one such pair into that group's chapters. A novel of 3600
//! chapters costs 36 requests.

use std::collections::HashSet;

use anyhow::anyhow;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};

use crate::source::{ChapterRef, SourceError, SourceResult};
use crate::utils::clean_text;

use super::{api, parser};

/// Path of the endpoint listing a novel's chapter groups, relative to the
/// novel page. It needs no authentication, unlike the group contents.
const GROUPS_PATH: &str = "ajax/chapters/";

/// One collapsed chapter group. Its `data-value` is the position range the
/// group covers, which is the key [`api`] wants.
static GROUP_ITEM: Lazy<Selector> =
    Lazy::new(|| Selector::parse("li.has-child[data-value]").unwrap());

/// The chapter's own label, as shown above the prose.
static CHAPTER_LABEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".entry-header h2").unwrap());

/// Fallback label, carried by the reader's floating player.
static MINI_LABEL: Lazy<Selector> =
    Lazy::new(|| Selector::parse("#tts-mini-chapter-title").unwrap());

/// Read every chapter group's position bounds, in reading order. A value that
/// is not a range is skipped rather than guessed at.
pub(super) fn parse_group_bounds(groups_html: &str) -> Vec<(String, String)> {
    let document = Html::parse_document(groups_html);
    document
        .select(&GROUP_ITEM)
        .filter_map(|item| item.value().attr("data-value"))
        .filter_map(|value| value.split_once("-to-"))
        .filter(|(from, to)| !from.is_empty() && !to.is_empty())
        .map(|(from, to)| (from.to_string(), to.to_string()))
        .collect()
}

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

/// Read the whole chapter index: the group list, then each group's chapters.
///
/// A group that cannot be retrieved fails the whole index rather than
/// truncating it, because a short index is indistinguishable from a short novel
/// and would silently produce an incomplete book.
pub(super) async fn fetch_index(novel_url: &str, manga_id: &str) -> SourceResult<Vec<ChapterRef>> {
    let groups_url = parser::join_path(novel_url, GROUPS_PATH)?;
    let groups_html = super::post_form(&groups_url, "", novel_url).await?;
    let bounds = parse_group_bounds(&groups_html);
    if bounds.is_empty() {
        return Err(SourceError::Other(anyhow!(
            "no chapter groups listed for {novel_url}"
        )));
    }

    let endpoint = format!(
        "{}{}",
        parser::origin_of(novel_url)?,
        api::CHAPTERS_ENDPOINT
    );
    let mut chapters: Vec<ChapterRef> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (from, to) in bounds {
        let body = api::group_form_body(manga_id, &from, &to);
        let json = super::post_form(&endpoint, &body, novel_url).await?;
        let entries: Vec<api::ChapterEntry> = serde_json::from_str(&json).map_err(|e| {
            SourceError::Other(anyhow!(
                "unexpected chapter group payload for {from}-to-{to}: {e}"
            ))
        })?;

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

    const GROUPS_HTML: &str = include_str!("../../../tests/fixtures/xtruyen_chapter_groups.html");
    const CHAPTER_HTML: &str = include_str!("../../../tests/fixtures/xtruyen_chapter.html");

    #[test]
    fn parse_group_bounds_reads_every_group_in_order() {
        assert_eq!(
            parse_group_bounds(GROUPS_HTML),
            vec![
                ("1".to_string(), "2".to_string()),
                ("3".to_string(), "3m".to_string()),
            ],
            "bounds are positions, and the final group's upper bound keeps its suffix"
        );
    }

    #[test]
    fn parse_group_bounds_returns_empty_when_no_groups_are_listed() {
        assert!(parse_group_bounds("<div></div>").is_empty());
    }

    #[test]
    fn parse_group_bounds_ignores_a_value_that_is_not_a_range() {
        assert!(
            parse_group_bounds(r#"<li class="has-child" data-value="rubbish"></li>"#).is_empty()
        );
    }

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
