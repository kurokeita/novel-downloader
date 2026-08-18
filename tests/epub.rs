use novel_downloader::epub::{
    BuildEpubParams, ChapterEntry, ContentOpfParams, EpubMetadataOverride, build_epub,
    chapter_xhtml, content_opf, epub_file_stem, extract_title_and_body_from_saved_chapter,
    list_chapter_files, nav_xhtml, ncx_xml, pick_cover_extension, title_page_xhtml,
};
use novel_downloader::source::metruyenhot::{
    extract_author_from_main_page, extract_cover_image_url,
    extract_novel_description_from_main_page, extract_novel_status_from_main_page,
    extract_novel_title_from_main_page,
};
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[test]
fn epub_file_stem_combines_title_and_author_preserving_unicode() {
    assert_eq!(
        epub_file_stem(
            "Người Chồng Vô Dụng Của Nữ Thần - Lâm Chính (Bản Chuẩn - Mới)",
            Some("Bạch Long")
        ),
        "Người Chồng Vô Dụng Của Nữ Thần - Lâm Chính (Bản Chuẩn - Mới) - Bạch Long"
    );
}

#[test]
fn epub_file_stem_uses_title_only_when_author_missing_or_blank() {
    assert_eq!(
        epub_file_stem("Người Chồng Vô Dụng", None),
        "Người Chồng Vô Dụng"
    );
    assert_eq!(
        epub_file_stem("Người Chồng Vô Dụng", Some("   ")),
        "Người Chồng Vô Dụng"
    );
}

#[test]
fn epub_file_stem_replaces_illegal_path_characters() {
    // Path separators and Windows-reserved characters become spaces, then
    // runs of whitespace collapse to single spaces.
    assert_eq!(epub_file_stem("A/B: C?", Some("D|E")), "A B C - D E");
}

#[test]
fn epub_file_stem_falls_back_to_book_when_empty() {
    assert_eq!(epub_file_stem("", None), "book");
    assert_eq!(epub_file_stem("///", None), "book");
}

#[test]
fn epub_metadata_override_new_rejects_blank_title() {
    assert!(EpubMetadataOverride::new("", Some("A".into())).is_none());
    assert!(EpubMetadataOverride::new("   ", None).is_none());
}

#[test]
fn epub_metadata_override_new_normalises_blank_author_to_none() {
    let m = EpubMetadataOverride::new("Title", Some("   ".into())).unwrap();
    assert_eq!(m.title, "Title");
    assert!(m.author.is_none());

    let m = EpubMetadataOverride::new("Title", Some("Author".into())).unwrap();
    assert_eq!(m.author.as_deref(), Some("Author"));
}

#[test]
fn extract_novel_title_prefers_h1_over_title_tag() {
    let html =
        "<html><head><title>Foo - truyenazz</title></head><body><h1>Cuốn Sách</h1></body></html>";
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
fn pick_cover_extension_uses_media_type_first() {
    assert_eq!(
        pick_cover_extension("https://x/cover.bin", "image/png"),
        ".png"
    );
}

#[test]
fn pick_cover_extension_falls_back_to_url_extension() {
    assert_eq!(pick_cover_extension("https://x/cover.jpeg", ""), ".jpeg");
}

#[test]
fn pick_cover_extension_defaults_to_jpg() {
    assert_eq!(pick_cover_extension("https://x/cover", ""), ".jpg");
}

#[tokio::test]
async fn list_chapter_files_returns_sorted_chapter_files() {
    let dir = tempfile::tempdir().unwrap();
    for n in [3, 1, 2] {
        let path = dir.path().join(format!("chapter_{:04}.html", n));
        tokio::fs::write(&path, b"<html></html>").await.unwrap();
    }
    tokio::fs::write(dir.path().join("notes.txt"), b"x")
        .await
        .unwrap();
    let files = list_chapter_files(dir.path()).await.unwrap();
    assert_eq!(files.len(), 3);
    assert!(files[0].ends_with("chapter_0001.html"));
    assert!(files[1].ends_with("chapter_0002.html"));
    assert!(files[2].ends_with("chapter_0003.html"));
}

#[tokio::test]
async fn list_chapter_files_errors_when_directory_empty() {
    let dir = tempfile::tempdir().unwrap();
    let err = list_chapter_files(dir.path()).await.unwrap_err();
    assert!(err.to_string().contains("No chapter"));
}

#[tokio::test]
async fn extract_title_and_body_reads_saved_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chapter_0001.html");
    let html = r#"<!DOCTYPE html>
<html><body>
  <h1 class="chapter-title">Chương 1</h1>
  <div class="chapter-content"><p>Đoạn một.</p><p>Đoạn hai.</p></div>
</body></html>"#;
    tokio::fs::write(&path, html.as_bytes()).await.unwrap();
    let parsed = extract_title_and_body_from_saved_chapter(&path)
        .await
        .unwrap();
    assert_eq!(parsed.title, "Chương 1");
    assert!(parsed.body_html.contains("<p>Đoạn một.</p>"));
}

