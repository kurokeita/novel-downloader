use truyenazz_crawler::crawler::{
    build_html_document, discover_last_chapter_number, discover_last_chapter_number_from_html,
    escape_html, extract_full_chapter_text,
};

#[test]
fn escape_html_replaces_special_characters() {
    assert_eq!(
        escape_html("<a href=\"x\">'b' & c</a>"),
        "&lt;a href=&quot;x&quot;&gt;&apos;b&apos; &amp; c&lt;/a&gt;"
    );
}

#[test]
fn escape_html_preserves_safe_text() {
    assert_eq!(escape_html("plain text 1 2 3"), "plain text 1 2 3");
}

#[test]
fn extract_full_chapter_text_pulls_titles_and_paragraphs() {
    let html = r#"
<html><body>
  <div class="rv-full-story-title"><h1>Người Chồng Vô Dụng</h1></div>
  <div class="rv-chapt-title"><h2>Chương 12: Bí mật</h2></div>
  <div class="chapter-c">
    <p>Đoạn một.</p>
    <p>Đoạn hai.</p>
    <p>Bạn đang đọc truyện mới tại spam.com</p>
    <p>Đoạn ba.</p>
  </div>
</body></html>
"#;
    let chapter = extract_full_chapter_text(html).unwrap();
    assert_eq!(chapter.novel_title, "Người Chồng Vô Dụng");
    assert_eq!(chapter.chapter_title, "Chương 12: Bí mật");
    assert_eq!(
        chapter.paragraphs,
        vec!["Đoạn một.", "Đoạn hai.", "Đoạn ba."]
    );
}

#[test]
fn extract_full_chapter_text_falls_back_to_default_titles() {
    let html = r#"
<html><body>
  <div class="chapter-c">
    <p>Hello world.</p>
  </div>
</body></html>
"#;
    let chapter = extract_full_chapter_text(html).unwrap();
    assert_eq!(chapter.novel_title, "Unknown Novel");
    assert_eq!(chapter.chapter_title, "Untitled Chapter");
    assert_eq!(chapter.paragraphs, vec!["Hello world."]);
}

#[test]
fn extract_full_chapter_text_filters_css_hidden_noise_paragraphs() {
    // metruyenhot hides injected promo lines with a per-page rotating class
    // (mshow-hb, mshow-bs, ms-b, ...) whose only commonality is a
    // `display: none` rule in a <style> block. Drop any element carrying a
    // class the page hides, regardless of the class name.
    let html = r#"
<html><head><style>
  .mshow-hb { display: none; }
  .rotating-xyz { color: red; display:none }
</style></head><body>
<div class="chapter-c">
  <p>Đoạn thật một.</p>
  <p class="mshow-hb">Lên google tìm kiếm từ khóa metruyenH0t để đọc...</p>
  <p class="rotating-xyz">Bạn đang đọc truyện mới tại đâu đó.</p>
  <p>Đoạn thật hai.</p>
  <div><p class="rotating-xyz">Quảng cáo lồng trong div.</p><p>Đoạn thật ba.</p></div>
</div>
</body></html>
"#;
    let chapter = extract_full_chapter_text(html).unwrap();
    assert_eq!(
        chapter.paragraphs,
        vec!["Đoạn thật một.", "Đoạn thật hai.", "Đoạn thật ba."]
    );
}

#[test]
fn extract_full_chapter_text_keeps_paragraphs_whose_class_is_not_hidden() {
    // A class that exists in a <style> block but is NOT display:none must not
    // be filtered — only display:none classes are noise markers.
    let html = r#"
<html><head><style>.fancy { color: navy; }</style></head><body>
<div class="chapter-c">
  <p class="fancy">Đoạn được tô màu nhưng vẫn hiển thị.</p>
  <p>Đoạn thường.</p>
</div>
</body></html>
"#;
    let chapter = extract_full_chapter_text(html).unwrap();
    assert_eq!(
        chapter.paragraphs,
        vec!["Đoạn được tô màu nhưng vẫn hiển thị.", "Đoạn thường."]
    );
}

#[test]
fn extract_full_chapter_text_dedupes_consecutive_lines() {
    let html = r#"
<div class="chapter-c">
  <p>Lặp lại</p>
  <p>Lặp lại</p>
  <p>Khác</p>
</div>
"#;
    let chapter = extract_full_chapter_text(html).unwrap();
    assert_eq!(chapter.paragraphs, vec!["Lặp lại", "Khác"]);
}

