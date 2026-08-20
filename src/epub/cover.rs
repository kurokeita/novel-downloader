//! Cover-image helpers for the EPUB writer. Source-independent: every site
//! needs the same Content-Type-to-extension mapping.

use std::path::Path;
use url::Url;

/// Extensions a reader and `epubcheck` decode reliably. Anything outside this
/// set is treated as unusable whichever signal produced it: a `.jfif` cover
/// (a legal JPEG alias) is reported as a corrupt image.
const ALLOWED_COVER_EXTENSIONS: [&str; 6] = [".jpg", ".jpeg", ".png", ".gif", ".svg", ".webp"];

/// Canonical extension for an EPUB 3 core image media type, `None` for
/// anything else. An explicit table rather than a reverse `mime_guess` lookup,
/// which returns aliases in alphabetical order and so mapped `image/jpeg` to
/// `.jfif`.
fn canonical_extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type.trim().to_lowercase().as_str() {
        "image/jpeg" => Some(".jpg"),
        "image/png" => Some(".png"),
        "image/gif" => Some(".gif"),
        "image/svg+xml" => Some(".svg"),
        "image/webp" => Some(".webp"),
        _ => None,
    }
}

/// Pick the file extension to use for the embedded cover image: the canonical
/// extension for the response's media type, else the URL path's extension when
/// a reader recognizes it, else `.jpg`.
pub fn pick_cover_extension(cover_url: &str, media_type: &str) -> String {
    if let Some(ext) = canonical_extension_for_media_type(media_type) {
        return ext.to_string();
    }
    let url_ext = Url::parse(cover_url)
        .ok()
        .and_then(|u| {
            Path::new(u.path())
                .extension()
                .and_then(|e| e.to_str().map(|s| format!(".{}", s.to_lowercase())))
        })
        .filter(|s| ALLOWED_COVER_EXTENSIONS.contains(&s.as_str()));
    url_ext.unwrap_or_else(|| ".jpg".to_string())
}
