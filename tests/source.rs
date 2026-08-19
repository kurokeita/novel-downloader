use novel_downloader::source::khodocsach::Khodocsach;
use novel_downloader::source::registry::{normalize_host, resolve, supported_hosts, validate_url};
use novel_downloader::source::{ChapterRef, SiteAdapter, SourceError};

/// Resolve without the local-host escape hatch, mirroring the old
/// `ensure_supported`.
fn ensure_supported(url: &str) -> anyhow::Result<String> {
    resolve(url, false)?;
    normalize_host(url)
}

/// Resolve with `--allow-any-host`, mirroring the old
/// `ensure_supported_or_local`.
fn ensure_supported_or_local(url: &str) -> anyhow::Result<String> {
    resolve(url, true)?;
    normalize_host(url)
}

#[test]
fn resolve_maps_every_metruyenhot_host_to_the_metruyenhot_adapter() {
    for host in ["metruyenhotne.com", "metruyenhotvn.com"] {
        let adapter = resolve(&format!("https://{host}/foo"), false).unwrap();
        assert_eq!(adapter.id(), "metruyenhot");
        assert_eq!(adapter.display_name(), "metruyenhot");
    }
}

#[test]
fn resolve_claims_every_supported_host_for_exactly_one_adapter() {
    for host in supported_hosts() {
        resolve(&format!("https://{host}/foo"), false)
            .unwrap_or_else(|e| panic!("{host} is listed as supported but does not resolve: {e}"));
    }
}

#[test]
fn resolve_accepts_localhost_only_with_allow_any_host() {
    assert!(resolve("http://localhost:8765/foo", false).is_err());
    assert_eq!(
        resolve("http://localhost:8765/foo", true).unwrap().id(),
        "metruyenhot"
    );
}

#[test]
fn supported_hosts_listed_alphabetically_for_stable_error_messages() {
    assert_eq!(
        supported_hosts(),
        vec!["khodocsach.com", "metruyenhotne.com", "metruyenhotvn.com"]
    );
}

#[test]
fn normalize_host_lowercases_and_strips_www() {
    assert_eq!(
        normalize_host("https://WWW.MetruyenhotVN.com/foo").unwrap(),
        "metruyenhotvn.com"
    );
}

#[test]
fn normalize_host_keeps_apex_host_unchanged() {
    assert_eq!(
        normalize_host("https://metruyenhotvn.com/bar/").unwrap(),
        "metruyenhotvn.com"
    );
}

#[test]
fn normalize_host_errors_on_url_without_host() {
    let err = normalize_host("not-a-url").unwrap_err();
    assert!(err.to_string().to_lowercase().contains("url"), "got: {err}");
}

#[test]
fn ensure_supported_accepts_each_listed_host() {
    for host in supported_hosts() {
        let url = format!("https://{host}/some-novel");
        let resolved = ensure_supported(&url)
            .unwrap_or_else(|e| panic!("expected {host} to be accepted, got {e}"));
        assert_eq!(&resolved, host);
    }
}

#[test]
fn ensure_supported_accepts_www_prefixed_host() {
    let host = ensure_supported("https://www.metruyenhotne.com/foo").unwrap();
    assert_eq!(host, "metruyenhotne.com");
}

#[test]
fn ensure_supported_rejects_unknown_host_and_lists_supported() {
    let err = ensure_supported("https://example.com/foo")
        .unwrap_err()
        .to_string();
    assert!(err.contains("example.com"), "got: {err}");
    for host in supported_hosts() {
        assert!(err.contains(host), "missing {host} in error: {err}");
    }
}

#[test]
fn ensure_supported_or_local_accepts_localhost() {
    assert_eq!(
        ensure_supported_or_local("http://localhost:8765/foo").unwrap(),
        "localhost"
    );
    assert_eq!(
        ensure_supported_or_local("http://127.0.0.1:8765/foo").unwrap(),
        "127.0.0.1"
    );
}

#[test]
fn ensure_supported_or_local_still_rejects_unrelated_host() {
    assert!(ensure_supported_or_local("https://example.com/foo").is_err());
}

#[test]
fn validate_url_returns_none_on_supported_host() {
    assert!(validate_url("https://metruyenhotvn.com/foo", false).is_none());
}

#[test]
fn validate_url_returns_error_message_on_unknown_host() {
    let msg = validate_url("https://example.com/foo", false).unwrap();
    assert!(msg.contains("example.com"));
    assert!(msg.contains("metruyenhotvn.com"));
}