#[test]
fn extract_full_chapter_text_extracts_injected_backup_content() {
    let injected = "var contentS = '<p>Đoạn ẩn 1.</p><p>Đoạn ẩn 2.</p>'; div.";
    let injected = format!("{}innerHTML = contentS;", injected);
    let html = format!(
        r#"
<html><body>
  <div class="chapter-c">
    <p>Mở đầu.</p>
    <div id="data-content-truyen-backup"></div>
    <p>Kết thúc.</p>
  </div>
  <script>{}</script>
</body></html>
"#,
        injected
    );
    let chapter = extract_full_chapter_text(&html).unwrap();
    assert_eq!(
        chapter.paragraphs,
        vec!["Mở đầu.", "Đoạn ẩn 1.", "Đoạn ẩn 2.", "Kết thúc."]
    );
}

#[test]
fn extract_full_chapter_text_errors_when_chapter_div_missing() {
    let html = "<html><body><p>nothing</p></body></html>";
    let err = extract_full_chapter_text(html).unwrap_err();
    assert!(err.to_string().contains("chapter-c"));
}

#[test]
fn build_html_document_escapes_titles_and_paragraphs() {
    let doc = build_html_document(
        "A & B",
        "<Chapter>",
        &["Hello & goodbye".to_string(), "<script>".to_string()],
    );
    assert!(doc.contains("<title>&lt;Chapter&gt;</title>"));
    assert!(doc.contains("<div class=\"novel-title\">A &amp; B</div>"));
    assert!(doc.contains("<p>Hello &amp; goodbye</p>"));
    assert!(doc.contains("<p>&lt;script&gt;</p>"));
}

#[test]
fn build_html_document_renders_chapter_title_as_h1() {
    let doc = build_html_document("N", "C1", &[]);
    assert!(doc.contains("<h1 class=\"chapter-title\">C1</h1>"));
}

#[tokio::test]
async fn discover_last_chapter_number_follows_pagination_nexts() {
    let mut server = mockito::Server::new_async().await;
    let main_url = format!("{}/foo/", server.url());
    let last_page_url = format!("{}/foo?page=48", server.url());
    let main_html = format!(
        r#"<html><body>
  <ul>
    <li><a href="/foo/chuong-1/">c1</a></li>
    <li><a href="/foo/chuong-50/">c50</a></li>
  </ul>
  <div class="pagination"><ul>
    <li class="active"><a href="javascript:void(0)">1</a></li>
    <li><a href="{last_page_url}">48</a></li>
    <li class="next"><a href="{}/foo?page=2"></a></li>
    <li class="nexts"><a href="{last_page_url}"></a></li>
  </ul></div>
</body></html>"#,
        server.url()
    );
    let last_html = r#"<html><body>
  <ul>
    <li><a href="/foo/chuong-2351/">c2351</a></li>
    <li><a href="/foo/chuong-2376/">c2376</a></li>
  </ul>
</body></html>"#;
    let _m1 = server
        .mock("GET", "/foo/")
        .with_body(&main_html)
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/foo?page=48")
        .with_body(last_html)
        .create_async()
        .await;
    let last = discover_last_chapter_number(main_url.trim_end_matches('/'))
        .await
        .unwrap();
    assert_eq!(last, 2376);
}

#[tokio::test]
async fn discover_last_chapter_number_falls_back_to_main_when_no_pagination() {
    let mut server = mockito::Server::new_async().await;
    let html = r#"<html><body>
  <ul>
    <li><a href="/foo/chuong-1/">c1</a></li>
    <li><a href="/foo/chuong-7/">c7</a></li>
    <li><a href="/foo/chuong-3/">c3</a></li>
  </ul>
</body></html>"#;
    let _mock = server
        .mock("GET", "/foo/")
        .with_body(html)
        .create_async()
        .await;
    let last = discover_last_chapter_number(&format!("{}/foo", server.url()))
        .await
        .unwrap();
    assert_eq!(last, 7);
}

#[tokio::test]
async fn discover_last_chapter_number_errors_when_no_chuong_links() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/foo/")
        .with_body("<html><body><p>nothing here</p></body></html>")
        .create_async()
        .await;
    let err = discover_last_chapter_number(&format!("{}/foo", server.url()))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("chapter"), "got: {err}");
}

