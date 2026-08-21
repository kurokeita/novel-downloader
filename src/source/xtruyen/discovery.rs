//! Reading the chapter index off the site.
//!
//! Chapter addresses cannot be synthesized: a chapter published as an extension
//! of an earlier one carries a suffix beyond that chapter's number, so a numeric
//! range would omit it entirely. Every chapter page instead carries the complete
//! address list of the hundred-chapter window it belongs to, and a link to the
//! chapter that follows it, so the index is walked window by window.

use std::collections::HashSet;

use anyhow::anyhow;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};

use crate::source::{ChapterRef, SourceError, SourceResult};
use crate::utils::clean_text;

use super::parser;

/// Upper bound on windows walked, so a site that links a window back to an
/// earlier one cannot spin forever. At a hundred chapters per window this
/// allows novels far longer than any the site publishes.
const MAX_WINDOWS: usize = 500;

/// The chapter select, whose options carry the window's addresses. The page also
/// renders a volume select whose options are group ranges rather than chapters,
/// which is why this matches the narrower class.
static WINDOW_OPTION: Lazy<Selector> =
    Lazy::new(|| Selector::parse("select.single-chapter-select option[data-redirect]").unwrap());

/// Link to the chapter after the one being viewed.
static NEXT_LINK: Lazy<Selector> = Lazy::new(|| Selector::parse(".nav-next a[href]").unwrap());

/// The chapter's own label, as shown above the prose.
static CHAPTER_LABEL: Lazy<Selector> = Lazy::new(|| Selector::parse(".entry-header h2").unwrap());

/// Fallback label, carried by the reader's floating player.
static MINI_LABEL: Lazy<Selector> =
    Lazy::new(|| Selector::parse("#tts-mini-chapter-title").unwrap());

/// Read the addresses of every chapter in this page's window, in reading order.
/// The page renders the same select once per reading nav, so addresses are
/// deduplicated while their first appearance fixes the order.
pub(super) fn parse_window(page_html: &str) -> Vec<String> {
    let document = Html::parse_document(page_html);
    let mut seen = HashSet::new();
    document
        .select(&WINDOW_OPTION)
        .filter_map(|option| option.value().attr("data-redirect"))
        .filter(|address| !address.is_empty())
        .filter(|address| seen.insert(address.to_string()))
        .map(str::to_string)
        .collect()
}

/// Read the address of the chapter that follows this one, when there is one.
/// The final chapter carries no forward link, which is what ends the walk.
pub(super) fn parse_next_href(page_html: &str) -> Option<String> {
    Html::parse_document(page_html)
        .select(&NEXT_LINK)
        .next()
        .and_then(|link| link.value().attr("href"))
        .filter(|href| !href.is_empty())
        .map(str::to_string)
}

/// Read this chapter's own label, preferring the heading above the prose and
/// falling back to the reader's floating player.
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

/// Walk the whole chapter index, starting from the novel's first chapter and
/// stopping once the novel's latest chapter has been seen.
///
/// A page that cannot be retrieved fails the whole index rather than truncating
/// it, because a short index is indistinguishable from a short novel and would
/// silently produce an incomplete book.
pub(super) async fn walk_index(
    novel_url: &str,
    first_chapter_href: &str,
    latest_chapter_href: Option<&str>,
) -> SourceResult<Vec<ChapterRef>> {
    let latest = latest_chapter_href
        .map(|href| parser::rebase_onto(novel_url, href))
        .transpose()?;

    let mut locators: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor = Some(parser::rebase_onto(novel_url, first_chapter_href)?);

    for _ in 0..MAX_WINDOWS {
        let Some(current) = cursor.take() else { break };
        let page = super::fetch_page(&current).await?;

        let mut fresh = 0;
        for href in parse_window(&page) {
            let locator = parser::rebase_onto(novel_url, &href)?;
            if seen.insert(locator.clone()) {
                locators.push(locator);
                fresh += 1;
            }
        }

        if fresh == 0 {
            // Nothing new means the page carried no usable select, or the walk
            // has come back round to a window it already read. Take the page
            // itself so a lone chapter is still indexed, then stop.
            if seen.insert(current.clone()) {
                locators.push(current);
            }
            break;
        }

        if latest
            .as_ref()
            .is_some_and(|latest| seen.contains(latest.as_str()))
        {
            break;
        }

        let tail = locators
            .last()
            .cloned()
            .ok_or_else(|| SourceError::Other(anyhow!("chapter window came back empty")))?;
        let tail_page = if tail == current {
            page
        } else {
            super::fetch_page(&tail).await?
        };

        cursor = match parse_next_href(&tail_page) {
            Some(href) => {
                let next = parser::rebase_onto(novel_url, &href)?;
                (!seen.contains(&next)).then_some(next)
            }
            None => None,
        };
    }

    if locators.is_empty() {
        return Err(SourceError::Other(anyhow!(
            "no chapters found for {novel_url}"
        )));
    }

    Ok(locators
        .into_iter()
        .enumerate()
        .map(|(index, locator)| ChapterRef {
            number: index as u32 + 1,
            title: None,
            locator,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAPTER_HTML: &str = include_str!("../../../tests/fixtures/xtruyen_chapter.html");
    const SUFFIXED_HTML: &str =
        include_str!("../../../tests/fixtures/xtruyen_chapter_suffixed.html");

    #[test]
    fn parse_window_reads_every_address_once_in_order() {
        assert_eq!(
            parse_window(CHAPTER_HTML),
            vec![
                "https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-1/",
                "https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-2/",
                "https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-3/",
            ],
            "the page renders the select twice, so the addresses must be deduplicated"
        );
    }

    #[test]
    fn parse_window_keeps_extension_chapters_in_place() {
        assert_eq!(
            parse_window(SUFFIXED_HTML),
            vec![
                "https://xtruyen.vn/truyen/truyen-mo-rong/chuong-1/",
                "https://xtruyen.vn/truyen/truyen-mo-rong/chuong-1-1/",
                "https://xtruyen.vn/truyen/truyen-mo-rong/chuong-1-2/",
                "https://xtruyen.vn/truyen/truyen-mo-rong/chuong-2/",
            ],
            "each extension follows the chapter it extends"
        );
    }

    #[test]
    fn parse_window_ignores_the_volume_select() {
        assert!(
            !parse_window(CHAPTER_HTML)
                .iter()
                .any(|address| address.contains("1-to-100")),
            "the volume select lists group ranges, not chapters"
        );
    }

    #[test]
    fn parse_window_returns_empty_for_a_page_with_no_select() {
        assert!(parse_window("<html><body></body></html>").is_empty());
    }

    #[test]
    fn parse_next_href_reads_the_forward_link() {
        assert_eq!(
            parse_next_href(CHAPTER_HTML).as_deref(),
            Some("https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-2/")
        );
    }

    #[test]
    fn parse_next_href_reads_a_forward_link_to_an_extension_chapter() {
        assert_eq!(
            parse_next_href(SUFFIXED_HTML).as_deref(),
            Some("https://xtruyen.vn/truyen/truyen-mo-rong/chuong-1-2/"),
            "a numeric guess would jump over this chapter"
        );
    }

    #[test]
    fn parse_next_href_is_none_on_the_final_chapter() {
        let final_chapter = CHAPTER_HTML.replace("nav-next", "nav-nothing");
        assert_eq!(parse_next_href(&final_chapter), None);
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
