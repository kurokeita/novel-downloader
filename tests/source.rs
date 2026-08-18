use novel_downloader::source::registry::{normalize_host, resolve, supported_hosts, validate_url};

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
fn resolve_maps_every_supported_host_to_the_metruyenhot_adapter() {
    for host in supported_hosts() {
        let adapter = resolve(&format!("https://{host}/foo"), false).unwrap();
        assert_eq!(adapter.id(), "metruyenhot");
        assert_eq!(adapter.display_name(), "metruyenhot");
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
        vec!["metruyenhotne.com", "metruyenhotvn.com"]
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
