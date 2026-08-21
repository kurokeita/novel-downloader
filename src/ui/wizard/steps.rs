use anyhow::Result;
use std::path::PathBuf;

use crate::crawler::ExistingFilePolicy;
use crate::source::ChapterRef;
use crate::ui::PromptOutcome;
use crate::ui::plan::{CrawlMode, InteractivePlan, SummaryParams, build_summary};
use crate::ui::screens::{
    run_confirm, run_loading_screen, run_path_prompt, run_select, run_text_prompt, show_note,
};
use crate::ui::widgets::{Select, SelectOption, Validator, expand_tilde};

use super::state::{
    FontChoice, StepResult, WizardState, WizardStep, step_after_end_chapter, step_after_mode,
    step_before_if_exists,
};

macro_rules! advance_or_back {
    ($outcome:expr, $previous:expr, |$value:ident| $on_submit:block) => {
        match $outcome {
            PromptOutcome::Submitted($value) => $on_submit,
            PromptOutcome::Back => Ok(StepResult::Next($previous)),
            PromptOutcome::Quit => Ok(StepResult::Quit),
        }
    };
}

/// Welcome screen. Esc cancels the wizard since there is no earlier step.
pub(super) fn step_welcome(_state: &mut WizardState) -> Result<StepResult> {
    match show_note(
        "novel-downloader",
        "Welcome — let's set up the crawl.\n\nPress Enter to continue, Esc/Ctrl+C to quit.",
    )? {
        PromptOutcome::Submitted(()) => Ok(StepResult::Next(WizardStep::BaseUrl)),
        PromptOutcome::Back | PromptOutcome::Quit => Ok(StepResult::Quit),
    }
}

/// Novel base URL prompt. Skipped entirely if the URL was supplied on the CLI.
pub(super) fn step_base_url(state: &mut WizardState) -> Result<StepResult> {
    if state.has_initial_url {
        return Ok(StepResult::Next(WizardStep::Mode));
    }
    let allow_any_host = state.allow_any_host;
    let validator: Validator = Box::new(move |value: &str| {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || !(trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        {
            return Some("Enter a valid http:// or https:// URL.".to_string());
        }
        crate::source::registry::validate_url(trimmed, allow_any_host)
    });
    let outcome = run_text_prompt(
        "Novel base URL",
        "Paste the novel base URL.",
        Some(state.base_url.clone()).filter(|s| !s.is_empty()),
        Some("https://metruyenhotvn.com/your-novel"),
        Some(validator),
    )?;
    advance_or_back!(outcome, WizardStep::Welcome, |value| {
        state.base_url = value.trim().to_string();
        // Invalidate any previously cached discovery whenever the URL changes.
        state.novel_title = None;
        state.last_discovered = None;
        state.chapter_index = Vec::new();
        Ok(StepResult::Next(WizardStep::Mode))
    })
}

/// Operating-mode select.
pub(super) fn step_mode(state: &mut WizardState) -> Result<StepResult> {
    let mode_options = vec![
        SelectOption {
            label: "Crawl chapters".into(),
            value: CrawlMode::Crawl,
            hint: None,
        },
        SelectOption {
            label: "Crawl chapters and build an EPUB".into(),
            value: CrawlMode::CrawlEpub,
            hint: None,
        },
        SelectOption {
            label: "Build an EPUB from existing chapter files".into(),
            value: CrawlMode::EpubOnly,
            hint: None,
        },
    ];
    let outcome = run_select(
        "Mode",
        "What do you want to do?",
        Select::with_initial(mode_options, &state.mode),
    )?;
    advance_or_back!(outcome, WizardStep::BaseUrl, |chosen| {
        state.mode = chosen;
        Ok(StepResult::Next(step_after_mode(state.mode)))
    })
}

/// Output root prompt. Offers path autocomplete and creates the directory
/// (and any missing parents) when it does not exist yet.
pub(super) fn step_output_root(state: &mut WizardState) -> Result<StepResult> {
    let outcome = run_path_prompt(
        "Output root",
        "Where should chapter files (and the EPUB) be saved? Tab to autocomplete.",
        Some(state.output_root.to_string_lossy().into_owned()),
    )?;
    advance_or_back!(outcome, WizardStep::Mode, |value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            show_note("Output root", "Enter an output directory.")?;
            return Ok(StepResult::Next(WizardStep::OutputRoot));
        }
        let path = PathBuf::from(expand_tilde(trimmed).as_ref());
        if let Err(error) = std::fs::create_dir_all(&path) {
            show_note(
                "Output root",
                &format!("Could not create {}:\n{error}", path.display()),
            )?;
            return Ok(StepResult::Next(WizardStep::OutputRoot));
        }
        state.output_root = path;
        Ok(StepResult::Next(WizardStep::Discover))
    })
}