#[tokio::test]
async fn extract_title_and_body_errors_for_invalid_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chapter_0001.html");
    tokio::fs::write(&path, b"<html></html>").await.unwrap();
    let err = extract_title_and_body_from_saved_chapter(&path)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("chapter-title"));
}

#[test]
fn chapter_xhtml_wraps_body_in_xhtml_skeleton() {
    let xhtml = chapter_xhtml("Chương 1", "<p>Hello</p>");
    assert!(xhtml.starts_with("<?xml"));
    assert!(xhtml.contains("<title>Chương 1</title>"));
    assert!(xhtml.contains("<h1>Chương 1</h1>"));
    assert!(xhtml.contains("<p>Hello</p>"));
}

#[test]
fn title_page_xhtml_includes_author_when_present() {
    let with = title_page_xhtml("Truyện X", Some("Tác giả Y"));
    assert!(with.contains("Tác giả Y"));
    let without = title_page_xhtml("Truyện X", None);
    assert!(!without.contains("Tác giả Y"));
}

#[test]
fn nav_xhtml_lists_each_chapter_as_link() {
    let xhtml = nav_xhtml(
        "N",
        &[
            ChapterEntry {
                id: "ch1".into(),
                file_name: "chapter_0001.xhtml".into(),
                title: "C1".into(),
            },
            ChapterEntry {
                id: "ch2".into(),
                file_name: "chapter_0002.xhtml".into(),
                title: "C2".into(),
            },
        ],
    );
    assert!(xhtml.contains("<a href=\"text/chapter_0001.xhtml\">C1</a>"));
    assert!(xhtml.contains("<a href=\"text/chapter_0002.xhtml\">C2</a>"));
}

#[test]
fn ncx_xml_emits_one_navpoint_per_chapter() {
    let xml = ncx_xml(
        "N",
        "https://x/",
        &[ChapterEntry {
            id: "ch1".into(),
            file_name: "chapter_0001.xhtml".into(),
            title: "C1".into(),
        }],
    );
    assert!(xml.contains("<navPoint id=\"navPoint-1\""));
    assert!(xml.contains("playOrder=\"1\""));
    assert!(xml.contains("text/chapter_0001.xhtml"));
}

#[test]
fn content_opf_includes_metadata_and_spine() {
    let opf = content_opf(ContentOpfParams {
        identifier: "https://x/".into(),
        title: "T".into(),
        author: Some("A".into()),
        include_cover: true,
        cover_ext: ".jpg".into(),
        include_font: true,
        font_file_name: "epub-font.ttf".into(),
        chapters: vec![ChapterEntry {
            id: "ch1".into(),
            file_name: "chapter_0001.xhtml".into(),
            title: "C1".into(),
        }],
    });
    assert!(opf.contains("<dc:title>T</dc:title>"));
    assert!(opf.contains("<dc:creator>A</dc:creator>"));
    assert!(opf.contains("<meta name=\"cover\" content=\"cover-image\"/>"));
    assert!(opf.contains("href=\"cover.jpg\""));
    assert!(opf.contains("href=\"fonts/epub-font.ttf\""));
    assert!(opf.contains("<itemref idref=\"ch1\""));
}

