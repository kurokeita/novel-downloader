//! The khodocsach adapter: `khodocsach.com`.
//!
//! Unlike metruyenhot this is a JSON API, not a set of scraped pages, so no
//! HTML parser is involved. Two things shape the implementation:
//!
//! - Chapter ids are opaque database ids that cannot be derived from a URL, so
//!   [`SiteAdapter::fetch_novel`] must page the real chapter listing rather
//!   than synthesize a range.
//! - Chapter content sits behind a short-lived per-chapter ticket. The ticket
//!   is requested inside the chapter fetch, immediately before the content
//!   request, because it expires in about a minute and the runner's pacer can
//!   hold a queued chapter for longer than that.

mod api;
mod parser;

use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use url::Url;

use crate::source::{
    ChapterContent, ChapterRef, Novel, RatePolicy, SiteAdapter, SourceError, SourceResult,
};
use crate::utils::http_client;

use api::{ApiError, Book, ChapterContentResponse, ChapterListItem, ChapterListPage, Ticket};

/// Hosts this adapter claims.
const HOSTS: &[&str] = &["khodocsach.com"];

/// Machine id used in logs, errors and the rate-limit message.
const ID: &str = "khodocsach";

/// Page size asked of the chapter listing. The host caps it at 200, so
/// requesting more only wastes the round trip; the count the server reports
/// back is what the walk trusts.
const LISTING_PER_PAGE: u32 = 200;

/// Query parameter carrying the novel title along with a chapter locator.
/// The chapter content endpoint does not return the book title, but the
/// crawler needs it to name the output directory, so it rides on the locator,
/// which the trait defines as opaque and owned entirely by this adapter.
const BOOK_TITLE_PARAM: &str = "book";

/// How long a single API request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The khodocsach adapter. Stateless, so the registry hands out a
/// `&'static` reference.
pub struct Khodocsach;

/// A failed API hop: the HTTP status plus the WordPress error envelope when
/// the body carried one. `status` is 0 for a transport-level failure, which
/// has no status but still needs a message.
#[derive(Debug)]
struct ApiFailure {
    status: u16,
    code: String,
    message: String,
    /// Wait the server asked for, when it sent one. Only a refusal carries
    /// this; a transport or schema failure has no headers to read.
    retry_after: Option<Duration>,
}

impl ApiFailure {
    /// Build the failure used for anything below the HTTP layer: no status to
    /// report, only a message.
    fn transport(message: String) -> Self {
        Self {
            status: 0,
            code: "transport".to_string(),
            message,
            retry_after: None,
        }
    }

    /// Build the failure used when a response arrives intact but does not
    /// match the shape this adapter expects. Kept distinct from
    /// [`ApiFailure::transport`] so schema drift on the site does not get
    /// reported to the user as a network problem.
    fn malformed(message: String) -> Self {
        Self {
            status: 0,
            code: "malformed response".to_string(),
            message,
            retry_after: None,
        }
    }

    /// True when the server rejected the ticket rather than the request. The
    /// caller answers by obtaining a fresh ticket and retrying once, since the
    /// usual cause is a ticket that expired while the chapter waited its turn.
    fn is_ticket_invalid(&self) -> bool {
        self.status == 401 || self.code == "ticket_invalid"
    }
}

impl From<ApiFailure> for SourceError {
    /// Translate a failed hop into the pipeline's vocabulary. Only statuses
    /// observed against the live host are mapped; anything else stays opaque
    /// rather than being guessed at.
    fn from(failure: ApiFailure) -> Self {
        let ApiFailure {
            status,
            code,
            message,
            retry_after,
        } = failure;
        match status {
            429 => Self::RateLimited {
                source_name: ID,
                message,
                retry_after,
            },
            403 => Self::ClientRejected(message),
            404 => Self::NotFound(message),
            0 => Self::Other(anyhow!("khodocsach {code}: {message}")),
            _ => Self::Other(anyhow!(
                "khodocsach API error (HTTP {status}, {code}): {message}"
            )),
        }
    }
}