#[test]
fn validate_url_with_allow_any_host_accepts_localhost() {
    assert!(validate_url("http://localhost:8765/foo", true).is_none());
}

#[test]
fn validate_url_with_allow_any_host_still_rejects_unsupported() {
    assert!(validate_url("https://example.com/foo", true).is_some());
}

/// Main page carrying full metadata plus a pagination link whose target is
/// broken: exactly the shape that made `--epub-only` fail once plan
/// construction started calling `fetch_novel` unconditionally.
fn main_page_with_broken_pagination(server_url: &str) -> String {
    format!(
        r#"<html><head><title>Cuốn Sách - truyenazz</title></head><body>
  <h1>Cuốn Sách</h1>
  <div class="content1"><div class="info">
    <p>Tác giả: Nguyễn Văn A</p>
    <p><span class="status">Đang ra</span></p>
  </div>
  <p>Thông tin chi tiết:</p>
  <p>Một truyện rất hay.</p>
  </div>
  <img class="lazyloaded" src="/cover.jpg" />
  <div class="pagination"><ul>
    <li class="nexts"><a href="{server_url}/foo?page=9"></a></li>
  </ul></div>
</body></html>"#
    )
}

#[tokio::test]
async fn fetch_metadata_returns_metadata_without_the_chapter_index() {
    let mut server = mockito::Server::new_async().await;
    let main_html = main_page_with_broken_pagination(&server.url());
    let _main = server
        .mock("GET", "/foo/")
        .with_body(&main_html)
        .create_async()
        .await;
    // The pagination target must never be requested: metadata-only callers do
    // not pay for discovery.
    let pagination = server
        .mock("GET", "/foo?page=9")
        .with_status(500)
        .expect(0)
        .create_async()
        .await;

    let url = format!("{}/foo", server.url());
    let adapter = resolve(&url, true).unwrap();
    let novel = adapter.fetch_metadata(&url).await.unwrap();

    assert_eq!(novel.title, "Cuốn Sách");
    assert_eq!(novel.author.as_deref(), Some("Nguyễn Văn A"));
    assert_eq!(novel.status.as_deref(), Some("Đang ra"));
    assert_eq!(novel.description.as_deref(), Some("Một truyện rất hay."));
    assert_eq!(
        novel.cover_url.as_deref(),
        Some(format!("{}/cover.jpg", server.url()).as_str())
    );
    assert!(
        novel.chapters.is_empty(),
        "metadata-only fetch must leave the index empty, got {} chapters",
        novel.chapters.len()
    );
    pagination.assert_async().await;
}

#[tokio::test]
async fn fetch_novel_still_walks_pagination_and_fails_when_it_is_unreachable() {
    let mut server = mockito::Server::new_async().await;
    let main_html = main_page_with_broken_pagination(&server.url());
    let _main = server
        .mock("GET", "/foo/")
        .with_body(&main_html)
        .create_async()
        .await;
    let _pagination = server
        .mock("GET", "/foo?page=9")
        .with_status(500)
        .create_async()
        .await;

    let url = format!("{}/foo", server.url());
    let adapter = resolve(&url, true).unwrap();
    assert!(
        adapter.fetch_novel(&url).await.is_err(),
        "fetch_novel needs the chapter index and must still fail here"
    );
}

// ---------------------------------------------------------------------------
// khodocsach
// ---------------------------------------------------------------------------

const BOOK_JSON: &str = include_str!("fixtures/khodocsach_book.json");
const CHAPTERS_PAGE1_JSON: &str = include_str!("fixtures/khodocsach_chapters_page1.json");
const CHAPTERS_PAGE2_JSON: &str = include_str!("fixtures/khodocsach_chapters_page2.json");
const TICKET_JSON: &str = include_str!("fixtures/khodocsach_ticket.json");
const CONTENT_JSON: &str = include_str!("fixtures/khodocsach_chapter_content.json");

/// Book-page URL on the mock server. The adapter derives its API base from
/// this URL's origin, which is what makes it testable without an injected
/// base URL.
fn khodocsach_book_url(server: &mockito::Server) -> String {
    format!("{}/mot-truyen-thu-nghiem", server.url())
}

