use anyhow::{Context, Result, anyhow};
use std::io::Write;
use std::path::PathBuf;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::font::{FontMetadata, extract_font_metadata};
use crate::utils::{download_binary, file_exists, find_font_file};

use super::chapters::{extract_title_and_body_from_saved_chapter, list_chapter_files};
use super::cover::pick_cover_extension;
use super::package::{
    ChapterEntry, ContentOpfParams, chapter_xhtml, content_opf, nav_xhtml, ncx_xml,
    title_page_xhtml,
};

/// User-supplied title/author to use verbatim instead of the metadata the
/// source reported. When present, the title and author are taken exactly
/// as given (an `author` of `None` means "no author"), so a deliberately blank
/// author is respected rather than re-filled from the page.
pub struct EpubMetadataOverride {
    /// Title used for the EPUB metadata, title page, and filename. Guaranteed
    /// non-blank by [`EpubMetadataOverride::new`].
    pub title: String,
    /// Author used for the EPUB metadata, title page, and filename.
    pub author: Option<String>,
}

impl EpubMetadataOverride {
    /// Build an override from a user-supplied title and author. Returns `None`
    /// when the title is blank, so a blank title can never bypass the
    /// source's own title and silently produce a `book.epub`. A blank author is
    /// normalised to `None` ("no author").
    pub fn new(title: impl Into<String>, author: Option<String>) -> Option<Self> {
        let title = title.into();
        if title.trim().is_empty() {
            return None;
        }
        let author = author.filter(|a| !a.trim().is_empty());
        Some(Self { title, author })
    }
}

pub struct BuildEpubParams {
    /// Main novel page URL. Used as the OPF `dc:identifier` and the NCX
    /// source URL; the EPUB writer never fetches it.
    pub novel_main_url: String,
    /// Novel title as the source reported it, used unless
    /// `metadata_override` supplies one.
    pub novel_title: String,
    /// Novel author as the source reported it, used unless
    /// `metadata_override` supplies one.
    pub novel_author: Option<String>,
    /// Absolute cover image URL the source reported, when it has one.
    pub cover_url: Option<String>,
    /// Directory containing the saved chapter HTML files.
    pub chapter_dir: PathBuf,
    /// Optional explicit output path; defaults to `<chapter_dir>/<slug>.epub`.
    pub output_epub: Option<PathBuf>,
    /// Optional override for the embedded font.
    pub font_path: Option<PathBuf>,
    /// Optional title/author override. When `None`, both come from
    /// `novel_title` / `novel_author` (the default, non-interactive
    /// behavior).
    pub metadata_override: Option<EpubMetadataOverride>,
}

/// Build the embedded stylesheet referencing the embedded font face.
fn build_main_css(font_metadata: &FontMetadata, font_file_name: &str) -> String {
    let escaped_family = font_metadata.family_name.replace('\'', "\\'");
    format!(
        "@font-face {{\n  font-family: '{family}';\n  src: url('../fonts/{font}');\n}}\n\n\
body {{\n  font-family: '{family}', serif;\n  line-height: 1.8;\n  margin: 0%;\n  padding: 0;\n}}\n\n\
h1 {{\n  text-align: center;\n  font-size: 2.2em;\n  font-weight: bold;\n  margin: 2.5em 0 1.5em 0;\n  padding: 0;\n}}\n\n\
p {{\n  margin: 0 0 0.9em 0;\n  text-indent: 2em;\n  text-align: justify;\n}}",
        family = escaped_family,
        font = font_file_name,
    )
}

/// Build the default EPUB file stem (no extension) from the novel title and
/// optional author, as the human-readable `<title> - <author>`. The original
/// Vietnamese text (diacritics, spaces, parentheses, dashes) is preserved;
/// only characters that are illegal in file names on common platforms are
/// replaced with a space and collapsed. Falls back to the title alone when
/// the author is missing or blank, and to `book` when nothing usable remains.
pub fn epub_file_stem(title: &str, author: Option<&str>) -> String {
    let combined = match author {
        Some(a) if !a.trim().is_empty() => format!("{} - {}", title.trim(), a.trim()),
        _ => title.trim().to_string(),
    };
    let sanitized: String = combined
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "book".to_string()
    } else {
        collapsed
    }
}

