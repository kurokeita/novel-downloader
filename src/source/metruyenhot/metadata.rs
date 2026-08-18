//! metruyenhot main-page extractors. Private to this adapter: the pipeline
//! reads novel metadata from [`crate::source::Novel`], never from HTML.

use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use crate::utils::clean_text;

/// Pre-compiled regex matching the trailing " - truyenazz" suffix on novel
/// page titles.
static TRUYENAZZ_SUFFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\s*-\s*truyenazz\s*$").unwrap());

/// Pre-compiled regex pulling the author name from the page body text.
static AUTHOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)Tác giả:\s*([^\n\r]+)").unwrap());

/// Extract the novel title from the main page HTML, preferring the first
/// `<h1>` then a cleaned-up `<title>` tag, defaulting to "Unknown Novel".
pub(super) fn extract_novel_title_from_main_page(html_source: &str) -> String {
    let doc = Html::parse_document(html_source);

    let h1 = Selector::parse("h1").unwrap();
    if let Some(elem) = doc.select(&h1).next() {
        let text = clean_text(&elem.text().collect::<String>());
        if !text.is_empty() {
            return text;
        }
    }

    let title = Selector::parse("title").unwrap();
    if let Some(elem) = doc.select(&title).next() {
        let text = clean_text(&elem.text().collect::<String>());
        if !text.is_empty() {
            return TRUYENAZZ_SUFFIX.replace(&text, "").to_string();
        }
    }
    "Unknown Novel".to_string()
}

/// Try to find the author name in the body text, returning None when the
/// "Tác giả:" prefix is absent. Trims any trailing genre text.
/// Extract the publication status (e.g. "Đang ra", "Hoàn thành") from the
/// main page. Looks for `div.content1 div.info p span.status`.
pub(super) fn extract_novel_status_from_main_page(html_source: &str) -> Option<String> {
    let doc = Html::parse_document(html_source);
    let sel = Selector::parse("div.content1 div.info p span.status").ok()?;
    let elem = doc.select(&sel).next()?;
    let text = clean_text(&elem.text().collect::<String>());
    if text.is_empty() { None } else { Some(text) }
}