/// Slice a source's chapter index down to the chosen chapter numbers.
/// Returns empty when no range was chosen (`EpubOnly`) or when discovery
/// never produced an index.
fn select_chapters(index: &[ChapterRef], numbers: Option<&[u32]>) -> Vec<ChapterRef> {
    let Some(numbers) = numbers else {
        return Vec::new();
    };
    index
        .iter()
        .filter(|chapter| numbers.contains(&chapter.number))
        .cloned()
        .collect()
}

/// Aggregated novel metadata pulled out of the main page during discovery.
struct DiscoveredNovel {
    title: Option<String>,
    chapters: Vec<ChapterRef>,
    author: Option<String>,
    cover_url: Option<String>,
    last_chapter: Option<u32>,
    status: Option<String>,
    description: Option<String>,
}

/// Run the title + status + description + last-chapter discovery under a
/// styled loading screen, then show a brief novel-info note.
pub(super) async fn step_discover(state: &mut WizardState) -> Result<StepResult> {
    let url = state.base_url.clone();
    let allow_any_host = state.allow_any_host;
    let outcome = run_loading_screen(
        "Discovering novel",
        "Fetching main page and detecting latest chapter…",
        async move {
            let adapter = crate::source::registry::resolve(&url, allow_any_host)
                .map_err(|error| error.to_string())?;
            let novel = adapter
                .fetch_novel(&url)
                .await
                .map_err(|error| format!("Could not read {url}:\n{error}"))?;
            Ok::<DiscoveredNovel, String>(DiscoveredNovel {
                title: Some(novel.title),
                author: novel.author,
                cover_url: novel.cover_url,
                last_chapter: novel.chapters.last().map(|chapter| chapter.number),
                chapters: novel.chapters,
                status: novel.status,
                description: novel.description,
            })
        },
    )
    .await?;
    let novel = match outcome {
        PromptOutcome::Submitted(Ok(novel)) => novel,
        // The fetch failed: tell the user explicitly instead of advancing with
        // silently-empty metadata, then let them enter values by hand.
        PromptOutcome::Submitted(Err(message)) => {
            return match show_note(
                "Discovery failed",
                &format!(
                    "{message}\n\nYou can still enter the title, author, and chapter range manually."
                ),
            )? {
                PromptOutcome::Submitted(()) => Ok(StepResult::Next(WizardStep::Title)),
                PromptOutcome::Back => Ok(StepResult::Next(WizardStep::OutputRoot)),
                PromptOutcome::Quit => Ok(StepResult::Quit),
            };
        }
        PromptOutcome::Back => return Ok(StepResult::Next(WizardStep::OutputRoot)),
        PromptOutcome::Quit => return Ok(StepResult::Quit),
    };
    state.novel_title = novel.title;
    state.novel_author = novel.author;
    state.novel_cover_url = novel.cover_url;
    state.last_discovered = novel.last_chapter;
    state.chapter_index = novel.chapters;
    state.novel_status = novel.status;
    state.novel_description = novel.description;

    if state.novel_title.is_some() || state.last_discovered.is_some() {
        let mut lines: Vec<String> = Vec::new();
        if let Some(title) = state.novel_title.as_ref() {
            lines.push(format!("Title: {}", title));
        }
        if let Some(author) = state.novel_author.as_ref() {
            lines.push(format!("Author: {}", author));
        }
        if let Some(status) = state.novel_status.as_ref() {
            lines.push(format!("Status: {}", status));
        }
        lines.extend(crate::ui::plan::chapter_summary_lines(
            state.chapter_index.len(),
            state.last_discovered,
            state
                .chapter_index
                .last()
                .and_then(|chapter| chapter.title.as_deref()),
        ));
        if let Some(desc) = state.novel_description.as_ref() {
            lines.push(String::new());
            lines.push("Description:".to_string());
            lines.push(desc.clone());
        }
        match show_note("Novel", &lines.join("\n"))? {
            PromptOutcome::Submitted(()) => {}
            PromptOutcome::Back => return Ok(StepResult::Next(WizardStep::OutputRoot)),
            PromptOutcome::Quit => return Ok(StepResult::Quit),
        }
    }
    Ok(StepResult::Next(WizardStep::Title))
}

