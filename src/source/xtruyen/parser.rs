//! Pure URL helpers for the xtruyen adapter. Nothing here performs I/O, so
//! everything is unit tested inline.

use anyhow::{Context, Result, anyhow};
use url::Url;

/// Path segment every novel lives under: the site serves novels at
/// `/truyen/<slug>/` and chapters one segment deeper.
const NOVEL_SEGMENT: &str = "truyen";

/// Extract the novel slug from a novel-page URL, rejecting anything that is not
/// one. A novel page is exactly `/truyen/<slug>`, so a listing, the site root
/// and a chapter page (which carries a third segment) are all turned away
/// before any request is made.
pub(super) fn novel_slug_from_url(url: &str) -> Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("invalid URL: {url}"))?;
    if parsed.host_str().is_none() {
        return Err(anyhow!("URL has no host: {url}"));
    }
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|segments| segments.filter(|segment| !segment.is_empty()).collect())
        .unwrap_or_default();

    match segments[..] {
        [NOVEL_SEGMENT, slug] => Ok(slug.to_string()),
        _ => Err(anyhow!(
            "not an xtruyen novel page: {url} (expected /{NOVEL_SEGMENT}/<ten-truyen>)"
        )),
    }
}

/// Origin of `url`, for building the site-wide endpoints that hang off it.
/// Deriving this from the caller's URL rather than hard-coding a host is what
/// lets the whole adapter run against a mock server.
pub(super) fn origin_of(url: &str) -> Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("invalid URL: {url}"))?;
    let origin = parsed.origin().ascii_serialization();
    if origin == "null" {
        return Err(anyhow!("URL has no host: {url}"));
    }
    Ok(origin)
}

/// Append `path` to a novel URL, tolerating a novel URL with or without its
/// trailing slash.
pub(super) fn join_path(novel_url: &str, path: &str) -> Result<String> {
    let trimmed = novel_url.trim_end_matches('/');
    origin_of(trimmed)?;
    Ok(format!("{trimmed}/{path}"))
}

/// Address of one chapter, from the novel URL and the slug the index supplied.
pub(super) fn chapter_url(novel_url: &str, slug: &str) -> Result<String> {
    let slug = slug.trim().trim_matches('/');
    if slug.is_empty() {
        return Err(anyhow!("chapter index carried an empty address"));
    }
    join_path(novel_url, &format!("{slug}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn novel_slug_from_url_reads_the_slug_after_the_novel_segment() {
        assert_eq!(
            novel_slug_from_url("https://xtruyen.vn/truyen/truyen-thu-nghiem").unwrap(),
            "truyen-thu-nghiem"
        );
    }

    #[test]
    fn novel_slug_from_url_tolerates_a_trailing_slash_and_a_query() {
        assert_eq!(
            novel_slug_from_url("https://xtruyen.vn/truyen/truyen-thu-nghiem/?a=b").unwrap(),
            "truyen-thu-nghiem"
        );
    }

    #[test]
    fn novel_slug_from_url_rejects_the_site_root() {
        assert!(novel_slug_from_url("https://xtruyen.vn").is_err());
        assert!(novel_slug_from_url("https://xtruyen.vn/").is_err());
    }

    #[test]
    fn novel_slug_from_url_rejects_a_listing_that_is_not_a_novel() {
        assert!(novel_slug_from_url("https://xtruyen.vn/the-loai/co-dai/").is_err());
        assert!(novel_slug_from_url("https://xtruyen.vn/tac-gia/mot-tac-gia/").is_err());
        assert!(novel_slug_from_url("https://xtruyen.vn/truyen/").is_err());
    }

    #[test]
    fn novel_slug_from_url_rejects_a_chapter_page() {
        assert!(
            novel_slug_from_url("https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-1/").is_err(),
            "a chapter page names a chapter, not a novel"
        );
    }

    #[test]
    fn novel_slug_from_url_errors_on_a_url_without_a_host() {
        assert!(novel_slug_from_url("not-a-url").is_err());
    }

    #[test]
    fn origin_of_keeps_a_non_default_port() {
        assert_eq!(
            origin_of("http://127.0.0.1:1234/truyen/a/").unwrap(),
            "http://127.0.0.1:1234"
        );
    }

    #[test]
    fn origin_of_errors_on_a_url_without_a_host() {
        assert!(origin_of("not-a-url").is_err());
    }

    #[test]
    fn join_path_appends_with_or_without_a_trailing_slash() {
        for base in [
            "https://xtruyen.vn/truyen/a",
            "https://xtruyen.vn/truyen/a/",
        ] {
            assert_eq!(
                join_path(base, "ajax/chapters/").unwrap(),
                "https://xtruyen.vn/truyen/a/ajax/chapters/"
            );
        }
    }

    #[test]
    fn chapter_url_builds_a_chapter_address_from_a_slug() {
        assert_eq!(
            chapter_url("https://xtruyen.vn/truyen/a/", "chuong-1-1").unwrap(),
            "https://xtruyen.vn/truyen/a/chuong-1-1/",
            "an extension chapter's address survives untouched"
        );
    }

    #[test]
    fn chapter_url_keeps_the_callers_origin() {
        assert_eq!(
            chapter_url("http://127.0.0.1:1234/truyen/a", "chuong-9").unwrap(),
            "http://127.0.0.1:1234/truyen/a/chuong-9/"
        );
    }

    #[test]
    fn chapter_url_rejects_an_empty_slug() {
        assert!(chapter_url("https://xtruyen.vn/truyen/a/", "  ").is_err());
    }
}
