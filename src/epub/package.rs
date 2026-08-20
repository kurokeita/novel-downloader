use unicode_normalization::UnicodeNormalization;

use crate::crawler::escape_html;

/// XML escape — same as the crawler's `escape_html` and reused here for
/// clarity at the call sites.
fn escape_xml(text: &str) -> String {
    escape_html(text)
}

/// Split a paragraph's opening text into its drop cap letter and the text
/// that follows it.
///
/// The text is normalized to NFC first: every Vietnamese letter has a
/// precomposed form, so a letter written as a base character plus combining
/// marks collapses into one `char` and the marks cannot be stranded outside
/// the drop cap span. Returns `None` unless the opening character is a
/// letter, which is also what rejects entity references (`&quot;`), quotation
/// marks, dashes, digits, whitespace and empty text.
pub fn split_drop_cap(text: &str) -> Option<(char, String)> {
    let mut chars = text.nfc();
    let first = chars.next()?;
    if !first.is_alphabetic() {
        return None;
    }
    Some((first, chars.collect()))
}

/// One entry in the EPUB chapter manifest used by the spine/nav/ncx/opf
/// builders.
#[derive(Debug, Clone)]
pub struct ChapterEntry {
    /// Manifest id (e.g. `chapter_0001`).
    pub id: String,
    /// File name relative to `EPUB/text/` (e.g. `chapter_0001.xhtml`).
    pub file_name: String,
    /// Display title for navigation.
    pub title: String,
}

/// Rewrite the chapter body so its first paragraph opens with a drop cap,
/// returning `None` when the body does not have the shape this can safely
/// touch.
///
/// The body is edited as a string rather than reparsed: `scraper`'s tree is
/// not built for mutation, and reserializing would risk changing escaping and
/// whitespace across the whole body. The guard only matches a bare `<p>` whose
/// content starts with text, which is what the crawler emits, so anything
/// unexpected falls through untouched instead of being rewritten blindly.
fn apply_drop_cap(body_html: &str) -> Option<String> {
    const P_OPEN: &str = "<p>";
    let tag_start = body_html.find(P_OPEN)?;
    let text_start = tag_start + P_OPEN.len();
    let after_tag = &body_html[text_start..];
    let text_end = after_tag.find('<')?;
    let (cap, remainder) = split_drop_cap(&after_tag[..text_end])?;
    Some(format!(
        "{before}<p class=\"dropcap-para\"><span class=\"dropcap\">{cap}</span>{remainder}{after}",
        before = &body_html[..tag_start],
        after = &after_tag[text_end..],
    ))
}

/// Render the per-chapter XHTML used inside the EPUB.
pub fn chapter_xhtml(title: &str, body_html: &str) -> String {
    let body_html = apply_drop_cap(body_html).unwrap_or_else(|| body_html.to_string());
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"vi\" lang=\"vi\">\n\
  <head>\n\
    <title>{title_esc}</title>\n\
    <link href=\"../styles/main.css\" rel=\"stylesheet\" type=\"text/css\"/>\n\
  </head>\n\
  <body>\n\
    <h1>{title_esc}</h1>\n\
    {body}\n\
  </body>\n\
</html>",
        title_esc = escape_xml(title),
        body = body_html,
    )
}

/// Render the title page XHTML, optionally including the author below the
/// novel title.
pub fn title_page_xhtml(title: &str, author: Option<&str>) -> String {
    let author_html = author
        .map(|a| {
            format!(
                "<p style=\"text-indent:0;text-align:center;\">{}</p>",
                escape_xml(a)
            )
        })
        .unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"vi\" lang=\"vi\">\n\
  <head>\n\
    <title>{title_esc}</title>\n\
    <link href=\"../styles/main.css\" rel=\"stylesheet\" type=\"text/css\"/>\n\
  </head>\n\
  <body>\n\
    <h1>{title_esc}</h1>\n\
    {author_html}\n\
  </body>\n\
</html>",
        title_esc = escape_xml(title),
        author_html = author_html,
    )
}