/// Book-title prompt, pre-filled with the title discovered from the web. In
/// EPUB-only mode the wizard skips discovery, so this step fetches the main
/// page once to pre-fill the title and author before prompting.
pub(super) async fn step_title(state: &mut WizardState) -> Result<StepResult> {
    let back = if state.mode == CrawlMode::EpubOnly {
        WizardStep::ChapterDir
    } else {
        WizardStep::Discover
    };

    // Only EPUB-only mode reaches this step without having run discovery, so
    // it is the only path that needs a pre-fill fetch - metadata only, since
    // EPUB-only never needs the chapter index. In crawl modes the
    // title/author are already populated (or left blank after a reported
    // discovery failure), so we never re-fetch here.
    if state.mode == CrawlMode::EpubOnly && state.novel_title.is_none() {
        let url = state.base_url.clone();
        let allow_any_host = state.allow_any_host;
        let outcome = run_loading_screen(
            "Fetching novel info",
            "Reading the title and author from the main page…",
            async move {
                let adapter = crate::source::registry::resolve(&url, allow_any_host)
                    .map_err(|error| error.to_string())?;
                let novel = adapter
                    .fetch_metadata(&url)
                    .await
                    .map_err(|error| format!("Could not read {url}:\n{error}"))?;
                Ok::<(String, Option<String>, Option<String>), String>((
                    novel.title,
                    novel.author,
                    novel.cover_url,
                ))
            },
        )
        .await?;
        match outcome {
            PromptOutcome::Submitted(Ok((title, author, cover_url))) => {
                state.novel_title = Some(title);
                state.novel_author = author;
                state.novel_cover_url = cover_url;
            }
            // Surface the failure; the user can still type the title by hand.
            PromptOutcome::Submitted(Err(message)) => {
                match show_note(
                    "Could not read novel info",
                    &format!("{message}\n\nYou can still enter the title and author manually."),
                )? {
                    PromptOutcome::Submitted(()) => {}
                    PromptOutcome::Back => return Ok(StepResult::Next(back)),
                    PromptOutcome::Quit => return Ok(StepResult::Quit),
                }
            }
            PromptOutcome::Back => return Ok(StepResult::Next(back)),
            PromptOutcome::Quit => return Ok(StepResult::Quit),
        }
    }

    let validator: Validator = Box::new(|value: &str| {
        if value.trim().is_empty() {
            Some("Enter a book title.".to_string())
        } else {
            None
        }
    });
    let outcome = run_text_prompt(
        "Book title",
        "Title used for the EPUB metadata, title page, and filename.",
        state.novel_title.clone().filter(|s| !s.is_empty()),
        None,
        Some(validator),
    )?;
    advance_or_back!(outcome, back, |value| {
        state.novel_title = Some(value.trim().to_string());
        Ok(StepResult::Next(WizardStep::Author))
    })
}

/// Author prompt, pre-filled with the author discovered from the web. A blank
/// value is kept as "no author" rather than re-extracted at build time.
pub(super) fn step_author(state: &mut WizardState) -> Result<StepResult> {
    let next = if state.mode == CrawlMode::EpubOnly {
        WizardStep::FontChoice
    } else {
        WizardStep::StartChapter
    };
    let outcome = run_text_prompt(
        "Author",
        "Author name for the EPUB (leave blank if unknown).",
        state.novel_author.clone().filter(|s| !s.is_empty()),
        None,
        None,
    )?;
    advance_or_back!(outcome, WizardStep::Title, |value| {
        let trimmed = value.trim();
        state.novel_author = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        Ok(StepResult::Next(next))
    })
}