/// Mount the book-resolution response for the fixture's slug.
async fn mount_book(server: &mut mockito::Server) -> mockito::Mock {
    server
        .mock("GET", "/wp-json/app/v1/books/mot-truyen-thu-nghiem")
        .with_header("content-type", "application/json")
        .with_body(BOOK_JSON)
        .create_async()
        .await
}

/// Mount both listing pages for the fixture book id.
async fn mount_listing(server: &mut mockito::Server) -> (mockito::Mock, mockito::Mock) {
    let page1 = server
        .mock("GET", "/wp-json/app/v1/books/83420/chapters")
        .match_query(mockito::Matcher::UrlEncoded("page".into(), "1".into()))
        .with_header("content-type", "application/json")
        .with_body(CHAPTERS_PAGE1_JSON)
        .create_async()
        .await;
    let page2 = server
        .mock("GET", "/wp-json/app/v1/books/83420/chapters")
        .match_query(mockito::Matcher::UrlEncoded("page".into(), "2".into()))
        .with_header("content-type", "application/json")
        .with_body(CHAPTERS_PAGE2_JSON)
        .create_async()
        .await;
    (page1, page2)
}

/// A [`ChapterRef`] addressing chapter 1 on the mock server, as
/// `fetch_novel` would have produced it.
fn chapter_one_ref(server: &mockito::Server) -> ChapterRef {
    ChapterRef {
        number: 1,
        title: Some("Chương 1: Một".to_string()),
        locator: format!(
            "{}/wp-json/app/v1/chapters/127361?book=M%E1%BB%99t+Truy%E1%BB%87n+Th%E1%BB%AD+Nghi%E1%BB%87m",
            server.url()
        ),
    }
}

#[test]
fn resolve_maps_khodocsach_host_to_the_khodocsach_adapter() {
    let adapter = resolve("https://khodocsach.com/mot-truyen", false).unwrap();
    assert_eq!(adapter.id(), "khodocsach");
}

#[test]
fn khodocsach_rate_policy_is_conservative_unlike_metruyenhot() {
    let khodocsach = resolve("https://khodocsach.com/foo", false).unwrap();
    let metruyenhot = resolve("https://metruyenhotvn.com/foo", false).unwrap();

    let policy = khodocsach.rate_policy();
    assert!(
        policy.max_concurrency <= 3,
        "expected a clamped concurrency, got {}",
        policy.max_concurrency
    );
    assert!(
        !policy.min_delay.is_zero(),
        "expected a non-zero min delay for a rate-limited host"
    );
    assert!(
        policy.max_retries > 0,
        "a rate-limited host must be worth retrying"
    );
    assert!(
        metruyenhot.rate_policy().max_concurrency > policy.max_concurrency,
        "metruyenhot must stay the permissive one"
    );
}

#[test]
fn khodocsach_rate_policy_stays_under_the_measured_ceiling() {
    let policy = resolve("https://khodocsach.com/foo", false)
        .unwrap()
        .rate_policy();

    // A chapter costs two requests (ticket, then content) and the pacer
    // spaces chapter *attempts*, so min_delay of 2s is 1 req/s. A browser
    // sustained 1.62 req/s over 100 requests but was refused at 4.3 req/s,
    // so 1 req/s sits comfortably under the measured ceiling.
    assert!(
        policy.min_delay >= std::time::Duration::from_secs(2),
        "min_delay {:?} exceeds 1 req/s once doubled for the ticket hop",
        policy.min_delay
    );

    // The limiter is self-extending: requests spaced 15s apart were still
    // refused 171s after tripping it, and only ~3 minutes of silence cleared
    // it. The first backoff step must therefore already out-wait that,
    // otherwise retries knock during the penalty and prolong it.
    assert!(
        policy.backoff_base >= std::time::Duration::from_secs(180),
        "backoff_base {:?} cannot out-wait the ~3 minute penalty",
        policy.backoff_base
    );
    assert!(
        policy.max_retries > 0,
        "a rate-limited host must be worth retrying"
    );

    // Burst concurrency is penalized independently of average rate, and at
    // this min_delay the pacer gates chapter starts anyway, so extra workers
    // buy no throughput and only risk overlapping requests.
    assert_eq!(
        policy.max_concurrency, 1,
        "extra workers add burst risk without throughput at this min_delay"
    );
}