/// Render the EPUB navigation document (`nav.xhtml`) — every chapter as a
/// link.
pub fn nav_xhtml(novel_title: &str, chapters: &[ChapterEntry]) -> String {
    let items = chapters
        .iter()
        .map(|c| {
            format!(
                "        <li><a href=\"text/{}\">{}</a></li>",
                c.file_name,
                escape_xml(&c.title)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"vi\" lang=\"vi\">\n\
  <head>\n\
    <title>{title_esc}</title>\n\
    <link href=\"styles/main.css\" rel=\"stylesheet\" type=\"text/css\"/>\n\
  </head>\n\
  <body>\n\
    <nav epub:type=\"toc\" id=\"toc\">\n\
      <h1>Mục lục</h1>\n\
      <ol>\n\
{items}\n\
      </ol>\n\
    </nav>\n\
  </body>\n\
</html>",
        title_esc = escape_xml(novel_title),
        items = items,
    )
}

/// Render the legacy NCX table of contents (`toc.ncx`).
pub fn ncx_xml(novel_title: &str, identifier: &str, chapters: &[ChapterEntry]) -> String {
    let nav_points = chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            format!(
                "    <navPoint id=\"navPoint-{n}\" playOrder=\"{n}\">\n\
      <navLabel><text>{title}</text></navLabel>\n\
      <content src=\"text/{file}\"/>\n\
    </navPoint>",
                n = index + 1,
                title = escape_xml(&chapter.title),
                file = chapter.file_name,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
  <head>\n\
    <meta name=\"dtb:uid\" content=\"{identifier_esc}\"/>\n\
    <meta name=\"dtb:depth\" content=\"1\"/>\n\
    <meta name=\"dtb:totalPageCount\" content=\"0\"/>\n\
    <meta name=\"dtb:maxPageNumber\" content=\"0\"/>\n\
  </head>\n\
  <docTitle><text>{title_esc}</text></docTitle>\n\
  <navMap>\n\
{nav_points}\n\
  </navMap>\n\
</ncx>",
        identifier_esc = escape_xml(identifier),
        title_esc = escape_xml(novel_title),
        nav_points = nav_points,
    )
}

/// All inputs needed to render the EPUB package document (`content.opf`).
pub struct ContentOpfParams {
    /// Stable identifier for the EPUB (we use the novel main URL).
    pub identifier: String,
    /// Display title.
    pub title: String,
    /// Optional author/creator name.
    pub author: Option<String>,
    /// Whether to include the cover image manifest entry.
    pub include_cover: bool,
    /// Cover image extension including the dot (e.g. ".jpg").
    pub cover_ext: String,
    /// Whether to include the embedded font manifest entry.
    pub include_font: bool,
    /// Embedded font file name (relative to `EPUB/fonts/`).
    pub font_file_name: String,
    /// Per-chapter manifest + spine entries.
    pub chapters: Vec<ChapterEntry>,
    /// Package modification instant as `CCYY-MM-DDThh:mm:ssZ`. Required by
    /// EPUB 3 and passed in rather than read from the clock here, so this
    /// renderer stays a pure function of its inputs.
    pub modified: String,
}

/// Render the EPUB 3 package document.
pub fn content_opf(params: ContentOpfParams) -> String {
    let author_metadata = params
        .author
        .as_ref()
        .map(|a| format!("    <dc:creator>{}</dc:creator>\n", escape_xml(a)))
        .unwrap_or_default();
    let cover_meta = if params.include_cover {
        "    <meta name=\"cover\" content=\"cover-image\"/>\n".to_string()
    } else {
        String::new()
    };
    let cover_manifest = if params.include_cover {
        let media_type = mime_guess::from_ext(params.cover_ext.trim_start_matches('.'))
            .first_raw()
            .unwrap_or("image/jpeg")
            .to_string();
        format!(
            "    <item id=\"cover-image\" href=\"cover{}\" media-type=\"{}\"/>\n",
            params.cover_ext, media_type
        )
    } else {
        String::new()
    };
    let font_manifest = if params.include_font {
        let media_type = mime_guess::from_path(&params.font_file_name)
            .first_raw()
            .unwrap_or("font/ttf")
            .to_string();
        format!(
            "    <item id=\"epub-font\" href=\"fonts/{}\" media-type=\"{}\"/>\n",
            params.font_file_name, media_type
        )
    } else {
        String::new()
    };
    let chapter_manifest = params
        .chapters
        .iter()
        .map(|c| {
            format!(
                "    <item id=\"{}\" href=\"text/{}\" media-type=\"application/xhtml+xml\"/>",
                c.id, c.file_name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let spine_items = params
        .chapters
        .iter()
        .map(|c| format!("    <itemref idref=\"{}\"/>", c.id))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"BookId\">\n\
  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
    <dc:identifier id=\"BookId\">{ident}</dc:identifier>\n\
    <dc:title>{title}</dc:title>\n\
    <dc:language>vi</dc:language>\n\
{author}    <meta property=\"dcterms:modified\">{modified}</meta>\n\
{cover_meta}  </metadata>\n\
  <manifest>\n\
    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
    <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n\
    <item id=\"style\" href=\"styles/main.css\" media-type=\"text/css\"/>\n\
    <item id=\"titlepage\" href=\"text/titlepage.xhtml\" media-type=\"application/xhtml+xml\"/>\n\
{cover_manifest}{font_manifest}{chapter_manifest}\n\
  </manifest>\n\
  <spine toc=\"ncx\">\n\
    <itemref idref=\"nav\"/>\n\
    <itemref idref=\"titlepage\"/>\n\
{spine_items}\n\
  </spine>\n\
</package>",
        ident = escape_xml(&params.identifier),
        title = escape_xml(&params.title),
        author = author_metadata,
        modified = escape_xml(&params.modified),
        cover_meta = cover_meta,
        cover_manifest = cover_manifest,
        font_manifest = font_manifest,
        chapter_manifest = chapter_manifest,
        spine_items = spine_items,
    )
}