/// Start chapter prompt.
pub(super) fn step_start_chapter(state: &mut WizardState) -> Result<StepResult> {
    let validator: Validator = Box::new(|v: &str| match v.trim().parse::<u32>() {
        Ok(n) if n > 0 => None,
        _ => Some("Enter a positive integer.".into()),
    });
    let initial = if state.start_chapter == 0 {
        1
    } else {
        state.start_chapter
    };
    let outcome = run_text_prompt(
        "Start chapter",
        "First chapter to download.",
        Some(initial.to_string()),
        None,
        Some(validator),
    )?;
    advance_or_back!(outcome, WizardStep::Author, |value| {
        state.start_chapter = value.trim().parse().unwrap_or(1);
        Ok(StepResult::Next(WizardStep::EndChapter))
    })
}

/// End chapter prompt.
pub(super) fn step_end_chapter(state: &mut WizardState) -> Result<StepResult> {
    let initial = if state.end_chapter > 0 {
        state.end_chapter
    } else {
        state
            .last_discovered
            .unwrap_or_else(|| state.start_chapter.max(1))
    };
    let validator: Validator = Box::new(|v: &str| match v.trim().parse::<u32>() {
        Ok(n) if n > 0 => None,
        _ => Some("Enter a positive integer.".into()),
    });
    let outcome = run_text_prompt(
        "End chapter",
        "Last chapter to download (inclusive).",
        Some(initial.to_string()),
        None,
        Some(validator),
    )?;
    advance_or_back!(outcome, WizardStep::StartChapter, |value| {
        let parsed: u32 = value.trim().parse().unwrap_or(state.start_chapter);
        state.end_chapter = parsed;
        if let Some(message) = crate::cli::validate_chapter_range(state.start_chapter, parsed) {
            // Show the validation error and then fall back to the prior step
            // so the user can pick a valid range.
            let _ = show_note("Invalid range", &message)?;
            return Ok(StepResult::Next(WizardStep::StartChapter));
        }
        // A source that paces itself overrides whatever the user would answer,
        // so its own numbers are written in here and the two prompts skipped.
        if let Some(policy) = state.rate_policy()
            && policy.fixes_pacing()
        {
            state.workers = policy.effective_workers(state.workers);
            state.delay = policy.effective_delay(state.delay);
        }
        Ok(StepResult::Next(step_after_end_chapter(
            state.pacing_is_fixed(),
        )))
    })
}

/// Workers prompt.
pub(super) fn step_workers(state: &mut WizardState) -> Result<StepResult> {
    let validator: Validator = Box::new(|v: &str| match v.trim().parse::<usize>() {
        Ok(n) if n > 0 => None,
        _ => Some("Enter a positive integer.".into()),
    });
    let outcome = run_text_prompt(
        "Workers",
        "How many download workers should run in parallel?",
        Some(state.workers.to_string()),
        None,
        Some(validator),
    )?;
    advance_or_back!(outcome, WizardStep::EndChapter, |value| {
        state.workers = value.trim().parse().unwrap_or(1).max(1);
        Ok(StepResult::Next(WizardStep::Delay))
    })
}

/// Delay prompt.
pub(super) fn step_delay(state: &mut WizardState) -> Result<StepResult> {
    let validator: Validator = Box::new(|v: &str| match v.trim().parse::<f64>() {
        Ok(n) if n >= 0.0 => None,
        _ => Some("Enter a non-negative number.".into()),
    });
    let outcome = run_text_prompt(
        "Delay",
        "Pause after each chapter is written (seconds).",
        Some(state.delay.to_string()),
        None,
        Some(validator),
    )?;
    advance_or_back!(outcome, WizardStep::Workers, |value| {
        let parsed: f64 = value.trim().parse().unwrap_or(0.0);
        state.delay = parsed.max(0.0);
        Ok(StepResult::Next(WizardStep::IfExists))
    })
}

