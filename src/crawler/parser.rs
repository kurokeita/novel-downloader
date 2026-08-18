//! The on-disk chapter document format. Source-independent: every adapter's
//! parsed output is rendered through here, and the EPUB importer reads it
//! back.

/// Replace XML/HTML special characters with named entities. Used both for
/// chapter HTML and for EPUB-generated XML.
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Render the saved-on-disk chapter HTML document used by the EPUB importer.
/// Both titles and every paragraph are HTML-escaped before insertion.
pub fn build_html_document(
    novel_title: &str,
    chapter_title: &str,
    paragraphs: &[String],
) -> String {
    let safe_novel = escape_html(novel_title);
    let safe_chapter = escape_html(chapter_title);
    let body = paragraphs
        .iter()
        .map(|p| format!("        <p>{}</p>", escape_html(p)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "<!DOCTYPE html>\n\
<html lang=\"vi\">\n\
<head>\n\
    <meta charset=\"UTF-8\">\n\
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
    <title>{safe_chapter}</title>\n\
    <link\n        href=\"https://fonts.googleapis.com/css2?family=Literata&display=swap\"\n        rel=\"stylesheet\"\n    >\n\
    <style>\n\
        body {{\n\
            margin: 0;\n\
            padding: 0;\n\
            background: #f6f1e7;\n\
            color: #222;\n\
            font-family: \"Bookerly\", \"Literata\", \"Georgia\", \"Times New Roman\", serif;\n\
            line-height: 1.9;\n\
        }}\n\
\n\
        .container {{\n\
            max-width: 860px;\n\
            margin: 0 auto;\n\
            padding: 48px 28px 72px;\n\
        }}\n\
\n\
        .novel-title {{\n\
            text-align: center;\n\
            font-size: 1rem;\n\
            color: #666;\n\
            margin-bottom: 12px;\n\
        }}\n\
\n\
        .chapter-title {{\n\
            text-align: center;\n\
            font-size: 2.2rem;\n\
            font-weight: 700;\n\
            line-height: 1.3;\n\
            margin: 0 0 36px;\n\
        }}\n\
\n\
        .chapter-content p {{\n\
            font-size: 1.2rem;\n\
            margin: 0 0 1.15em;\n\
            text-align: justify;\n\
            text-indent: 2em;\n\
        }}\n\
    </style>\n\
</head>\n\
<body>\n\
    <div class=\"container\">\n\
        <div class=\"novel-title\">{safe_novel}</div>\n\
        <h1 class=\"chapter-title\">{safe_chapter}</h1>\n\
        <div class=\"chapter-content\">\n\
{body}\n\
        </div>\n\
    </div>\n\
</body>\n\
</html>"
    )
}