#[tokio::test]
async fn build_epub_uses_metadata_override_for_title_author_and_filename() {
    let mut server = mockito::Server::new_async().await;
    // The page advertises different metadata; the override must win.
    let main_html = r#"<html><body>
  <h1>Extracted Title</h1>
  Tác giả: Extracted Author Thể loại: Tu chân
</body></html>"#;
    let _main_mock = server
        .mock("GET", "/foo/")
        .with_status(200)
        .with_body(main_html)
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let chapter_dir = tmp.path().join("chapters");
    tokio::fs::create_dir_all(&chapter_dir).await.unwrap();
    let chapter_html = r#"<!DOCTYPE html>
<html><body>
  <h1 class="chapter-title">Chương 1</h1>
  <div class="chapter-content"><p>Hello.</p></div>
</body></html>"#;
    tokio::fs::write(
        chapter_dir.join("chapter_0001.html"),
        chapter_html.as_bytes(),
    )
    .await
    .unwrap();

    let returned = build_epub(BuildEpubParams {
        novel_main_url: format!("{}/foo/", server.url()),
        chapter_dir: chapter_dir.clone(),
        output_epub: None,
        font_path: None,
        metadata_override: Some(EpubMetadataOverride {
            title: "Override Title".to_string(),
            author: Some("Override Author".to_string()),
        }),
    })
    .await
    .unwrap();

    // Default filename derives from the overridden title + author.
    assert_eq!(
        returned.file_name().unwrap().to_string_lossy(),
        "Override Title - Override Author.epub"
    );

    let bytes = tokio::fs::read(&returned).await.unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut opf = String::new();
    archive
        .by_name("EPUB/content.opf")
        .unwrap()
        .read_to_string(&mut opf)
        .unwrap();
    assert!(
        opf.contains("<dc:title>Override Title</dc:title>"),
        "opf: {opf}"
    );
    assert!(
        opf.contains("<dc:creator>Override Author</dc:creator>"),
        "opf: {opf}"
    );
    assert!(!opf.contains("Extracted Title"));
    assert!(!opf.contains("Extracted Author"));
}

#[tokio::test]
async fn build_epub_produces_valid_zip_with_expected_entries() {
    let mut server = mockito::Server::new_async().await;
    let main_html = r#"<html><body>
  <h1>Truyện Đẹp</h1>
  Tác giả: Người Viết Thể loại: Tu chân
</body></html>"#;
    let _main_mock = server
        .mock("GET", "/foo/")
        .with_status(200)
        .with_body(main_html)
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let chapter_dir = tmp.path().join("chapters");
    tokio::fs::create_dir_all(&chapter_dir).await.unwrap();
    let chapter_html = r#"<!DOCTYPE html>
<html><body>
  <h1 class="chapter-title">Chương 1</h1>
  <div class="chapter-content"><p>Hello.</p></div>
</body></html>"#;
    tokio::fs::write(
        chapter_dir.join("chapter_0001.html"),
        chapter_html.as_bytes(),
    )
    .await
    .unwrap();

    let output = tmp.path().join("out.epub");
    let returned = build_epub(BuildEpubParams {
        novel_main_url: format!("{}/foo/", server.url()),
        chapter_dir: chapter_dir.clone(),
        output_epub: Some(output.clone()),
        font_path: None,
        metadata_override: None,
    })
    .await
    .unwrap();
    assert_eq!(returned, output);

    let bytes = tokio::fs::read(&output).await.unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n == "mimetype"));
    assert!(names.iter().any(|n| n == "META-INF/container.xml"));
    assert!(names.iter().any(|n| n == "EPUB/content.opf"));
    assert!(names.iter().any(|n| n == "EPUB/nav.xhtml"));
    assert!(names.iter().any(|n| n == "EPUB/toc.ncx"));
    assert!(names.iter().any(|n| n == "EPUB/styles/main.css"));
    assert!(names.iter().any(|n| n == "EPUB/text/titlepage.xhtml"));
    assert!(names.iter().any(|n| n == "EPUB/text/chapter_0001.xhtml"));

    let mut buf = String::new();
    {
        let mut mimetype = archive.by_name("mimetype").unwrap();
        mimetype.read_to_string(&mut buf).unwrap();
    }
    assert_eq!(buf, "application/epub+zip");

    let mut opf_text = String::new();
    {
        let mut opf = archive.by_name("EPUB/content.opf").unwrap();
        opf.read_to_string(&mut opf_text).unwrap();
    }
    assert!(opf_text.contains("<dc:creator>Người Viết</dc:creator>"));
    assert!(opf_text.contains("<dc:title>Truyện Đẹp</dc:title>"));
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