/// Existing-file policy select. Hides the `Ask` option when running in parallel.
pub(super) fn step_if_exists(state: &mut WizardState) -> Result<StepResult> {
    let mut allowed = Vec::new();
    if state.workers <= 1 {
        allowed.push(SelectOption {
            label: "Ask what to do for each existing chapter".into(),
            value: ExistingFilePolicy::Ask,
            hint: None,
        });
    }
    allowed.push(SelectOption {
        label: "Skip existing chapter files".into(),
        value: ExistingFilePolicy::Skip,
        hint: None,
    });
    allowed.push(SelectOption {
        label: "Overwrite existing chapter files".into(),
        value: ExistingFilePolicy::Overwrite,
        hint: None,
    });
    let initial_policy = if state.workers > 1 && state.if_exists == ExistingFilePolicy::Ask {
        ExistingFilePolicy::Skip
    } else {
        state.if_exists
    };
    let outcome = run_select(
        "If chapter exists",
        "Pick a behavior for existing chapter files.",
        Select::with_initial(allowed, &initial_policy),
    )?;
    advance_or_back!(
        outcome,
        step_before_if_exists(state.pacing_is_fixed()),
        |value| {
            state.if_exists = value;
            Ok(StepResult::Next(WizardStep::FastSkip))
        }
    )
}

/// Chapter directory prompt — only used in `EpubOnly` mode.
pub(super) fn step_chapter_dir(state: &mut WizardState) -> Result<StepResult> {
    let outcome = run_path_prompt(
        "Chapter directory",
        "Path to the existing chapter directory. Tab to autocomplete.",
        state
            .chapter_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    )?;
    advance_or_back!(outcome, WizardStep::Mode, |value| {
        state.chapter_dir = Some(PathBuf::from(expand_tilde(value.trim()).as_ref()));
        Ok(StepResult::Next(WizardStep::Title))
    })
}

/// Fast-skip yes/no.
pub(super) fn step_fast_skip(state: &mut WizardState) -> Result<StepResult> {
    let outcome = run_confirm(
        "Fast skip",
        "Bypass the remote check when the chapter file already exists locally?",
        state.fast_skip,
    )?;
    advance_or_back!(outcome, WizardStep::IfExists, |value| {
        state.fast_skip = value;
        let next = if state.mode == CrawlMode::Crawl {
            WizardStep::Confirm
        } else {
            WizardStep::FontChoice
        };
        Ok(StepResult::Next(next))
    })
}

/// Choose between the bundled font, a previously remembered font, or a custom
/// file. The remembered list is validated once per wizard run and cached in
/// the state, so returning here by back-navigation costs no filesystem work.
pub(super) async fn step_font_choice(state: &mut WizardState) -> Result<StepResult> {
    if state.recent_fonts.is_none() {
        let fonts = match crate::recent_fonts::config_dir() {
            Some(dir) => crate::recent_fonts::load(&dir).await,
            None => Vec::new(),
        };
        state.recent_fonts = Some(fonts);
    }
    let remembered = state.recent_fonts.as_deref().unwrap_or_default();

    let mut options = vec![SelectOption {
        label: "Use the bundled Bokerlam.ttf".into(),
        value: FontChoice::Default,
        hint: None,
    }];
    options.extend(remembered.iter().map(|font| SelectOption {
        label: font.family_name.clone(),
        value: FontChoice::Remembered(font.path.clone()),
        hint: Some(font.path.to_string_lossy().into_owned()),
    }));
    options.push(SelectOption {
        label: "Pick a custom font file path".into(),
        value: FontChoice::Custom,
        hint: None,
    });

    let outcome = run_select(
        "EPUB font",
        "Pick the font embedded in the EPUB.",
        Select::with_initial(options, &state.font_choice),
    )?;
    let previous = if state.mode == CrawlMode::EpubOnly {
        WizardStep::Author
    } else {
        WizardStep::FastSkip
    };
    advance_or_back!(outcome, previous, |choice| {
        state.font_choice = choice;
        let next = match &state.font_choice {
            FontChoice::Custom => WizardStep::FontPath,
            FontChoice::Remembered(path) => {
                state.font_path = Some(path.clone());
                WizardStep::Confirm
            }
            FontChoice::Default => {
                state.font_path = None;
                WizardStep::Confirm
            }
        };
        Ok(StepResult::Next(next))
    })
}