#[tokio::test]
async fn khodocsach_fetch_novel_pages_the_listing_and_returns_reading_order() {
    let mut server = mockito::Server::new_async().await;
    let _book = mount_book(&mut server).await;
    let (page1, page2) = mount_listing(&mut server).await;

    let novel = Khodocsach
        .fetch_novel(&khodocsach_book_url(&server))
        .await
        .unwrap();

    assert_eq!(novel.title, "Một Truyện Thử Nghiệm");
    assert_eq!(novel.author.as_deref(), Some("Tác Giả Thử Nghiệm"));
    assert_eq!(novel.status.as_deref(), Some("Đang cập nhật"));
    assert_eq!(
        novel.cover_url.as_deref(),
        Some("https://khodocsach.com/wp-content/uploads/2026/07/mot-truyen-thu-nghiem-cover.jpg")
    );

    // The listing arrives newest-first; the index must be reading order.
    let numbers: Vec<u32> = novel.chapters.iter().map(|c| c.number).collect();
    assert_eq!(numbers, vec![1, 2, 3]);
    let titles: Vec<&str> = novel
        .chapters
        .iter()
        .map(|c| c.title.as_deref().unwrap())
        .collect();
    assert_eq!(
        titles,
        vec!["Chương 1: Một", "Chương 2: Hai", "Chương 3: Ba"]
    );

    // Locators address the chapter API by db id, not the human-facing URL,
    // and carry the novel title because the content endpoint omits it.
    let locator = &novel.chapters[0].locator;
    assert!(
        locator.starts_with(&format!("{}/wp-json/app/v1/chapters/127361?", server.url())),
        "unexpected locator: {locator}"
    );
    let round_tripped = url::Url::parse(locator).unwrap();
    assert_eq!(
        round_tripped
            .query_pairs()
            .find(|(key, _)| key == "book")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some("Một Truyện Thử Nghiệm")
    );

    page1.assert_async().await;
    page2.assert_async().await;
}

#[tokio::test]
async fn khodocsach_fetch_novel_strips_html_from_the_description() {
    let mut server = mockito::Server::new_async().await;
    let _book = mount_book(&mut server).await;
    let _listing = mount_listing(&mut server).await;

    let novel = Khodocsach
        .fetch_novel(&khodocsach_book_url(&server))
        .await
        .unwrap();

    let description = novel.description.unwrap();
    assert!(
        !description.contains('<'),
        "description must not carry markup: {description}"
    );
    assert!(description.contains("Dòng đầu của phần mô tả."));
    assert!(
        description.contains("Dòng thứ hai & ký tự đặc biệt."),
        "entities must be decoded: {description}"
    );
}

#[tokio::test]
async fn khodocsach_fetch_metadata_never_requests_the_chapter_listing() {
    let mut server = mockito::Server::new_async().await;
    let _book = mount_book(&mut server).await;
    let listing = server
        .mock("GET", "/wp-json/app/v1/books/83420/chapters")
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create_async()
        .await;

    let novel = Khodocsach
        .fetch_metadata(&khodocsach_book_url(&server))
        .await
        .unwrap();

    assert_eq!(novel.title, "Một Truyện Thử Nghiệm");
    assert!(
        novel.chapters.is_empty(),
        "metadata-only fetch must leave the index empty, got {}",
        novel.chapters.len()
    );
    listing.assert_async().await;
}

#[tokio::test]
async fn khodocsach_fetch_novel_fails_when_a_listing_page_is_unreachable() {
    let mut server = mockito::Server::new_async().await;
    let _book = mount_book(&mut server).await;
    let _page1 = server
        .mock("GET", "/wp-json/app/v1/books/83420/chapters")
        .match_query(mockito::Matcher::UrlEncoded("page".into(), "1".into()))
        .with_header("content-type", "application/json")
        .with_body(CHAPTERS_PAGE1_JSON)
        .create_async()
        .await;
    let _page2 = server
        .mock("GET", "/wp-json/app/v1/books/83420/chapters")
        .match_query(mockito::Matcher::UrlEncoded("page".into(), "2".into()))
        .with_status(500)
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_novel(&khodocsach_book_url(&server))
        .await
        .expect_err("a partial index must not be returned as success");
    assert!(
        matches!(err, SourceError::Other(_)),
        "expected an opaque failure, got {err:?}"
    );
}

#[tokio::test]
async fn khodocsach_rejects_a_url_that_is_not_a_book_page() {
    let server = mockito::Server::new_async().await;
    for path in ["", "/", "/the_genre/co-dai/", "/a/b"] {
        let url = format!("{}{path}", server.url());
        assert!(
            Khodocsach.fetch_metadata(&url).await.is_err(),
            "expected {url} to be rejected as a non-book page"
        );
    }
}

