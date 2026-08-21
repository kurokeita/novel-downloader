//! The chapter-group endpoint the site's own reader uses.
//!
//! The novel page ships an accordion of chapter groups with empty child lists;
//! expanding one posts to `/api/api-chapters.php` and fills it from JSON. That
//! endpoint is the only complete, authoritative chapter index the site exposes,
//! so the adapter calls it directly.

use serde::Deserialize;

/// Path of the chapter-group endpoint, appended to the site origin.
pub(super) const CHAPTERS_ENDPOINT: &str = "/api/api-chapters.php";

/// Header the endpoint requires. Without it the response is `401`, and without
/// a `Referer` on the novel page it is `403`.
pub(super) const AUTH_HEADER: &str = "X-Custom-Auth";

/// Value that header carries. Captured from a live request the site's own
/// reader made on 2026-08-21; it is a constant baked into the site's scripts
/// rather than anything tied to a session or a user, and it does not appear in
/// the page source, so there is nothing to read it out of at run time. If the
/// site rotates it, every index request starts failing with `401` and the new
/// value has to be captured the same way.
pub(super) const AUTH_TOKEN: &str = "abC0000011111";

/// One chapter as the group endpoint returns it. The wire format uses
/// single-letter keys.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ChapterEntry {
    /// Address of the chapter, relative to the novel.
    #[serde(rename = "s")]
    pub(super) slug: String,
    /// The site's own chapter label, e.g. `Chương 12`.
    #[serde(rename = "n")]
    pub(super) label: String,
    /// The chapter's title, empty when it has none.
    #[serde(rename = "e", default)]
    pub(super) title: String,
}

impl ChapterEntry {
    /// Chapter title as it should reach the EPUB: the site's label, plus the
    /// chapter's own title when it has one.
    pub(super) fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            self.label.trim().to_string()
        } else {
            format!("{}: {}", self.label.trim(), self.title.trim())
        }
    }
}

/// Form body requesting one group's chapters. `from` and `to` are the position
/// bounds the accordion publishes, passed through verbatim because the final
/// group's upper bound carries an `m` suffix the endpoint understands.
pub(super) fn group_form_body(manga_id: &str, from: &str, to: &str) -> String {
    format!("manga_id={manga_id}&from={from}&to={to}&vol=")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP_A: &str = include_str!("../../../tests/fixtures/xtruyen_chapters_group_a.json");

    #[test]
    fn chapter_entries_deserialize_from_the_wire_format() {
        let entries: Vec<ChapterEntry> = serde_json::from_str(GROUP_A).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].slug, "chuong-1");
        assert_eq!(entries[0].label, "Chương 1");
        assert_eq!(entries[1].slug, "chuong-1-1");
    }

    #[test]
    fn display_title_joins_the_label_and_the_title() {
        let entries: Vec<ChapterEntry> = serde_json::from_str(GROUP_A).unwrap();
        assert_eq!(
            entries[0].display_title(),
            "Chương 1: Nhan đề thử nghiệm một"
        );
    }

    #[test]
    fn display_title_falls_back_to_the_label_alone() {
        let entry = ChapterEntry {
            slug: "chuong-9".to_string(),
            label: "Chương 9".to_string(),
            title: "   ".to_string(),
        };
        assert_eq!(entry.display_title(), "Chương 9");
    }

    #[test]
    fn a_missing_title_field_is_tolerated() {
        let entries: Vec<ChapterEntry> =
            serde_json::from_str(r#"[{"s":"chuong-1","n":"Chương 1"}]"#).unwrap();
        assert_eq!(entries[0].display_title(), "Chương 1");
    }

    #[test]
    fn group_form_body_passes_the_bounds_through_verbatim() {
        assert_eq!(
            group_form_body("1143876", "3577", "3577m"),
            "manga_id=1143876&from=3577&to=3577m&vol="
        );
    }
}
