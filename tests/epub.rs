use novel_downloader::epub::{
    BuildEpubParams, ChapterEntry, ContentOpfParams, EpubMetadataOverride, build_epub,
    chapter_xhtml, content_opf, epub_file_stem, extract_title_and_body_from_saved_chapter,
    list_chapter_files, nav_xhtml, ncx_xml, pick_cover_extension, split_drop_cap, title_page_xhtml,
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

#[test]
fn pick_cover_extension_maps_jpeg_to_jpg() {
    // `.jfif` is a legal JPEG alias that validators refuse to decode.
    let ext = pick_cover_extension("https://x/cover", "image/jpeg");
    assert_eq!(ext, ".jpg");
    assert_ne!(ext, ".jfif");
}

#[test]
fn pick_cover_extension_maps_core_image_types() {
    for (media_type, expected) in [
        ("image/png", ".png"),
        ("image/gif", ".gif"),
        ("image/svg+xml", ".svg"),
        ("image/webp", ".webp"),
    ] {
        assert_eq!(
            pick_cover_extension("https://x/cover", media_type),
            expected
        );
    }
}

#[test]
fn pick_cover_extension_rejects_unrecognized_url_extension() {
    assert_eq!(pick_cover_extension("https://x/cover.jfif", ""), ".jpg");
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
    assert!(xhtml.contains("<p class=\"dropcap-para\"><span class=\"dropcap\">H</span>ello</p>"));
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
        modified: "2026-08-20T08:42:00Z".into(),
    });
    assert!(opf.contains("<dc:title>T</dc:title>"));
    assert!(opf.contains("<dc:creator>A</dc:creator>"));
    assert!(opf.contains("<meta name=\"cover\" content=\"cover-image\"/>"));
    assert!(opf.contains("href=\"cover.jpg\""));
    assert!(opf.contains("href=\"fonts/epub-font.ttf\""));
    assert!(opf.contains("<itemref idref=\"ch1\""));
}

#[test]
fn content_opf_declares_dcterms_modified_exactly_once() {
    let opf = content_opf(ContentOpfParams {
        identifier: "https://x/".into(),
        title: "T".into(),
        author: None,
        include_cover: false,
        cover_ext: ".jpg".into(),
        include_font: false,
        font_file_name: "epub-font.ttf".into(),
        chapters: vec![ChapterEntry {
            id: "ch1".into(),
            file_name: "chapter_0001.xhtml".into(),
            title: "C1".into(),
        }],
        modified: "2026-08-20T08:42:00Z".into(),
    });
    let entry = "<meta property=\"dcterms:modified\">2026-08-20T08:42:00Z</meta>";
    assert_eq!(opf.matches(entry).count(), 1);
    let metadata = opf
        .split_once("<metadata")
        .and_then(|(_, rest)| rest.split_once("</metadata>"))
        .map(|(inner, _)| inner)
        .expect("package document has a metadata element");
    assert!(metadata.contains(entry));
}

#[tokio::test]
async fn build_epub_uses_metadata_override_for_title_author_and_filename() {
    // The source reported different metadata; the override must win.
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
        novel_main_url: "https://example.test/foo/".to_string(),
        novel_title: "Source Title".to_string(),
        novel_author: Some("Source Author".to_string()),
        cover_url: None,
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
    assert!(!opf.contains("Source Title"));
    assert!(!opf.contains("Source Author"));
}

