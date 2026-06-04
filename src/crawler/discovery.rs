use anyhow::{Result, anyhow};
use scraper::{Html, Selector};
use url::Url;

use crate::utils::fetch_html;

/// Return the absolute URL of the pagination "jump to last page" link
/// (`div.pagination li.nexts a[href]`) on the novel main page, or `None`
/// when there is no such link (single-page chapter lists / short novels).
/// Relative hrefs are resolved against `main_url`.
pub fn find_last_page_url(html: &str, main_url: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("div.pagination li.nexts a[href]").ok()?;
    let elem = doc.select(&sel).next()?;
    let href = elem.value().attr("href")?;
    let base = Url::parse(main_url).ok()?;
    base.join(href).ok().map(|u| u.to_string())
}

/// Scan every `<a href>` on the page and return the largest `N` from any
/// link whose absolute URL path is `<novel_slug>/chuong-N[/]` where
/// `<novel_slug>` is derived from `main_url`'s path. Returns `None` when no
/// such chapter link is present.
///
/// Filtering by path slug (rather than origin) lets us follow legitimate
/// cross-host CDN mirrors that share the same path layout (e.g.
/// `metruyenhotvn.com` and `metruyenhotne.com` serve the same chapter URLs
/// under matching paths), while still rejecting chuong links that belong to
/// a different novel slug or sit on an unrelated path.
pub fn max_chapter_in_html(html: &str, main_url: &str) -> Option<u32> {
    let doc = Html::parse_document(html);
    let a_sel = Selector::parse("a[href]").ok()?;
    let base = Url::parse(main_url).ok()?;
    let novel_path = format!("{}/", base.path().trim_end_matches('/'));
    let chuong_prefix = format!("{novel_path}chuong-");
    let mut max_n: Option<u32> = None;
    for a in doc.select(&a_sel) {
        let href = match a.value().attr("href") {
            Some(h) => h,
            None => continue,
        };
        let absolute = match base.join(href) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let rest = match absolute.path().strip_prefix(&chuong_prefix) {
            Some(r) => r.trim_end_matches('/'),
            None => continue,
        };
        if let Ok(n) = rest.parse::<u32>() {
            max_n = Some(max_n.map_or(n, |m| m.max(n)));
        }
    }
    max_n
}

/// Async wrapper around [`max_chapter_in_html`]: extract the highest chapter
/// number visible in the given (already-fetched) novel main-page HTML.
/// Errors when no `/chuong-N/` link is present in the document. Used by
/// callers that already have the HTML in hand (e.g. the wizard's discover
/// step) and want a cheap, single-page answer.
pub fn discover_last_chapter_number_from_html(html: &str, main_url: &str) -> Result<u32> {
    max_chapter_in_html(html, main_url)
        .ok_or_else(|| anyhow!("Could not find any /chuong-N/ links on {main_url}."))
}

/// Discover the highest available chapter number for a novel by following
/// the chapter-list pagination. The novel main page lists only the first
/// `N` chapters; `div.pagination li.nexts` links to the last page where the
/// newest chapter lives. Falls back to the main page itself when no
/// pagination is present (short novels with a single-page chapter list).
pub async fn discover_last_chapter_number(base_url: &str) -> Result<u32> {
    let trimmed = base_url.trim_end_matches('/');
    let main_url = format!("{trimmed}/");
    let main_html = fetch_html(&main_url).await?;

    let scan_url = match find_last_page_url(&main_html, &main_url) {
        Some(last_page) => last_page,
        None => {
            return max_chapter_in_html(&main_html, &main_url)
                .ok_or_else(|| anyhow!("No chapter links on {main_url}."));
        }
    };

    let last_html = fetch_html(&scan_url).await?;
    // Prefer the paginated last page (it carries the newest chapters), but
    // fall back to the main page if the paginated response somehow has no
    // chuong-N links — better to under-report than to fail outright.
    max_chapter_in_html(&last_html, &main_url)
        .or_else(|| max_chapter_in_html(&main_html, &main_url))
        .ok_or_else(|| anyhow!("No chapter links on {scan_url} or {main_url}."))
}
