//! Novel-page extractors. One fetch of the novel page yields everything the
//! EPUB needs plus the two chapter addresses the index walk starts from.

use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
use scraper::{ElementRef, Html, Selector};

use crate::utils::clean_text;

/// The metadata tag carrying the title in its published casing. The visible
/// heading is upper-cased in the markup itself rather than by styling, so
/// reading the heading would put a shouting title into every EPUB.
static OG_TITLE: Lazy<Selector> =
    Lazy::new(|| Selector::parse(r#"meta[property="og:title"]"#).unwrap());

/// Fallback title, used when the page carries no metadata tag.
static POST_TITLE: Lazy<Selector> = Lazy::new(|| Selector::parse(".post-title").unwrap());

/// Author block.
static AUTHOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".author-content").unwrap());

/// Synopsis block.
static DESCRIPTION: Lazy<Selector> = Lazy::new(|| Selector::parse(".summary__content").unwrap());

/// Cover image.
static COVER: Lazy<Selector> = Lazy::new(|| Selector::parse(".summary_image img").unwrap());

/// One labeled metadata row. The page states its rows as a heading and a value
/// rather than as classed fields, so the value is found by its label.
static CONTENT_ITEM: Lazy<Selector> = Lazy::new(|| Selector::parse(".post-content_item").unwrap());

/// The label half of a metadata row.
static ROW_HEADING: Lazy<Selector> = Lazy::new(|| Selector::parse(".summary-heading").unwrap());

/// The value half of a metadata row.
static ROW_VALUE: Lazy<Selector> = Lazy::new(|| Selector::parse(".summary-content").unwrap());

/// First and last chapter buttons, in that order.
static INIT_LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("#init-links a").unwrap());

/// Latest chapter link, a second place the newest chapter is published.
static LATEST_CHAPTER: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".summary-content-chapter a").unwrap());

/// Label of the row holding the publication status.
const STATUS_LABEL: &str = "Trạng thái";

/// Flatten an element's text into one cleaned line.
fn text_of(element: ElementRef<'_>) -> String {
    clean_text(&element.text().collect::<String>())
}

/// Read the value of the metadata row whose label contains `label`.
fn labeled_row(document: &Html, label: &str) -> Option<String> {
    document.select(&CONTENT_ITEM).find_map(|item| {
        let heading = item.select(&ROW_HEADING).next()?;
        if !text_of(heading).contains(label) {
            return None;
        }
        let value = text_of(item.select(&ROW_VALUE).next()?);
        (!value.is_empty()).then_some(value)
    })
}

/// Everything the novel page carries, as read in one pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct NovelPage {
    /// Display title.
    pub(super) title: String,
    /// Author name when the page names one.
    pub(super) author: Option<String>,
    /// Synopsis when the page carries one.
    pub(super) description: Option<String>,
    /// Publication status when the page states one.
    pub(super) status: Option<String>,
    /// Cover image address when the page advertises one.
    pub(super) cover_url: Option<String>,
    /// Address of the novel's first chapter, as written on the page.
    pub(super) first_chapter_href: Option<String>,
    /// Address of the novel's latest chapter, as written on the page.
    pub(super) latest_chapter_href: Option<String>,
}

/// Read every field the novel page carries. Only the title is required: it
/// names the output directory and the EPUB, so a page without one is not a
/// novel page. Every other field degrades to `None`.
pub(super) fn parse_novel_page(html: &str) -> Result<NovelPage> {
    let document = Html::parse_document(html);

    let title = document
        .select(&OG_TITLE)
        .next()
        .and_then(|tag| tag.value().attr("content"))
        .map(clean_text)
        .or_else(|| document.select(&POST_TITLE).next().map(text_of))
        .filter(|title| !title.is_empty())
        .ok_or_else(|| anyhow!("page names no novel: no title found"))?;

    let mut init_links = document
        .select(&INIT_LINKS)
        .filter_map(|link| link.value().attr("href"))
        .map(str::to_string);
    let first_chapter_href = init_links.next();
    let latest_chapter_href = init_links.next().or_else(|| {
        document
            .select(&LATEST_CHAPTER)
            .next()
            .and_then(|link| link.value().attr("href"))
            .map(str::to_string)
    });

    Ok(NovelPage {
        title,
        author: document
            .select(&AUTHOR)
            .next()
            .map(text_of)
            .filter(|author| !author.is_empty()),
        description: document
            .select(&DESCRIPTION)
            .next()
            .map(text_of)
            .filter(|description| !description.is_empty()),
        status: labeled_row(&document, STATUS_LABEL),
        cover_url: document
            .select(&COVER)
            .next()
            .and_then(|image| image.value().attr("src"))
            .map(str::to_string),
        first_chapter_href,
        latest_chapter_href,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOVEL_HTML: &str = include_str!("../../../tests/fixtures/xtruyen_novel.html");

    #[test]
    fn parse_novel_page_reads_the_title_in_its_published_casing() {
        let page = parse_novel_page(NOVEL_HTML).unwrap();
        assert_eq!(
            page.title, "Truyện Thử Nghiệm",
            "the heading is upper-cased in the markup, the metadata tag is not"
        );
    }

    #[test]
    fn parse_novel_page_reads_the_author() {
        assert_eq!(
            parse_novel_page(NOVEL_HTML).unwrap().author,
            Some("Tác Giả Thử Nghiệm".to_string())
        );
    }

    #[test]
    fn parse_novel_page_reads_the_status() {
        assert_eq!(
            parse_novel_page(NOVEL_HTML).unwrap().status,
            Some("Đang ra".to_string())
        );
    }

    #[test]
    fn parse_novel_page_reads_the_description() {
        assert_eq!(
            parse_novel_page(NOVEL_HTML).unwrap().description,
            Some(
                "Phần giới thiệu thử nghiệm cho truyện thử nghiệm, gồm một câu duy nhất."
                    .to_string()
            )
        );
    }

    #[test]
    fn parse_novel_page_reads_the_cover_address() {
        assert_eq!(
            parse_novel_page(NOVEL_HTML).unwrap().cover_url,
            Some("https://img.xtruyen.vn/truyen-thu-nghiem.webp".to_string())
        );
    }

    #[test]
    fn parse_novel_page_reads_both_chapter_addresses() {
        let page = parse_novel_page(NOVEL_HTML).unwrap();
        assert_eq!(
            page.first_chapter_href,
            Some("https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-1/".to_string())
        );
        assert_eq!(
            page.latest_chapter_href,
            Some("https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-3/".to_string())
        );
    }

    #[test]
    fn parse_novel_page_leaves_absent_fields_empty() {
        let stripped = NOVEL_HTML
            .replace("author-content", "removed-content")
            .replace("summary_image", "removed_image");
        let page = parse_novel_page(&stripped).unwrap();
        assert_eq!(page.author, None);
        assert_eq!(page.cover_url, None);
        assert_eq!(
            page.title, "Truyện Thử Nghiệm",
            "losing optional fields must not lose the title"
        );
    }

    #[test]
    fn parse_novel_page_errors_when_the_page_names_no_novel() {
        assert!(parse_novel_page("<html><body>not a novel page</body></html>").is_err());
    }
}
