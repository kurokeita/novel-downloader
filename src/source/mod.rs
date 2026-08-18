//! Source-side addressing types shared by the crawler pipeline.
//!
//! Today this holds only [`ChapterRef`], the opaque chapter locator that
//! replaces `base_url` + `chapter_number` arithmetic across the pipeline.
//! The `SiteAdapter` trait, `Novel` and `RatePolicy` join it here later.

/// One chapter in a novel's index.
///
/// `locator` is whatever the owning source needs to fetch the chapter: a
/// full URL for metruyenhot, an opaque id elsewhere. Nothing outside the
/// source that produced it may interpret it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterRef {
    /// One-based position in the index. Drives the `chapter_NNNN.html`
    /// output file name and every progress event.
    pub number: u32,
    /// Chapter title when the index already carries it. metruyenhot learns
    /// the title only on fetch, so its refs leave this `None`.
    pub title: Option<String>,
    /// Source-owned address used to fetch this chapter.
    pub locator: String,
}
