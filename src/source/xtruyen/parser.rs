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

/// Rebuild `href` against the origin of `base`, keeping only its path.
///
/// The site writes its own absolute URLs into every link, so a run against a
/// mock server or a mirror would otherwise walk straight back to production.
/// Taking the path and keeping the caller's origin is what makes the adapter
/// testable without an injected base URL.
pub(super) fn rebase_onto(base: &str, href: &str) -> Result<String> {
    let base_url = Url::parse(base).with_context(|| format!("invalid base URL: {base}"))?;
    if base_url.host_str().is_none() {
        return Err(anyhow!("base URL has no host: {base}"));
    }
    let target = base_url
        .join(href)
        .with_context(|| format!("invalid link {href} on {base}"))?;

    let mut rebased = base_url;
    rebased.set_path(target.path());
    rebased.set_query(None);
    rebased.set_fragment(None);
    Ok(rebased.to_string())
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
    fn rebase_onto_moves_a_production_href_to_the_callers_origin() {
        assert_eq!(
            rebase_onto(
                "http://127.0.0.1:1234/truyen/truyen-thu-nghiem/",
                "https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-2/"
            )
            .unwrap(),
            "http://127.0.0.1:1234/truyen/truyen-thu-nghiem/chuong-2/"
        );
    }

    #[test]
    fn rebase_onto_resolves_a_relative_href() {
        assert_eq!(
            rebase_onto(
                "https://xtruyen.vn/truyen/truyen-thu-nghiem/",
                "/truyen/truyen-thu-nghiem/chuong-2/"
            )
            .unwrap(),
            "https://xtruyen.vn/truyen/truyen-thu-nghiem/chuong-2/"
        );
    }

    #[test]
    fn rebase_onto_keeps_the_callers_port() {
        assert_eq!(
            rebase_onto(
                "http://localhost:8765/truyen/a/",
                "https://xtruyen.vn/truyen/a/chuong-9/"
            )
            .unwrap(),
            "http://localhost:8765/truyen/a/chuong-9/"
        );
    }

    #[test]
    fn rebase_onto_errors_on_a_base_without_a_host() {
        assert!(rebase_onto("not-a-url", "/truyen/a/chuong-1/").is_err());
    }
}