#[tokio::test]
async fn build_epub_produces_valid_zip_with_expected_entries() {
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
        novel_main_url: "https://example.test/foo/".to_string(),
        novel_title: "Truyện Đẹp".to_string(),
        novel_author: Some("Người Viết".to_string()),
        cover_url: None,
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

#[tokio::test]
async fn build_epub_stamps_a_whole_second_utc_modification_timestamp() {
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
    build_epub(BuildEpubParams {
        novel_main_url: "https://example.test/foo/".to_string(),
        novel_title: "T".to_string(),
        novel_author: None,
        cover_url: None,
        chapter_dir,
        output_epub: Some(output.clone()),
        font_path: None,
        metadata_override: None,
    })
    .await
    .unwrap();

    let bytes = tokio::fs::read(&output).await.unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut opf_text = String::new();
    {
        let mut opf = archive.by_name("EPUB/content.opf").unwrap();
        opf.read_to_string(&mut opf_text).unwrap();
    }

    let open = "<meta property=\"dcterms:modified\">";
    assert_eq!(opf_text.matches(open).count(), 1, "opf: {opf_text}");
    let value = opf_text
        .split_once(open)
        .and_then(|(_, rest)| rest.split_once("</meta>"))
        .map(|(value, _)| value)
        .expect("package document declares dcterms:modified");
    // CCYY-MM-DDThh:mm:ssZ: no fractional seconds, no numeric offset.
    assert_eq!(value.len(), 20, "timestamp: {value}");
    assert!(value.ends_with('Z'), "timestamp: {value}");
    let digits_and_separators =
        value
            .chars()
            .zip("0000-00-00T00:00:00Z".chars())
            .all(|(actual, shape)| match shape {
                '0' => actual.is_ascii_digit(),
                expected => actual == expected,
            });
    assert!(digits_and_separators, "timestamp: {value}");
}

#[test]
fn split_drop_cap_splits_a_letter_opening_from_the_rest() {
    assert_eq!(
        split_drop_cap("Sương mù phủ kín thung lũng"),
        Some(('S', "ương mù phủ kín thung lũng".to_string()))
    );
}

#[test]
fn split_drop_cap_rejects_openings_that_are_not_letters() {
    assert_eq!(split_drop_cap("\"Ngươi là ai?\" hắn hỏi"), None);
    assert_eq!(split_drop_cap("- Ngươi là ai?"), None);
    assert_eq!(split_drop_cap("&quot;Ngươi là ai?&quot;"), None);
    assert_eq!(split_drop_cap("1954 là năm ấy"), None);
    assert_eq!(split_drop_cap(" Sương mù phủ kín"), None);
    assert_eq!(split_drop_cap(""), None);
}

#[test]
fn chapter_xhtml_marks_the_first_paragraph_with_a_drop_cap() {
    let xhtml = chapter_xhtml("Chương 1", "<p>Sương mù phủ kín thung lũng</p>");
    assert!(xhtml.contains(
        "<p class=\"dropcap-para\"><span class=\"dropcap\">S</span>ương mù phủ kín thung lũng</p>"
    ));
}

#[test]
fn chapter_xhtml_drop_caps_only_the_first_paragraph() {
    let xhtml = chapter_xhtml(
        "Chương 1",
        "<p>Sương mù phủ kín thung lũng</p>\n<p>Hắn bước ra khỏi cửa</p>\n<p>Trời đã sáng</p>",
    );
    assert_eq!(xhtml.matches("class=\"dropcap\"").count(), 1);
    assert_eq!(xhtml.matches("class=\"dropcap-para\"").count(), 1);
    assert!(xhtml.contains("<p>Hắn bước ra khỏi cửa</p>"));
    assert!(xhtml.contains("<p>Trời đã sáng</p>"));
}

#[test]
fn chapter_xhtml_leaves_a_dialogue_opening_alone() {
    let body = "<p>\"Ngươi là ai?\" hắn hỏi</p>\n<p>Không ai trả lời</p>";
    let xhtml = chapter_xhtml("Chương 1", body);
    assert!(!xhtml.contains("dropcap"));
    assert!(xhtml.contains(body));
}

#[tokio::test]
async fn build_epub_drop_caps_the_chapter_but_not_the_title_page() {
    let tmp = tempfile::tempdir().unwrap();
    let chapter_dir = tmp.path().join("chapters");
    tokio::fs::create_dir_all(&chapter_dir).await.unwrap();
    let chapter_html = r#"<!DOCTYPE html>
<html><body>
  <h1 class="chapter-title">Chương 1</h1>
  <div class="chapter-content"><p>Sương mù phủ kín thung lũng</p></div>
</body></html>"#;
    tokio::fs::write(
        chapter_dir.join("chapter_0001.html"),
        chapter_html.as_bytes(),
    )
    .await
    .unwrap();

    let output = tmp.path().join("out.epub");
    build_epub(BuildEpubParams {
        novel_main_url: "https://example.test/foo/".to_string(),
        novel_title: "Truyện Đẹp".to_string(),
        novel_author: Some("Người Viết".to_string()),
        cover_url: None,
        chapter_dir: chapter_dir.clone(),
        output_epub: Some(output.clone()),
        font_path: None,
        metadata_override: None,
    })
    .await
    .unwrap();

    let bytes = tokio::fs::read(&output).await.unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();

    let mut chapter = String::new();
    archive
        .by_name("EPUB/text/chapter_0001.xhtml")
        .unwrap()
        .read_to_string(&mut chapter)
        .unwrap();
    assert!(chapter.contains(
        "<p class=\"dropcap-para\"><span class=\"dropcap\">S</span>ương mù phủ kín thung lũng</p>"
    ));

    let mut title_page = String::new();
    archive
        .by_name("EPUB/text/titlepage.xhtml")
        .unwrap()
        .read_to_string(&mut title_page)
        .unwrap();
    assert!(!title_page.contains("dropcap"));
}

#[tokio::test]
async fn build_epub_stylesheet_carries_the_drop_cap_and_toc_list_rules() {
    let tmp = tempfile::tempdir().unwrap();
    let chapter_dir = tmp.path().join("chapters");
    tokio::fs::create_dir_all(&chapter_dir).await.unwrap();
    let chapter_html = r#"<!DOCTYPE html>
<html><body>
  <h1 class="chapter-title">Chương 1</h1>
  <div class="chapter-content"><p>Sương mù phủ kín thung lũng</p></div>
</body></html>"#;
    tokio::fs::write(
        chapter_dir.join("chapter_0001.html"),
        chapter_html.as_bytes(),
    )
    .await
    .unwrap();

    let output = tmp.path().join("out.epub");
    build_epub(BuildEpubParams {
        novel_main_url: "https://example.test/foo/".to_string(),
        novel_title: "Truyện Đẹp".to_string(),
        novel_author: Some("Người Viết".to_string()),
        cover_url: None,
        chapter_dir: chapter_dir.clone(),
        output_epub: Some(output.clone()),
        font_path: None,
        metadata_override: None,
    })
    .await
    .unwrap();

    let bytes = tokio::fs::read(&output).await.unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut css = String::new();
    archive
        .by_name("EPUB/styles/main.css")
        .unwrap()
        .read_to_string(&mut css)
        .unwrap();

    assert!(css.contains("p.dropcap-para"));
    assert!(css.contains("text-indent: 0;"));
    assert!(css.contains(".dropcap {"));
    assert!(css.contains("float: left;"));
    assert!(css.contains("nav ol {"));
    assert!(css.contains("padding-left: 4em;"));
}

#[test]
fn title_page_xhtml_carries_no_drop_cap() {
    let xhtml = title_page_xhtml("Truyện X", Some("Tác giả Y"));
    assert!(!xhtml.contains("dropcap"));
}

#[test]
fn chapter_xhtml_leaves_a_body_without_a_paragraph_alone() {
    let body = "<div>Sương mù phủ kín thung lũng</div>";
    let xhtml = chapter_xhtml("Chương 1", body);
    assert!(!xhtml.contains("dropcap"));
    assert!(xhtml.contains(body));
}

#[test]
fn split_drop_cap_composes_a_decomposed_letter_into_one_character() {
    let decomposed = "E\u{0302}\u{0301}m đềm trôi qua";
    assert_eq!(
        split_drop_cap(decomposed),
        Some(('\u{1EBE}', "m đềm trôi qua".to_string()))
    );
}