#[test]
fn discover_last_chapter_number_from_html_returns_max_chuong() {
    let html = r#"<html><body>
  <a href="/foo/chuong-1/">c1</a>
  <a href="/foo/chuong-9/">c9</a>
  <a href="/foo/chuong-3/">c3</a>
</body></html>"#;
    let n = discover_last_chapter_number_from_html(html, "https://metruyenhotvn.com/foo/").unwrap();
    assert_eq!(n, 9);
}

#[test]
fn discover_last_chapter_number_from_html_errors_when_no_chuong_links() {
    let err =
        discover_last_chapter_number_from_html("<html></html>", "https://x/foo/").unwrap_err();
    assert!(err.to_string().to_lowercase().contains("chuong"));
}

#[test]
fn find_last_page_url_resolves_relative_href_against_main_url() {
    use truyenazz_crawler::crawler::find_last_page_url;
    let html = r#"<html><body><div class="pagination"><ul>
        <li class="nexts"><a href="?page=48"></a></li>
    </ul></div></body></html>"#;
    let u = find_last_page_url(html, "https://metruyenhotvn.com/foo/").unwrap();
    assert_eq!(u, "https://metruyenhotvn.com/foo/?page=48");
}

#[test]
fn find_last_page_url_returns_none_for_single_page_chapter_list() {
    use truyenazz_crawler::crawler::find_last_page_url;
    let html = r#"<html><body><a href="/foo/chuong-1/">c1</a></body></html>"#;
    assert!(find_last_page_url(html, "https://metruyenhotvn.com/foo/").is_none());
}

#[test]
fn max_chapter_in_html_ignores_chuong_links_under_a_different_novel_slug() {
    // A chuong link whose path does not start with the novel's own slug is
    // rejected — that's a link to a different novel listed on the same page.
    use truyenazz_crawler::crawler::max_chapter_in_html;
    let html = r#"<html><body>
        <a href="/different-novel/chuong-9999/">other novel</a>
        <a href="/foo/chuong-42/">c42</a>
    </body></html>"#;
    let n = max_chapter_in_html(html, "https://metruyenhotvn.com/foo/").unwrap();
    assert_eq!(n, 42);
}

#[test]
fn max_chapter_in_html_accepts_chuong_links_on_any_host_with_matching_slug() {
    // metruyenhotvn.com's paginated pages legitimately render chuong hrefs
    // on a sibling host (metruyenhotne.com) while keeping the novel slug
    // intact. As long as the slug matches, accept any host.
    use truyenazz_crawler::crawler::max_chapter_in_html;
    let html = r#"<html><body>
        <a href="https://metruyenhotne.com/foo/chuong-2351/">c2351</a>
        <a href="https://metruyenhotne.com/foo/chuong-2376/">c2376</a>
    </body></html>"#;
    let n = max_chapter_in_html(html, "https://metruyenhotvn.com/foo/").unwrap();
    assert_eq!(n, 2376);
}

/// Locks in that `extract_full_chapter_text` works on metruyenhotvn.com
/// chapter HTML without any site-specific branching — the parser is already
/// template-compatible across hosts.
#[test]
fn extract_full_chapter_text_handles_metruyenhot_chapter_fixture() {
    use truyenazz_crawler::crawler::extract_full_chapter_text;
    let html = std::fs::read_to_string("tests/fixtures/metruyenhot_chapter.html").unwrap();
    let content = extract_full_chapter_text(&html).expect("parser should accept metruyenhot");
    assert!(
        content.novel_title.contains("Vô Địch Tiên Nhân"),
        "novel title: {}",
        content.novel_title
    );
    assert!(
        content.chapter_title.contains("Chương 1"),
        "chapter title: {}",
        content.chapter_title
    );
    assert!(
        content.paragraphs.len() >= 5,
        "expected at least 5 paragraphs, got {}",
        content.paragraphs.len()
    );
}

/// Locks in that the existing `Chương Mới Nhất` sibling walk works on
/// metruyenhotvn.com novel HTML.
#[test]
fn discover_last_chapter_number_handles_metruyenhot_novel_fixture() {
    use truyenazz_crawler::crawler::discover_last_chapter_number_from_html;
    let html = std::fs::read_to_string("tests/fixtures/metruyenhot_novel.html").unwrap();
    let n = discover_last_chapter_number_from_html(
        &html,
        "https://metruyenhotvn.com/vo-dich-tien-nhan/",
    )
    .expect("discovery should accept metruyenhot");
    assert!(n >= 100, "expected a sizeable chapter count, got {n}");
}
