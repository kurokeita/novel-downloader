//! Cover-image helpers for the EPUB writer. Source-independent: every site
//! needs the same Content-Type-to-extension mapping.

use std::path::Path;
use url::Url;

/// Pick the file extension to use for the embedded cover image, preferring
/// the value implied by the response's Content-Type, then the URL path's
/// extension, then `.jpg` as a final fallback.
pub fn pick_cover_extension(cover_url: &str, media_type: &str) -> String {
    if !media_type.is_empty()
        && let Some(exts) = mime_guess::get_mime_extensions_str(media_type)
        && let Some(first) = exts.first()
    {
        return format!(".{}", first);
    }
    let url_ext = Url::parse(cover_url)
        .ok()
        .and_then(|u| {
            Path::new(u.path())
                .extension()
                .and_then(|e| e.to_str().map(|s| format!(".{}", s.to_lowercase())))
        })
        .filter(|s| !s.is_empty() && s != ".");
    url_ext.unwrap_or_else(|| ".jpg".to_string())
}