#[tokio::test]
async fn khodocsach_maps_a_403_to_client_rejected() {
    let mut server = mockito::Server::new_async().await;
    let _book = server
        .mock("GET", "/wp-json/app/v1/books/mot-truyen-thu-nghiem")
        .with_status(403)
        .with_body("forbidden")
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_metadata(&khodocsach_book_url(&server))
        .await
        .unwrap_err();
    assert!(matches!(err, SourceError::ClientRejected(_)), "got {err:?}");
}

#[tokio::test]
async fn khodocsach_maps_a_404_to_not_found() {
    let mut server = mockito::Server::new_async().await;
    let _book = server
        .mock("GET", "/wp-json/app/v1/books/mot-truyen-thu-nghiem")
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":"not_found","message":"Book not found","data":{"status":404}}"#)
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_metadata(&khodocsach_book_url(&server))
        .await
        .unwrap_err();
    assert!(matches!(err, SourceError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn khodocsach_maps_a_429_on_the_ticket_hop_to_rate_limited() {
    let mut server = mockito::Server::new_async().await;
    let _ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_status(429)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":"rate_limited","message":"Too many requests","data":{"status":429}}"#)
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            SourceError::RateLimited {
                source_name: "khodocsach",
                ..
            }
        ),
        "got {err:?}"
    );
}

#[tokio::test]
async fn khodocsach_maps_a_429_on_the_content_hop_to_rate_limited() {
    let mut server = mockito::Server::new_async().await;
    let _ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_header("content-type", "application/json")
        .with_body(TICKET_JSON)
        .create_async()
        .await;
    let _content = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::Any)
        .with_status(429)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":"rate_limited","message":"Too many requests","data":{"status":429}}"#)
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .unwrap_err();
    assert!(
        matches!(err, SourceError::RateLimited { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn khodocsach_fetch_chapter_performs_the_ticket_handshake() {
    let mut server = mockito::Server::new_async().await;
    let ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_header("content-type", "application/json")
        .with_body(TICKET_JSON)
        .expect(1)
        .create_async()
        .await;
    // The content hop must carry every field the ticket handed back.
    let content = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("nonce".into(), "1dcaaedc6f27a0d1".into()),
            mockito::Matcher::UrlEncoded("exp".into(), "1787044275".into()),
            mockito::Matcher::UrlEncoded("sig".into(), "5f26adf064d37e4047b5bf70".into()),
        ]))
        .with_header("content-type", "application/json")
        .with_body(CONTENT_JSON)
        .expect(1)
        .create_async()
        .await;

    let fetched = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .unwrap();

    assert_eq!(fetched.chapter_title, "Chương 1: Một");
    assert_eq!(
        fetched.novel_title, "Một Truyện Thử Nghiệm",
        "the novel title must survive the locator round trip: it names the output directory"
    );
    assert_eq!(
        fetched.paragraphs,
        vec![
            "Đoạn thứ nhất của chương thử nghiệm.",
            "Đoạn thứ hai, sau một dòng trống.",
            "Đoạn thứ ba và cuối cùng.",
        ],
        "blank lines and known noise must be dropped, order preserved"
    );

    ticket.assert_async().await;
    content.assert_async().await;
}

#[tokio::test]
async fn khodocsach_re_tickets_once_when_the_content_hop_rejects_the_ticket() {
    let mut server = mockito::Server::new_async().await;
    let ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_header("content-type", "application/json")
        .with_body(TICKET_JSON)
        .expect(2)
        .create_async()
        .await;
    let stale = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::Any)
        .with_status(401)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":"ticket_invalid","message":"Invalid ticket","data":{"status":401}}"#)
        .expect(1)
        .create_async()
        .await;
    let retried = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(CONTENT_JSON)
        .expect(1)
        .create_async()
        .await;

    let fetched = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .unwrap();
    assert_eq!(fetched.chapter_title, "Chương 1: Một");

    ticket.assert_async().await;
    stale.assert_async().await;
    retried.assert_async().await;
}

