use std::path::PathBuf;

use crate::cli::CliOptions;
use crate::crawler::ExistingFilePolicy;
use crate::recent_fonts::RecentFont;
use crate::source::{ChapterRef, RatePolicy, SiteAdapter};
use crate::ui::plan::{CrawlMode, InteractivePlan};

#[derive(Debug, PartialEq, Eq)]
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

/// Step that follows the mode select. Build-only runs derive every path from
/// the chapter directory they ask for next, so they never see the output-root
/// prompt; the download modes write beneath it and do.
pub(super) fn step_after_mode(mode: CrawlMode) -> WizardStep {
    match mode {
        CrawlMode::EpubOnly => WizardStep::ChapterDir,
        CrawlMode::Crawl | CrawlMode::CrawlEpub => WizardStep::OutputRoot,
    }
}

/// Step that follows the end-chapter prompt. A source that fixes the pacing
/// enforces its own worker count and delay whatever the user answers, so those
/// two prompts are skipped rather than asked and overridden.
pub(super) fn step_after_end_chapter(pacing_is_fixed: bool) -> WizardStep {
    if pacing_is_fixed {
        WizardStep::IfExists
    } else {
        WizardStep::Workers
    }
}

/// Where back-navigation from the existing-file prompt lands. It has to mirror
/// [`step_after_end_chapter`], or going back would land on a prompt that was
/// never shown.
pub(super) fn step_before_if_exists(pacing_is_fixed: bool) -> WizardStep {
    if pacing_is_fixed {
        WizardStep::EndChapter
    } else {
        WizardStep::Delay
    }
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
    /// Rate policy of the source this run's URL resolves to, or `None` while
    /// the URL is still unset or unsupported. Resolution is a host lookup with
    /// no I/O, so this is read where it is needed rather than cached.
    pub(super) fn rate_policy(&self) -> Option<RatePolicy> {
        crate::source::registry::resolve(&self.base_url, self.allow_any_host)
            .ok()
            .map(SiteAdapter::rate_policy)
    }

    /// Whether the resolved source fixes this run's pacing.
    pub(super) fn pacing_is_fixed(&self) -> bool {
        self.rate_policy()
            .is_some_and(|policy| policy.fixes_pacing())
    }

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

/// Tested inline because `WizardStep` and the transition are `pub(super)`:
/// an integration test under `tests/` could only reach them through a `pub`
/// re-export the wizard has no use for.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_after_end_chapter_skips_the_pacing_prompts_for_a_rate_limited_source() {
        assert_eq!(step_after_end_chapter(true), WizardStep::IfExists);
    }

    #[test]
    fn step_after_end_chapter_keeps_the_pacing_prompts_for_an_unconstrained_source() {
        assert_eq!(step_after_end_chapter(false), WizardStep::Workers);
    }

    #[test]
    fn step_before_if_exists_mirrors_the_forward_route() {
        assert_eq!(step_before_if_exists(true), WizardStep::EndChapter);
        assert_eq!(step_before_if_exists(false), WizardStep::Delay);
    }

    #[test]
    fn step_after_mode_skips_the_output_root_for_epub_only() {
        assert_eq!(step_after_mode(CrawlMode::EpubOnly), WizardStep::ChapterDir);
    }

    #[test]
    fn step_after_mode_keeps_the_output_root_for_download_modes() {
        assert_eq!(step_after_mode(CrawlMode::Crawl), WizardStep::OutputRoot);
        assert_eq!(
            step_after_mode(CrawlMode::CrawlEpub),
            WizardStep::OutputRoot
        );
    }
}