/// Extract the short novel description from the main page. The novel info
/// block is followed by a "Thông tin chi tiết:" marker paragraph and then
/// the description paragraph; some hosts (e.g. metruyenhotvn.com) inject an
/// extra empty `<p>` between the two, so we skip the first element-level
/// sibling (the marker) and return the next non-empty `<p>` we find.
pub(super) fn extract_novel_description_from_main_page(html_source: &str) -> Option<String> {
    let doc = Html::parse_document(html_source);
    let info_sel = Selector::parse("div.content1 div.info").ok()?;
    let info = doc.select(&info_sel).next()?;
    let mut sibling = info.next_sibling();
    let mut skipped_marker = false;
    while let Some(node) = sibling {
        if let Some(elem) = scraper::ElementRef::wrap(node) {
            if !skipped_marker {
                skipped_marker = true;
            } else if elem.value().name() == "p" {
                let text = clean_text(&elem.text().collect::<String>());
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        sibling = node.next_sibling();
    }
    None
}

/// Extract the author name from the novel's main page, trimming any trailing
/// "Thể loại:" suffix and surrounding punctuation. Returns `None` when the
/// author is not present or resolves to an empty string.
pub(super) fn extract_author_from_main_page(html_source: &str) -> Option<String> {
    let doc = Html::parse_document(html_source);
    let body_text: String = doc.root_element().text().collect();
    let captures = AUTHOR_RE.captures(&body_text)?;
    let raw = captures.get(1)?.as_str();
    let cleaned = clean_text(raw);
    let trimmed = cleaned
        .split("Thể loại:")
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches(|c: char| c == ',' || c.is_whitespace())
        .to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Find a cover image URL on the main page, walking the same selector
/// preference order as the TS implementation. Resolves relative URLs against
/// `novel_main_url`.
pub(super) fn extract_cover_image_url(novel_main_url: &str, html_source: &str) -> Option<String> {
    let doc = Html::parse_document(html_source);
    let selectors = [
        "img.lazyloaded",
        "img.lazyload",
        ".book-img img",
        ".detail-info img",
        ".info-img img",
        "img",
    ];
    let base = Url::parse(novel_main_url).ok()?;
    for selector in selectors {
        let sel = Selector::parse(selector).ok()?;
        for image in doc.select(&sel) {
            let src = image
                .value()
                .attr("src")
                .or_else(|| image.value().attr("data-src"))
                .or_else(|| image.value().attr("data-original"))
                .or_else(|| image.value().attr("data-lazy-src"));
            let raw = match src {
                Some(value) => value.trim(),
                None => continue,
            };
            if raw.is_empty() || raw.starts_with("data:") {
                continue;
            }
            if let Ok(absolute) = base.join(raw) {
                return Some(absolute.to_string());
            }
        }
    }
    None
}

/// Unit tests for the extractors. Inline because the extractors are private
/// to this adapter: an integration test under `tests/` could only reach them
/// through a `pub` re-export the pipeline has no use for.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_novel_title_prefers_h1_over_title_tag() {
        let html = "<html><head><title>Foo - truyenazz</title></head><body><h1>Cuốn Sách</h1></body></html>";
        assert_eq!(extract_novel_title_from_main_page(html), "Cuốn Sách");
    }

    #[test]
    fn extract_novel_title_strips_trailing_truyenazz_suffix() {
        let html = "<html><head><title>Foo Bar - truyenazz</title></head><body></body></html>";
        assert_eq!(extract_novel_title_from_main_page(html), "Foo Bar");
    }

    #[test]
    fn extract_novel_title_falls_back_to_unknown() {
        assert_eq!(
            extract_novel_title_from_main_page("<html></html>"),
            "Unknown Novel"
        );
    }

    #[test]
    fn extract_author_returns_none_when_missing() {
        assert!(extract_author_from_main_page("<html><body>Nothing</body></html>").is_none());
    }

    #[test]
    fn extract_author_strips_after_genre_marker() {
        let html = "<html><body>Tác giả: Nguyễn Văn A Thể loại: Tu chân</body></html>";
        assert_eq!(extract_author_from_main_page(html).unwrap(), "Nguyễn Văn A");
    }

    #[test]
    fn extract_cover_image_url_finds_lazy_loaded_img() {
        let html = "<html><body><div class=\"book-img\"><img class=\"lazyloaded\" src=\"/cover.jpg\"></div></body></html>";
        let url = extract_cover_image_url("https://metruyenhotvn.com/foo/", html).unwrap();
        assert_eq!(url, "https://metruyenhotvn.com/cover.jpg");
    }

    #[test]
    fn extract_cover_image_url_skips_data_uris() {
        let html = "<html><body><img src=\"data:image/png;base64,aaa\"><img class=\"lazyloaded\" src=\"/cover.jpg\"></body></html>";
        let url = extract_cover_image_url("https://metruyenhotvn.com/foo/", html).unwrap();
        assert_eq!(url, "https://metruyenhotvn.com/cover.jpg");
    }

    #[test]
    fn extract_cover_image_url_returns_none_when_no_img() {
        assert!(extract_cover_image_url("https://x/", "<html></html>").is_none());
    }

    #[test]
    fn extract_novel_status_pulls_status_span_under_info_p() {
        let html = r#"
    <html><body>
      <div class="content1">
        <div class="info">
          <p>Trạng thái: <span class="status">Đang ra</span></p>
        </div>
      </div>
    </body></html>
    "#;
        assert_eq!(
            extract_novel_status_from_main_page(html).as_deref(),
            Some("Đang ra")
        );
    }

    #[test]
    fn extract_novel_status_returns_none_when_missing() {
        assert!(extract_novel_status_from_main_page("<html></html>").is_none());
        // Status span exists but in the wrong place — must be under .content1 .info p.
        let unrelated = "<html><body><span class=\"status\">Đang ra</span></body></html>";
        assert!(extract_novel_status_from_main_page(unrelated).is_none());
    }

    #[test]
    fn extract_novel_description_returns_second_sibling_after_info() {
        let html = r#"
    <html><body>
      <div class="content1">
        <div class="info"><p>info goes here</p></div>
        <p>first sibling — not the desc</p>
        <p>The novel description goes here.</p>
      </div>
    </body></html>
    "#;
        assert_eq!(
            extract_novel_description_from_main_page(html).as_deref(),
            Some("The novel description goes here.")
        );
    }

    #[test]
    fn extract_novel_description_ignores_whitespace_text_nodes() {
        // Even though scraper produces text nodes between siblings, we only count
        // element siblings when locating the 2nd one after `info`.
        let html = "<html><body><div class=\"content1\"><div class=\"info\"></div>\n  <p>first</p>\n  <p>second is desc</p></div></body></html>";
        assert_eq!(
            extract_novel_description_from_main_page(html).as_deref(),
            Some("second is desc")
        );
    }

    #[test]
    fn extract_novel_description_returns_none_when_missing() {
        assert!(extract_novel_description_from_main_page("<html></html>").is_none());
        let only_one_sibling = r#"<html><body><div class="content1"><div class="info"></div><p>only one</p></div></body></html>"#;
        assert!(extract_novel_description_from_main_page(only_one_sibling).is_none());
    }

    #[test]
    fn extract_novel_description_skips_empty_paragraph_between_marker_and_body() {
        // metruyenhotvn.com renders an empty <p> between the "Thông tin chi tiết:"
        // marker and the actual description body; the extractor must skip past
        // empty siblings rather than returning the empty one as the description.
        let html = "<html><body><div class=\"content1\"><div class=\"info\"></div>\
            <p>Thông tin chi tiết:</p>\
            <p></p>\
            <p>real description</p></div></body></html>";
        assert_eq!(
            extract_novel_description_from_main_page(html).as_deref(),
            Some("real description")
        );
    }

    /// Locks in that the existing main-page selectors work on
    /// metruyenhotvn.com without changes — the unquoted-attribute
    /// difference is invisible to a real DOM parser.
    mod metruyenhot_regression {
        use super::*;

        /// Load the saved metruyenhot novel main-page fixture from disk.
        fn fixture() -> String {
            std::fs::read_to_string("tests/fixtures/metruyenhot_novel.html").unwrap()
        }

        #[test]
        fn extract_novel_title_from_metruyenhot_main_page() {
            let title = extract_novel_title_from_main_page(&fixture());
            assert!(title.contains("Vô Địch Tiên Nhân"), "title was: {title}");
        }

        #[test]
        fn extract_author_from_metruyenhot_main_page() {
            let author = extract_author_from_main_page(&fixture()).expect("author present");
            assert_eq!(author, "Tần Cấn");
        }

        #[test]
        fn extract_novel_status_from_metruyenhot_main_page() {
            let status = extract_novel_status_from_main_page(&fixture()).expect("status present");
            assert_eq!(status, "Đang ra");
        }

        #[test]
        fn extract_novel_description_from_metruyenhot_main_page() {
            let desc =
                extract_novel_description_from_main_page(&fixture()).expect("description present");
            assert!(
                desc.contains("bạn gái")
                    || desc.contains("Dương Bách Xuyên")
                    || desc.contains("công viên"),
                "description was: {desc}"
            );
        }

        #[test]
        fn extract_cover_image_url_from_metruyenhot_main_page() {
            let url =
                extract_cover_image_url("https://metruyenhotvn.com/vo-dich-tien-nhan/", &fixture());
            assert!(
                url.as_deref()
                    .map(|u| u.starts_with("http"))
                    .unwrap_or(false),
                "cover url: {url:?}"
            );
        }
    }
}