#[tokio::test]
async fn khodocsach_gives_up_after_one_re_ticket() {
    let mut server = mockito::Server::new_async().await;
    let ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_header("content-type", "application/json")
        .with_body(TICKET_JSON)
        .expect(2)
        .create_async()
        .await;
    let content = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::Any)
        .with_status(401)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":"ticket_invalid","message":"Invalid ticket","data":{"status":401}}"#)
        .expect(2)
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .expect_err("a persistently invalid ticket must fail rather than loop");
    assert!(
        !matches!(err, SourceError::RateLimited { .. }),
        "got {err:?}"
    );

    ticket.assert_async().await;
    content.assert_async().await;
}

#[tokio::test]
async fn khodocsach_maps_an_unreadable_chapter_to_unentitled() {
    let mut server = mockito::Server::new_async().await;
    let _ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_header("content-type", "application/json")
        .with_body(TICKET_JSON)
        .create_async()
        .await;
    let _content = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(CONTENT_JSON.replace("\"can_read\": true", "\"can_read\": false"))
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .unwrap_err();
    assert!(matches!(err, SourceError::Unentitled(_)), "got {err:?}");
}

#[tokio::test]
async fn khodocsach_fails_a_chapter_whose_content_yields_no_paragraphs() {
    let mut server = mockito::Server::new_async().await;
    let _ticket = server
        .mock("GET", "/wp-json/app/v1/chapters/127361/ticket")
        .with_header("content-type", "application/json")
        .with_body(TICKET_JSON)
        .create_async()
        .await;
    let _content = server
        .mock("GET", "/wp-json/app/v1/chapters/127361")
        .match_query(mockito::Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(CONTENT_JSON.replace(
            "Đoạn thứ nhất của chương thử nghiệm.\\n\\n   \\nĐoạn thứ hai, sau một dòng trống.\\nNhấn Mở Bình Luận để xem thêm.\\nĐoạn thứ ba và cuối cùng.",
            "   \\n\\n  ",
        ))
        .create_async()
        .await;

    let err = Khodocsach
        .fetch_chapter(&chapter_one_ref(&server))
        .await
        .expect_err("an empty body must not reach disk as an empty chapter");
    assert!(matches!(err, SourceError::Other(_)), "got {err:?}");
}

#[tokio::test]
async fn khodocsach_resolves_a_kds_permalink_to_the_bare_api_slug() {
    let mut server = mockito::Server::new_async().await;
    // The mock answers only at the bare slug, which is what the API route
    // accepts: its pattern cannot match the dot in the permalink.
    let book = mount_book(&mut server).await;

    let novel = Khodocsach
        .fetch_metadata(&format!("{}/mot-truyen-thu-nghiem.kds/", server.url()))
        .await
        .expect("a .kds permalink is the canonical book URL and must resolve");

    assert_eq!(novel.title, "Một Truyện Thử Nghiệm");
    book.assert_async().await;
}

/// Book payload in the shape the live host returns for a book with no author
/// term: an empty string rather than `null` or an object. Three of the 124
/// books on the site look like this and every one of them failed to parse
/// before the term fields became tolerant.
const AUTHORLESS_BOOK_JSON: &str = r#"{
  "id": 82402,
  "slug": "thay-doi-mo",
  "title": "Thầy Dời Mộ",
  "desc": "<p>Một đoạn mô tả.</p>",
  "cover": "https://khodocsach.com/wp-content/uploads/2024/07/cover.jpg",
  "author": "",
  "genres": [],
  "status": { "term_id": 14, "name": "Hoàn thành", "slug": "hoan-thanh" },
  "type": { "term_id": 11, "name": "Truyện chữ", "slug": "truyen-chu" },
  "total_chapter": "604"
}"#;

#[tokio::test]
async fn khodocsach_reads_a_book_whose_author_term_is_an_empty_string() {
    let mut server = mockito::Server::new_async().await;
    let _book = server
        .mock("GET", "/wp-json/app/v1/books/thay-doi-mo")
        .with_header("content-type", "application/json")
        .with_body(AUTHORLESS_BOOK_JSON)
        .create_async()
        .await;

    let novel = Khodocsach
        .fetch_metadata(&format!("{}/thay-doi-mo.kds/", server.url()))
        .await
        .expect("an absent author must not fail the whole book payload");

    assert_eq!(novel.title, "Thầy Dời Mộ");
    assert_eq!(novel.author, None);
    assert_eq!(novel.status.as_deref(), Some("Hoàn thành"));
    assert_eq!(novel.description.as_deref(), Some("Một đoạn mô tả."));
}