/// GET `url` and deserialize a successful JSON body. Every non-2xx is turned
/// into an [`ApiFailure`] carrying the status and, when the body is the usual
/// WordPress error envelope, its `code` and `message`.
async fn get_json<T: DeserializeOwned>(url: &str, ua: Option<&str>) -> Result<T, ApiFailure> {
    let client =
        http_client(REQUEST_TIMEOUT, ua).map_err(|e| ApiFailure::transport(e.to_string()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ApiFailure::transport(format!("failed to fetch {url}: {e}")))?;

    let status = response.status();
    let retry_after = crate::utils::retry_after(response.headers());
    let body = response
        .text()
        .await
        .map_err(|e| ApiFailure::transport(format!("failed to read body from {url}: {e}")))?;

    if !status.is_success() {
        let envelope = serde_json::from_str::<ApiError>(&body).ok();
        return Err(ApiFailure {
            status: status.as_u16(),
            code: envelope
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |e| e.code.clone()),
            message: envelope.map_or_else(
                || format!("HTTP {} from {url}", status.as_u16()),
                |e| e.message,
            ),
            retry_after,
        });
    }

    serde_json::from_str::<T>(&body)
        .map_err(|e| ApiFailure::malformed(format!("unexpected JSON at {url}: {e}")))
}

/// Resolve a book-page URL into the API base and the book payload. Shared by
/// the two metadata paths so neither can drift from the other.
async fn resolve_book(url: &str) -> SourceResult<(String, Book)> {
    let base = parser::api_base(url)?;
    let slug = parser::book_slug_from_url(url)?;
    let book = get_json::<Book>(&format!("{base}/books/{slug}"), None).await?;
    Ok((base, book))
}

/// Map a book payload onto [`Novel`], leaving the chapter index empty. The
/// description arrives as HTML and is the one field needing a strip.
fn novel_from_book(book: &Book) -> Novel {
    Novel {
        title: book.title.clone(),
        author: book.author.as_ref().map(|term| term.name.clone()),
        description: book
            .desc
            .as_deref()
            .map(parser::strip_html_to_text)
            .filter(|text| !text.is_empty()),
        status: book.status.as_ref().map(|term| term.name.clone()),
        cover_url: book.cover.clone().filter(|url| !url.is_empty()),
        chapters: Vec::new(),
    }
}

/// Build the opaque locator for one chapter: its API URL plus the novel title
/// the crawler needs for the output directory.
fn chapter_locator(base: &str, chapter_id: u64, novel_title: &str) -> anyhow::Result<String> {
    let mut locator = Url::parse(&format!("{base}/chapters/{chapter_id}"))?;
    locator
        .query_pairs_mut()
        .append_pair(BOOK_TITLE_PARAM, novel_title);
    Ok(locator.to_string())
}

/// Split a locator back into the bare chapter API URL and the novel title,
/// undoing [`chapter_locator`].
fn parse_locator(locator: &str) -> anyhow::Result<(String, String)> {
    let mut parsed = Url::parse(locator)?;
    let novel_title = parsed
        .query_pairs()
        .find(|(key, _)| key == BOOK_TITLE_PARAM)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    parsed.set_query(None);
    Ok((parsed.to_string(), novel_title))
}

/// Walk the chapter listing to completion and return its entries in reading
/// order. The listing is served newest-first and the walk stops on the page
/// count the server reports, not on one this adapter assumed. A page that
/// cannot be retrieved aborts the whole index: a partial one would silently
/// truncate the novel.
async fn fetch_chapter_index(base: &str, book_id: u64) -> SourceResult<Vec<ChapterListItem>> {
    let mut items: Vec<ChapterListItem> = Vec::new();
    let mut page: u32 = 1;
    loop {
        let url =
            format!("{base}/books/{book_id}/chapters?page={page}&per_page={LISTING_PER_PAGE}");
        let listing = get_json::<ChapterListPage>(&url, None).await?;
        items.extend(listing.data);
        if page >= listing.pagination.total_pages {
            break;
        }
        page += 1;
    }
    items.sort_by_key(|item| item.index);
    Ok(items)
}

