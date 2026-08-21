use std::path::{Path, PathBuf};

use crate::crawler::ExistingFilePolicy;
use crate::source::ChapterRef;
use crate::utils::slugify;

/// Top-level operating mode chosen during the interactive flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrawlMode {
    /// Download chapters but do not build an EPUB.
    Crawl,
    /// Download chapters and then build an EPUB.
    CrawlEpub,
    /// Skip downloading and only build the EPUB from existing files.
    EpubOnly,
}

/// Directory the EPUB will be written into, or `None` when the plan builds no
/// EPUB or carries nothing to derive a directory from. The confirmation
/// summary and the run both read this, so the destination shown to the user
/// cannot disagree with the one used.
///
/// Build-only runs use the chapter directory they were given, falling back to
/// the inferred per-novel directory the non-interactive `--epub-only` path
/// relies on. Crawl-and-build ignores any chapter directory, because that mode
/// writes chapters beneath the output root and the EPUB lands with them.
pub fn epub_destination_dir(
    mode: CrawlMode,
    output_root: &Path,
    chapter_dir: Option<&Path>,
    novel_title: Option<&str>,
) -> Option<PathBuf> {
    let inferred = || novel_title.map(|title| output_root.join(slugify(title, "book")));
    match mode {
        CrawlMode::Crawl => None,
        CrawlMode::EpubOnly => match chapter_dir {
            Some(dir) => Some(dir.to_path_buf()),
            None => inferred(),
        },
        CrawlMode::CrawlEpub => inferred(),
    }
}

/// Outcome of the interactive flow when the user confirms the plan.
#[derive(Debug, Clone)]
pub struct InteractivePlan {
    /// Novel base URL.
    pub base_url: String,
    /// Operating mode chosen by the user.
    pub mode: CrawlMode,
    /// Resolved output root directory.
    pub output_root: PathBuf,
    /// Chapter range, or `None` for `EpubOnly` mode.
    pub chapter_numbers: Option<Vec<u32>>,
    /// The selected slice of the source's chapter index. Empty for
    /// `EpubOnly`, where nothing is downloaded.
    pub chapters: Vec<ChapterRef>,
    /// Sleep between successful chapter writes.
    pub delay: f64,
    /// Concurrency level.
    pub workers: usize,
    /// Whether an EPUB should be built.
    pub epub: bool,
    /// Existing chapter directory override (used by `EpubOnly`).
    pub chapter_dir: Option<PathBuf>,
    /// Optional embedded font override.
    pub font_path: Option<PathBuf>,
    /// Existing-file policy.
    pub if_exists: ExistingFilePolicy,
    /// Fast-skip flag.
    pub fast_skip: bool,
    /// Discovered novel title (for fast-skip path resolution).
    pub novel_title: Option<String>,
    /// Novel author, used as an EPUB metadata override when set.
    pub novel_author: Option<String>,
    /// Cover image URL reported by the source, passed to the EPUB writer.
    pub novel_cover_url: Option<String>,
}

/// Holds the ratatui terminal and ensures the alternate screen + raw mode
/// state are restored on drop, even on panic.
pub struct SummaryParams<'a> {
    /// Resolved novel base URL.
    pub base_url: &'a str,
    /// Display name of the source the base URL resolved to.
    pub source: &'a str,
    /// Operating mode chosen by the user.
    pub mode: CrawlMode,
    /// Output root directory.
    pub output_root: &'a std::path::Path,
    /// Chapter range, or `None` for `EpubOnly` mode.
    pub chapter_numbers: Option<&'a [u32]>,
    /// Sleep between successful chapter writes.
    pub delay: f64,
    /// Concurrency level.
    pub workers: usize,
    /// Existing-file policy.
    pub if_exists: ExistingFilePolicy,
    /// Existing chapter directory override.
    pub chapter_dir: Option<&'a std::path::Path>,
    /// Optional embedded font override.
    pub font_path: Option<&'a std::path::Path>,
    /// Fast-skip flag.
    pub fast_skip: bool,
    /// Book title that will be written to the EPUB, if known.
    pub novel_title: Option<&'a str>,
    /// Book author that will be written to the EPUB, if known.
    pub novel_author: Option<&'a str>,
    /// Whether the resolved source fixes the pacing, in which case `workers`
    /// and `delay` are the source's values rather than the user's and the
    /// summary says so.
    pub pacing_fixed_by_source: bool,
}

/// Render the plan summary text shown before confirmation.
pub fn build_summary(params: SummaryParams<'_>) -> String {
    let SummaryParams {
        base_url,
        source,
        mode,
        output_root,
        chapter_numbers,
        delay,
        workers,
        if_exists,
        chapter_dir,
        font_path,
        fast_skip,
        novel_title,
        novel_author,
        pacing_fixed_by_source,
    } = params;
    let mode_label = match mode {
        CrawlMode::Crawl => "Crawl chapters",
        CrawlMode::CrawlEpub => "Crawl chapters and build EPUB",
        CrawlMode::EpubOnly => "Build EPUB from existing chapters",
    };
    let if_exists_label = match if_exists {
        ExistingFilePolicy::Ask => "ask",
        ExistingFilePolicy::Skip => "skip",
        ExistingFilePolicy::Overwrite => "overwrite",
        ExistingFilePolicy::SkipAll => "skip-all",
    };
    let mut lines = vec![
        format!("Base URL: {}", base_url),
        format!("Source: {}", source),
        format!("Mode: {}", mode_label),
    ];
    // Build-only runs are never asked for an output root, so naming one would
    // report a directory the user never chose and the run never touches.
    if mode != CrawlMode::EpubOnly {
        lines.push(format!("Output root: {}", output_root.display()));
    }
    if let Some(dir) = epub_destination_dir(mode, output_root, chapter_dir, novel_title) {
        lines.push(format!("EPUB output: {}", dir.display()));
    }

    let has_chapter_range = matches!(chapter_numbers, Some(c) if !c.is_empty());
    if let Some(chapters) = chapter_numbers
        && let (Some(first), Some(last)) = (chapters.first(), chapters.last())
    {
        lines.push(format!(
            "Chapters: {} -> {} ({} total)",
            first,
            last,
            chapters.len()
        ));
    }

    // Always show the per-run knobs whenever a download stage is part of the
    // plan, so the user can verify their choices on one screen.
    if mode != CrawlMode::EpubOnly || has_chapter_range {
        // A source that paces itself has already overridden these two, so the
        // summary names the reason rather than showing numbers the user never
        // chose and cannot change.
        let pacing_note = if pacing_fixed_by_source {
            format!(" (required by {source})")
        } else {
            String::new()
        };
        lines.push(format!("Workers: {workers}{pacing_note}"));
        lines.push(format!("Delay: {delay}s{pacing_note}"));
        lines.push(format!("If chapter exists: {}", if_exists_label));
        lines.push(format!(
            "Fast skip: {}",
            if fast_skip { "yes" } else { "no" }
        ));
    }

    if let Some(dir) = chapter_dir {
        lines.push(format!("Chapter directory: {}", dir.display()));
    }

    let build_epub = mode != CrawlMode::Crawl;
    lines.push(format!(
        "Build EPUB: {}",
        if build_epub { "yes" } else { "no" }
    ));
    if build_epub {
        if let Some(title) = novel_title {
            lines.push(format!("Title: {}", title));
        }
        lines.push(format!("Author: {}", novel_author.unwrap_or("(none)")));
        let font_line = match font_path {
            Some(p) => format!("Font path: {}", p.display()),
            None => "Font path: default packaged font".into(),
        };
        lines.push(font_line);
    }
    lines.join("\n")
}
