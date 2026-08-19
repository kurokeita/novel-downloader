//! Pure text and URL helpers for the khodocsach adapter. Nothing here
//! performs I/O, so everything is unit tested inline.

use anyhow::{Context, Result, anyhow};
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use crate::utils::{clean_text, is_noise};

/// Pre-compiled regex matching a single HTML tag. The book description is the
/// only markup khodocsach serves, so a tag strip beats pulling in a parser.
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]*>").unwrap());

/// First path segment of a khodocsach URL that is never a book: taxonomy
/// archives and the API itself. A single-segment taxonomy URL would otherwise
/// look exactly like a book slug.
const RESERVED_SEGMENTS: &[&str] = &[
    "wp-json",
    "wp-admin",
    "wp-content",
    "wp-includes",
    "the_genre",
    "the_author",
    "the_status",
    "the_type",
];

/// The REST namespace every endpoint hangs off, appended to the site origin.
const API_PREFIX: &str = "/wp-json/app/v1";

/// Derive the API base (`<origin>/wp-json/app/v1`) from any URL on the site.
/// Taking the origin from the caller's URL rather than hard-coding the host is
/// what lets the whole adapter be exercised against a mock server.
pub(super) fn api_base(url: &str) -> Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("invalid URL: {url}"))?;
    let origin = parsed.origin().ascii_serialization();
    if origin == "null" {
        return Err(anyhow!("URL has no host: {url}"));
    }
    Ok(format!("{origin}{API_PREFIX}"))
}

/// Extract the book slug from a book-page URL, rejecting anything that is not
/// one: the site serves books at `/<slug>`, so any other shape of path is a
/// listing, a taxonomy archive or a chapter page, none of which this adapter
/// can resolve into a novel.
pub(super) fn book_slug_from_url(url: &str) -> Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("invalid URL: {url}"))?;
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.filter(|segment| !segment.is_empty()).collect())
        .unwrap_or_default();

    let [segment] = segments[..] else {
        return Err(anyhow!(
            "not a khodocsach book page: {url} (expected a single-segment path like /ten-truyen)"
        ));
    };

    // Book permalinks carry a `.kds` extension the API slug does not:
    // `/nguoi-tim-xac.kds/` is served for the book whose slug is
    // `nguoi-tim-xac`, and the bare form merely redirects to it. Splitting at
    // the first dot is safe because the `books/(?P<id>[\w-]+)` route cannot
    // match a dot, so no API slug ever contains one.
    let slug = segment.split('.').next().unwrap_or_default();
    if slug.is_empty() || RESERVED_SEGMENTS.contains(&slug) {
        return Err(anyhow!("not a khodocsach book page: {url}"));
    }
    Ok(slug.to_string())
}

/// Flatten the HTML book description into a single plain-text line. Tags
/// become spaces so `<p>a</p><p>b</p>` does not run together as `ab`, then
/// [`clean_text`] decodes entities and collapses the whitespace.
pub(super) fn strip_html_to_text(html: &str) -> String {
    clean_text(&TAG_RE.replace_all(html, " "))
}

/// Split plain-text chapter content into ordered paragraphs. khodocsach sends
/// one paragraph per line with no markup, so a line is a paragraph; blank
/// lines and known boilerplate are dropped and reading order is preserved.
pub(super) fn split_paragraphs(content: &str) -> Vec<String> {
    content
        .lines()
        .map(clean_text)
        .filter(|line| !is_noise(line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_appends_the_rest_namespace_to_the_origin() {
        assert_eq!(
            api_base("https://khodocsach.com/mot-truyen").unwrap(),
            "https://khodocsach.com/wp-json/app/v1"
        );
    }

    #[test]
    fn api_base_keeps_a_non_default_port() {
        assert_eq!(
            api_base("http://127.0.0.1:1234/mot-truyen").unwrap(),
            "http://127.0.0.1:1234/wp-json/app/v1"
        );
    }

    #[test]
    fn api_base_errors_on_a_url_without_a_host() {
        assert!(api_base("not-a-url").is_err());
    }

    #[test]
    fn book_slug_from_url_reads_a_single_segment_path() {
        assert_eq!(
            book_slug_from_url("https://khodocsach.com/mot-truyen-thu-nghiem").unwrap(),
            "mot-truyen-thu-nghiem"
        );
    }

    #[test]
    fn book_slug_from_url_strips_the_kds_permalink_extension() {
        assert_eq!(
            book_slug_from_url("https://khodocsach.com/nguoi-tim-xac.kds/").unwrap(),
            "nguoi-tim-xac"
        );
        assert_eq!(
            book_slug_from_url("https://khodocsach.com/nguoi-tim-xac.kds").unwrap(),
            "nguoi-tim-xac"
        );
    }

    #[test]
    fn book_slug_from_url_rejects_a_path_that_is_only_an_extension() {
        assert!(book_slug_from_url("https://khodocsach.com/.kds").is_err());
    }

    #[test]
    fn book_slug_from_url_tolerates_a_trailing_slash_and_a_query() {
        assert_eq!(
            book_slug_from_url("https://khodocsach.com/mot-truyen-thu-nghiem/?a=b").unwrap(),
            "mot-truyen-thu-nghiem"
        );
    }

    #[test]
    fn book_slug_from_url_rejects_the_site_root() {
        assert!(book_slug_from_url("https://khodocsach.com").is_err());
        assert!(book_slug_from_url("https://khodocsach.com/").is_err());
    }

    #[test]
    fn book_slug_from_url_rejects_a_chapter_page() {
        assert!(
            book_slug_from_url("https://khodocsach.com/mot-truyen-thu-nghiem/chuong-1-p127361")
                .is_err()
        );
    }

    #[test]
    fn book_slug_from_url_rejects_taxonomy_archives() {
        assert!(book_slug_from_url("https://khodocsach.com/the_genre/co-dai/").is_err());
        assert!(book_slug_from_url("https://khodocsach.com/the_genre").is_err());
        assert!(book_slug_from_url("https://khodocsach.com/wp-json").is_err());
    }

    #[test]
    fn strip_html_to_text_drops_tags_and_decodes_entities() {
        assert_eq!(
            strip_html_to_text("<p>Dòng đầu.</p>\n<p>Dòng hai &amp; ba.</p>"),
            "Dòng đầu. Dòng hai & ba."
        );
    }

    #[test]
    fn strip_html_to_text_keeps_adjacent_paragraphs_apart() {
        assert_eq!(strip_html_to_text("<p>a</p><p>b</p>"), "a b");
    }

    #[test]
    fn strip_html_to_text_returns_empty_for_markup_only_input() {
        assert_eq!(strip_html_to_text("<p></p>"), "");
    }

    #[test]
    fn split_paragraphs_keeps_one_paragraph_per_line_in_order() {
        assert_eq!(
            split_paragraphs("Một.\nHai.\nBa."),
            vec!["Một.", "Hai.", "Ba."]
        );
    }

    #[test]
    fn split_paragraphs_drops_blank_and_whitespace_only_lines() {
        assert_eq!(
            split_paragraphs("Một.\n\n   \n\tHai.\n"),
            vec!["Một.", "Hai."]
        );
    }

    #[test]
    fn split_paragraphs_drops_known_boilerplate() {
        assert_eq!(
            split_paragraphs("Một.\nNhấn Mở Bình Luận để xem thêm.\nHai."),
            vec!["Một.", "Hai."]
        );
    }

    #[test]
    fn split_paragraphs_returns_empty_for_content_with_nothing_usable() {
        assert!(split_paragraphs("   \n\n\t").is_empty());
    }
}
