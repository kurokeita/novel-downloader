use std::path::PathBuf;

use crate::cli::CliOptions;
use crate::crawler::ExistingFilePolicy;
use crate::recent_fonts::RecentFont;
use crate::source::ChapterRef;
use crate::ui::plan::{CrawlMode, InteractivePlan};

pub(super) enum WizardStep {
    Welcome,
    BaseUrl,
    Mode,
    OutputRoot,
    Discover,
    Title,
    Author,
    StartChapter,
    EndChapter,
    Workers,
    Delay,
    IfExists,
    ChapterDir,
    FastSkip,
    FontChoice,
    FontPath,
    Confirm,
}

/// Outcome of running one wizard step.
pub(super) enum StepResult {
    /// Move on to the named step.
    Next(WizardStep),
    /// User pressed Ctrl+C — abort the wizard.
    Quit,
    /// User confirmed the plan; surface the resulting [`InteractivePlan`].
    Done(Box<InteractivePlan>),
}

/// Whether the user picked the bundled font, one previously remembered font,
/// or a custom file path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FontChoice {
    Default,
    Remembered(PathBuf),
    Custom,
}

/// Mutable wizard state threaded across step transitions. Defaults are seeded
/// from the parsed CLI options so that flags pre-populate the prompts.
pub(super) struct WizardState {
    pub(super) has_initial_url: bool,
    pub(super) base_url: String,
    pub(super) mode: CrawlMode,
    pub(super) output_root: PathBuf,
    pub(super) novel_title: Option<String>,
    pub(super) novel_author: Option<String>,
    pub(super) novel_cover_url: Option<String>,
    pub(super) novel_status: Option<String>,
    pub(super) novel_description: Option<String>,
    pub(super) last_discovered: Option<u32>,
    /// Full chapter index from the resolved source, sliced to the chosen
    /// range when the plan is built. Empty until discovery runs.
    pub(super) chapter_index: Vec<ChapterRef>,
    pub(super) start_chapter: u32,
    pub(super) end_chapter: u32,
    pub(super) workers: usize,
    pub(super) delay: f64,
    pub(super) if_exists: ExistingFilePolicy,
    pub(super) fast_skip: bool,
    pub(super) chapter_dir: Option<PathBuf>,
    pub(super) font_choice: FontChoice,
    pub(super) font_path: Option<PathBuf>,
    /// Validated remembered fonts, computed once on first entry to the font
    /// step so back-navigation never re-stats the filesystem.
    pub(super) recent_fonts: Option<Vec<RecentFont>>,
    pub(super) allow_any_host: bool,
}

impl WizardState {
    /// Build the initial state from CLI options, pre-filling every field
    /// with a sensible default so back-navigation never hits unset values.
    pub(super) fn seed(initial_base_url: Option<String>, options: &CliOptions) -> Self {
        let mode = if options.epub_only {
            CrawlMode::EpubOnly
        } else if options.epub {
            CrawlMode::CrawlEpub
        } else {
            CrawlMode::Crawl
        };
        Self {
            has_initial_url: initial_base_url.is_some(),
            base_url: initial_base_url.unwrap_or_default(),
            mode,
            output_root: PathBuf::from(&options.output_root),
            novel_title: None,
            novel_author: None,
            novel_cover_url: None,
            novel_status: None,
            novel_description: None,
            last_discovered: None,
            chapter_index: Vec::new(),
            start_chapter: options.start.unwrap_or(1),
            end_chapter: options.end.unwrap_or(0),
            workers: options.workers.max(1),
            delay: options.delay.max(0.0),
            if_exists: options.if_exists,
            fast_skip: options.fast_skip,
            chapter_dir: options.chapter_dir.as_ref().map(PathBuf::from),
            font_choice: if options.font_path.is_some() {
                FontChoice::Custom
            } else {
                FontChoice::Default
            },
            font_path: options.font_path.as_ref().map(PathBuf::from),
            recent_fonts: None,
            allow_any_host: options.allow_any_host,
        }
    }
}