/// Assemble the EPUB archive at `output_epub` from previously-saved chapter
/// HTML files in `chapter_dir`. Metadata and the cover URL arrive with the
/// params, so no HTML is read here; downloads the cover, embeds the font when
/// available, and returns the final output path.
pub async fn build_epub(params: BuildEpubParams) -> Result<PathBuf> {
    let chapter_dir =
        std::fs::canonicalize(&params.chapter_dir).unwrap_or_else(|_| params.chapter_dir.clone());
    if !file_exists(&chapter_dir).await {
        return Err(anyhow!(
            "Chapter directory not found: {}",
            chapter_dir.display()
        ));
    }

    let (novel_title, author) = match &params.metadata_override {
        Some(meta) => (meta.title.clone(), meta.author.clone()),
        None => (params.novel_title.clone(), params.novel_author.clone()),
    };

    let mut cover_bytes: Option<Vec<u8>> = None;
    let mut cover_ext = ".jpg".to_string();
    let mut cover_media_type = "image/jpeg".to_string();
    if let Some(url) = &params.cover_url
        && let Ok(downloaded) = download_binary(url).await
    {
        cover_bytes = Some(downloaded.content);
        if !downloaded.content_type.is_empty() {
            cover_media_type = downloaded.content_type.clone();
        }
        cover_ext = pick_cover_extension(url, &cover_media_type);
    }

    let output_epub = params.output_epub.clone().unwrap_or_else(|| {
        chapter_dir.join(format!(
            "{}.epub",
            epub_file_stem(&novel_title, author.as_deref())
        ))
    });

    let font_path = find_font_file(params.font_path.as_deref()).await?;
    let font_bytes = match &font_path {
        Some(path) => Some(tokio::fs::read(path).await?),
        None => None,
    };
    let font_metadata = match &font_path {
        Some(path) => extract_font_metadata(path).await?,
        None => FontMetadata {
            family_name: "serif".into(),
            extension: ".ttf".into(),
        },
    };
    let embedded_font_file_name = format!("epub-font{}", font_metadata.extension);

    if let Some(parent) = output_epub.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let chapter_files = list_chapter_files(&chapter_dir).await?;
    let mut chapter_entries: Vec<ChapterEntry> = Vec::with_capacity(chapter_files.len());
    let mut chapter_xhtmls: Vec<(String, String)> = Vec::with_capacity(chapter_files.len());
    for (index, chapter_file) in chapter_files.iter().enumerate() {
        let saved = extract_title_and_body_from_saved_chapter(chapter_file).await?;
        let file_name = format!("chapter_{:04}.xhtml", index + 1);
        let id = format!("chapter_{:04}", index + 1);
        chapter_xhtmls.push((
            file_name.clone(),
            chapter_xhtml(&saved.title, &saved.body_html),
        ));
        chapter_entries.push(ChapterEntry {
            id,
            file_name,
            title: saved.title,
        });
    }

    let css = build_main_css(&font_metadata, &embedded_font_file_name);
    let opf = content_opf(ContentOpfParams {
        identifier: params.novel_main_url.clone(),
        title: novel_title.clone(),
        author: author.clone(),
        include_cover: cover_bytes.is_some(),
        cover_ext: cover_ext.clone(),
        include_font: font_bytes.is_some(),
        font_file_name: embedded_font_file_name.clone(),
        chapters: chapter_entries.clone(),
    });
    let nav = nav_xhtml(&novel_title, &chapter_entries);
    let ncx = ncx_xml(&novel_title, &params.novel_main_url, &chapter_entries);
    let titlepage = title_page_xhtml(&novel_title, author.as_deref());

    let output_epub_clone = output_epub.clone();
    let cover_bytes_clone = cover_bytes;
    let cover_ext_clone = cover_ext;
    let font_bytes_clone = font_bytes;
    let embedded_font_file_name_clone = embedded_font_file_name;

    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::create(&output_epub_clone)
            .with_context(|| format!("failed to create {}", output_epub_clone.display()))?;
        let mut zip = zip::ZipWriter::new(file);

        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored)?;
        zip.write_all(b"application/epub+zip")?;

        let deflated =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("META-INF/container.xml", deflated)?;
        zip.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n  <rootfiles>\n    <rootfile full-path=\"EPUB/content.opf\" media-type=\"application/oebps-package+xml\"/>\n  </rootfiles>\n</container>")?;

        zip.start_file("EPUB/styles/main.css", deflated)?;
        zip.write_all(css.as_bytes())?;

        if let Some(bytes) = &font_bytes_clone {
            zip.start_file(format!("EPUB/fonts/{}", embedded_font_file_name_clone), deflated)?;
            zip.write_all(bytes)?;
        }

        if let Some(bytes) = &cover_bytes_clone {
            zip.start_file(format!("EPUB/cover{}", cover_ext_clone), deflated)?;
            zip.write_all(bytes)?;
        }

        for (file_name, body) in &chapter_xhtmls {
            zip.start_file(format!("EPUB/text/{}", file_name), deflated)?;
            zip.write_all(body.as_bytes())?;
        }

        zip.start_file("EPUB/text/titlepage.xhtml", deflated)?;
        zip.write_all(titlepage.as_bytes())?;

        zip.start_file("EPUB/nav.xhtml", deflated)?;
        zip.write_all(nav.as_bytes())?;

        zip.start_file("EPUB/toc.ncx", deflated)?;
        zip.write_all(ncx.as_bytes())?;

        zip.start_file("EPUB/content.opf", deflated)?;
        zip.write_all(opf.as_bytes())?;

        zip.finish()?;
        Ok(())
    })
    .await
    .map_err(|e| anyhow!("epub writer task panicked: {}", e))??;

    Ok(output_epub)
}