/// Custom font path picker. The submitted path is validated here so a bad
/// font is caught at the prompt rather than after the whole crawl has run.
pub(super) async fn step_font_path(state: &mut WizardState) -> Result<StepResult> {
    let outcome = run_path_prompt(
        "Font path",
        "Absolute path to the .ttf/.otf file. Tab to autocomplete.",
        state
            .font_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    )?;
    advance_or_back!(outcome, WizardStep::FontChoice, |value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(StepResult::Next(WizardStep::FontChoice));
        }
        let candidate = PathBuf::from(expand_tilde(trimmed).as_ref());
        match crate::utils::validate_font_file(&candidate).await {
            Ok((canonical, _)) => {
                state.font_path = Some(canonical);
                Ok(StepResult::Next(WizardStep::Confirm))
            }
            Err(error) => {
                match show_note(
                    "Font path",
                    &format!("{error}\n\nPress Enter to try again."),
                )? {
                    PromptOutcome::Quit => Ok(StepResult::Quit),
                    _ => Ok(StepResult::Next(WizardStep::FontPath)),
                }
            }
        }
    })
}

/// Final confirmation. Yes finalizes the plan; No goes back to the prior step.
pub(super) fn step_confirm(state: &mut WizardState) -> Result<StepResult> {
    let chapter_numbers = if state.mode == CrawlMode::EpubOnly {
        None
    } else {
        Some(crate::cli::chapter_range(
            state.start_chapter,
            state.end_chapter,
        ))
    };
    let source = crate::source::registry::resolve(&state.base_url, state.allow_any_host)
        .map(|adapter| adapter.display_name())
        .unwrap_or("unknown");
    let summary = build_summary(SummaryParams {
        base_url: &state.base_url,
        source,
        mode: state.mode,
        output_root: &state.output_root,
        chapter_numbers: chapter_numbers.as_deref(),
        delay: state.delay,
        workers: state.workers,
        if_exists: state.if_exists,
        chapter_dir: state.chapter_dir.as_deref(),
        font_path: state.font_path.as_deref(),
        fast_skip: state.fast_skip,
        novel_title: state.novel_title.as_deref(),
        novel_author: state.novel_author.as_deref(),
        pacing_fixed_by_source: state.pacing_is_fixed(),
    });
    let previous = match state.mode {
        CrawlMode::Crawl => WizardStep::FastSkip,
        CrawlMode::CrawlEpub | CrawlMode::EpubOnly => match state.font_choice {
            FontChoice::Custom => WizardStep::FontPath,
            FontChoice::Default | FontChoice::Remembered(_) => WizardStep::FontChoice,
        },
    };
    let outcome = run_confirm("Plan", &summary, true)?;
    match outcome {
        PromptOutcome::Submitted(true) => Ok(StepResult::Done(Box::new(InteractivePlan {
            chapters: select_chapters(&state.chapter_index, chapter_numbers.as_deref()),
            base_url: state.base_url.clone(),
            mode: state.mode,
            output_root: state.output_root.clone(),
            chapter_numbers,
            delay: state.delay,
            workers: state.workers,
            epub: state.mode != CrawlMode::Crawl,
            chapter_dir: state.chapter_dir.clone(),
            font_path: state.font_path.clone(),
            if_exists: state.if_exists,
            fast_skip: state.fast_skip,
            novel_title: state.novel_title.clone(),
            novel_author: state.novel_author.clone(),
            novel_cover_url: state.novel_cover_url.clone(),
        }))),
        PromptOutcome::Submitted(false) => Ok(StepResult::Next(previous)),
        PromptOutcome::Back => Ok(StepResult::Next(previous)),
        PromptOutcome::Quit => Ok(StepResult::Quit),
    }
}