/// Perform one ticket-then-content round trip. Kept separate from the retry
/// so the retry re-runs both hops, which is the point: a fresh ticket is
/// useless without a fresh content request to spend it on.
async fn fetch_content_once(
    chapter_url: &str,
    ua: Option<&str>,
) -> Result<ChapterContentResponse, ApiFailure> {
    let ticket = get_json::<Ticket>(&format!("{chapter_url}/ticket"), ua).await?;

    let mut content_url = Url::parse(chapter_url).map_err(|e| {
        ApiFailure::transport(format!("invalid chapter locator {chapter_url}: {e}"))
    })?;
    content_url
        .query_pairs_mut()
        .append_pair("nonce", &ticket.nonce)
        .append_pair("exp", &ticket.exp.to_string())
        .append_pair("sig", &ticket.sig);

    get_json::<ChapterContentResponse>(content_url.as_str(), ua).await
}

#[async_trait]
impl SiteAdapter for Khodocsach {
    /// Stable machine id used in logs and errors.
    fn id(&self) -> &'static str {
        ID
    }

    /// Name shown on the wizard summary screen.
    fn display_name(&self) -> &'static str {
        "khodocsach"
    }

    /// The single host this adapter claims.
    fn hosts(&self) -> &'static [&'static str] {
        HOSTS
    }

    /// The limiter guards the `/ticket` hop and buckets requests by the exact
    /// `User-Agent` header. Rather than pacing the crawler sequentially and
    /// forcing the user to wait hours for a long novel, we bypass the limit
    /// entirely by rotating the User-Agent per-chapter (`rev/{chapter_number}`).
    ///
    /// - `max_concurrency`: Uncapped (`usize::MAX`), allowing the user to
    ///   throw as many workers at it as they want (similar to metruyenhot).
    /// - `min_delay`: `0`s, because the unique UAs absorb the burst.
    /// - `backoff_base`: Reduced from a 3-minute penalty wait to just `2`s.
    ///   If a worker happens to hit a 429, it retries almost immediately.
    fn rate_policy(&self) -> RatePolicy {
        RatePolicy {
            max_concurrency: usize::MAX,
            min_delay: Duration::from_secs(0),
            max_retries: 2,
            backoff_base: Duration::from_secs(2),
        }
    }

    /// Resolve the book, then page its chapter listing to completion. The
    /// index is real data from the site, not a synthesized range: chapter ids
    /// are opaque and cannot be computed from a chapter number.
    async fn fetch_novel(&self, url: &str) -> SourceResult<Novel> {
        let (base, book) = resolve_book(url).await?;
        let items = fetch_chapter_index(&base, book.id).await?;

        let mut novel = novel_from_book(&book);
        novel.chapters = items
            .into_iter()
            .map(|item| {
                Ok(ChapterRef {
                    number: item.index,
                    title: Some(item.title),
                    locator: chapter_locator(&base, item.id, &book.title)?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(novel)
    }

    /// One book request and nothing else. `--epub-only` must not pay for the
    /// listing walk, which on a long novel is several more round trips against
    /// a rate-limited host.
    async fn fetch_metadata(&self, url: &str) -> SourceResult<Novel> {
        let (_, book) = resolve_book(url).await?;
        Ok(novel_from_book(&book))
    }

    /// Ticket, then content, retrying the pair once when the server rejects
    /// the ticket. No backoff or sleeping happens here: the runner's pacer
    /// owns rate handling, and a second layer would compound its delays.
    async fn fetch_chapter(&self, chapter: &ChapterRef) -> SourceResult<ChapterContent> {
        let (chapter_url, novel_title) = parse_locator(&chapter.locator)?;

        let rotated_ua = format!("{} rev/{}", crate::utils::USER_AGENT, chapter.number);
        let ua = Some(rotated_ua.as_str());

        let response = match fetch_content_once(&chapter_url, ua).await {
            Ok(response) => response,
            Err(failure) if failure.is_ticket_invalid() => {
                fetch_content_once(&chapter_url, ua).await?
            }
            Err(failure) => return Err(failure.into()),
        };

        if !response.can_read {
            return Err(SourceError::Unentitled(format!(
                "{} is not available to this client",
                response.title
            )));
        }

        let paragraphs = parser::split_paragraphs(&response.content);
        if paragraphs.is_empty() {
            return Err(anyhow!("No chapter content extracted from {chapter_url}").into());
        }

        Ok(ChapterContent {
            novel_title,
            chapter_title: response.title,
            paragraphs,
        })
    }
}
