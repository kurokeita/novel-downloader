use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use novel_downloader::cli::{
    CliOptions, RawArgs, chapter_range, from_raw, validate_chapter_range, validate_shared_options,
};
use novel_downloader::crawler::{CrawlStatus, ExistingChapterDecision, ExistingFilePolicy};
use novel_downloader::epub::{BuildEpubParams, EpubMetadataOverride, build_epub};
use novel_downloader::runner::{
    ParallelParams, ProgressCallback, ProgressEvent, SequentialParams, crawl_chapters_parallel,
    crawl_chapters_sequential,
};
use novel_downloader::source::registry::{resolve, validate_url};
use novel_downloader::source::{ChapterRef, SiteAdapter};
use novel_downloader::ui::{
    CrawlMode, DownloadProgress, InteractivePlan, epub_destination_dir, make_tui_progress_callback,
    run_download_screen, run_interactive_flow,
};

/// Non-TUI prompt for existing chapter files. Reads a line from stdin and
/// maps r/s/a to the [`ExistingChapterDecision`] variants. Defaults to Skip
/// if stdin is closed or the input is unparseable.
fn cli_existing_chapter_prompt(chapter_path: &std::path::Path) -> ExistingChapterDecision {
    use std::io::{Write, stdin, stdout};
    eprintln!("[EXISTS] {}", chapter_path.display());
    eprint!("Choose: [r]edownload / [s]kip / skip [a]ll existing: ");
    let _ = stdout().flush();
    let mut buf = String::new();
    if stdin().read_line(&mut buf).is_err() {
        return ExistingChapterDecision::Skip;
    }
    match buf.trim().to_ascii_lowercase().as_str() {
        "r" | "redownload" => ExistingChapterDecision::Redownload,
        "a" | "all" | "skip_all" | "skip-all" => ExistingChapterDecision::SkipAll,
        _ => ExistingChapterDecision::Skip,
    }
}

/// Build an [`InteractivePlan`] from non-interactive CLI options. Discovers
/// the last available chapter when no explicit `--end` was provided.
async fn build_non_interactive_plan(
    base_url: String,
    options: &CliOptions,
    adapter: &'static dyn SiteAdapter,
) -> Result<InteractivePlan> {
    let mut chapter_numbers: Option<Vec<u32>> = None;
    let mut chapters: Vec<ChapterRef> = Vec::new();

    // The EPUB writer takes its metadata from here rather than reading the main
    // page itself, so `--epub-only` needs a fetch too - but only the cheap
    // metadata one. Walking the chapter index would make packaging an existing
    // directory fail whenever the site's chapter listing is unreachable.
    let novel = if options.epub_only {
        adapter.fetch_metadata(&base_url).await?
    } else {
        adapter.fetch_novel(&base_url).await?
    };
    let novel_title = Some(novel.title.clone());
    let novel_author = novel.author.clone();
    let novel_cover_url = novel.cover_url.clone();

    if !options.epub_only {
        let last = novel.chapters.last().map(|chapter| chapter.number);
        let start = options.start.unwrap_or(1);
        let mut end = match (options.end, last) {
            (Some(e), _) => e,
            (None, Some(l)) => l,
            (None, None) => start,
        };
        if let Some(l) = last
            && end > l
        {
            eprintln!(
                "[INFO] Requested end chapter {} exceeds the last available chapter {}; stopping at {}.",
                end, l, l
            );
            end = l;
        }
        if let Some(message) = validate_chapter_range(start, end) {
            return Err(anyhow::anyhow!("{}", message.replace("Error: ", "")));
        }
        let numbers = chapter_range(start, end);
        chapters = novel
            .chapters
            .into_iter()
            .filter(|chapter| numbers.contains(&chapter.number))
            .collect();
        chapter_numbers = Some(numbers);
    }

    let mode = if options.epub_only {
        CrawlMode::EpubOnly
    } else if options.epub {
        CrawlMode::CrawlEpub
    } else {
        CrawlMode::Crawl
    };

    Ok(InteractivePlan {
        base_url,
        mode,
        output_root: PathBuf::from(&options.output_root),
        chapter_numbers,
        chapters,
        delay: options.delay,
        workers: options.workers,
        epub: options.epub || options.epub_only,
        chapter_dir: options.chapter_dir.as_ref().map(PathBuf::from),
        font_path: options.font_path.as_ref().map(PathBuf::from),
        if_exists: options.if_exists,
        fast_skip: options.fast_skip,
        novel_title,
        novel_author,
        novel_cover_url,
    })
}

/// Build a styled indicatif progress bar with chapter counter, elapsed time,
/// and ETA. The bar is configured for `total` discrete chapter ticks.
fn build_progress_bar(total: u64) -> ProgressBar {
    let bar = ProgressBar::new(total);
    let template = "{prefix:.bold.cyan} {spinner:.cyan} [{elapsed_precise}] [{bar:40.cyan/blue}] \
                    {pos}/{len} ({percent}%) ETA {eta_precise} {wide_msg}";
    let style = ProgressStyle::with_template(template)
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("█▉▊▋▌▍▎▏ ");
    bar.set_style(style);
    bar.set_prefix("Chapters");
    bar.enable_steady_tick(std::time::Duration::from_millis(120));
    bar
}

/// Format a single-line status update for one chapter event, used as the
/// fallback when indicatif's bar is hidden (non-TTY) and as the message
/// printed alongside the bar when it is visible.
fn format_event(event: &ProgressEvent) -> Option<String> {
    match event {
        ProgressEvent::Started { .. } => None,
        ProgressEvent::Completed { number, status } => {
            let label = match status {
                CrawlStatus::Written => "OK",
                CrawlStatus::Skipped => "SKIP",
                CrawlStatus::SkipAll => "SKIP-ALL",
            };
            Some(format!("[{label}] Chapter {}", number))
        }
        ProgressEvent::Failed { number, message } => {
            Some(format!("[FAIL] Chapter {}: {}", number, message))
        }
        ProgressEvent::ConcurrencyClamped {
            requested,
            effective,
            source,
        } => Some(format!(
            "[INFO] {} allows at most {} concurrent requests; using {} workers instead of {}.",
            source, effective, effective, requested
        )),
    }
}

/// Wire an indicatif [`ProgressBar`] up to runner [`ProgressEvent`]s. The
/// returned callback advances the bar, updates the `wide_msg` slot for the
/// in-flight chapter, and falls back to plain `eprintln!` lines when the bar
/// is hidden (non-TTY) so progress is always visible.
fn make_progress_callback(bar: ProgressBar) -> ProgressCallback {
    Arc::new(move |event| {
        if let ProgressEvent::Started { number, .. } = &event {
            bar.set_message(format!("→ chapter {}", number));
        }
        let line = format_event(&event);
        // Only chapter-keyed events advance the bar; the run-scoped clamp
        // notice is printed alongside it.
        let advances = matches!(
            event,
            ProgressEvent::Completed { .. } | ProgressEvent::Failed { .. }
        );
        if let Some(text) = line {
            if bar.is_hidden() {
                eprintln!("{}", text);
            } else {
                bar.println(text);
            }
        }
        if advances {
            bar.inc(1);
        }
    })
}

/// Drive a chapter run with an indicatif progress bar (non-interactive mode).
async fn run_with_indicatif(
    plan: &InteractivePlan,
    adapter: &'static dyn SiteAdapter,
    chapters: Vec<ChapterRef>,
    prompt: Arc<dyn Fn(&std::path::Path) -> ExistingChapterDecision + Send + Sync>,
) -> Result<novel_downloader::runner::RunnerOutcome, i32> {
    let bar = build_progress_bar(chapters.len() as u64);
    let progress = make_progress_callback(bar.clone());
    let outcome = if plan.workers <= 1 {
        crawl_chapters_sequential(SequentialParams {
            adapter,
            chapters,
            output_root: plan.output_root.clone(),
            if_exists: plan.if_exists,
            delay: plan.delay,
            novel_title: plan.novel_title.clone(),
            fast_skip: plan.fast_skip,
            prompt,
            progress: Some(progress),
        })
        .await
    } else {
        if plan.if_exists == ExistingFilePolicy::Ask {
            bar.finish_and_clear();
            eprintln!("Error: --workers > 1 requires --if-exists skip or --if-exists overwrite.");
            return Err(1);
        }
        crawl_chapters_parallel(ParallelParams {
            adapter,
            chapters,
            output_root: plan.output_root.clone(),
            if_exists: plan.if_exists,
            workers: plan.workers,
            delay: plan.delay,
            novel_title: plan.novel_title.clone(),
            fast_skip: plan.fast_skip,
            prompt,
            progress: Some(progress),
        })
        .await
    };
    bar.finish_with_message("done");
    Ok(outcome)
}

/// Drive a chapter run with the styled ratatui TUI download screen.
///
/// `wait_for_user` controls whether the screen pauses for an Enter press
/// after completion. Pass `false` when an EPUB build screen will follow so
/// the bare terminal never flashes between TUI screens.
async fn run_with_tui(
    plan: &InteractivePlan,
    adapter: &'static dyn SiteAdapter,
    chapters: Vec<ChapterRef>,
    prompt: Arc<dyn Fn(&std::path::Path) -> ExistingChapterDecision + Send + Sync>,
    wait_for_user: bool,
) -> Result<novel_downloader::runner::RunnerOutcome, i32> {
    if plan.workers > 1 && plan.if_exists == ExistingFilePolicy::Ask {
        eprintln!("Error: --workers > 1 requires --if-exists skip or --if-exists overwrite.");
        return Err(1);
    }
    let total = chapters.len() as u32;
    let state = Arc::new(std::sync::Mutex::new(DownloadProgress::new(total)));
    let progress = make_tui_progress_callback(Arc::clone(&state));

    let plan_clone = plan.clone();
    let prompt_clone = Arc::clone(&prompt);
    let task = tokio::spawn(async move {
        if plan_clone.workers <= 1 {
            crawl_chapters_sequential(SequentialParams {
                adapter,
                chapters,
                output_root: plan_clone.output_root.clone(),
                if_exists: plan_clone.if_exists,
                delay: plan_clone.delay,
                novel_title: plan_clone.novel_title.clone(),
                fast_skip: plan_clone.fast_skip,
                prompt: prompt_clone,
                progress: Some(progress),
            })
            .await
        } else {
            crawl_chapters_parallel(ParallelParams {
                adapter,
                chapters,
                output_root: plan_clone.output_root.clone(),
                if_exists: plan_clone.if_exists,
                workers: plan_clone.workers,
                delay: plan_clone.delay,
                novel_title: plan_clone.novel_title.clone(),
                fast_skip: plan_clone.fast_skip,
                prompt: prompt_clone,
                progress: Some(progress),
            })
            .await
        }
    });

    match run_download_screen(state, task, wait_for_user).await {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            eprintln!("[FAIL] download screen error: {}", error);
            Err(1)
        }
    }
}

/// What the user picked when prompted about a partially-failed crawl
/// before the EPUB build step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureAction {
    /// Re-run only the failed chapter numbers and try the EPUB again.
    Retry,
    /// Skip the EPUB build entirely.
    Abort,
}

/// Render the failed chapter numbers (sorted, deduplicated, comma-separated)
/// for use in user-facing prompts.
fn format_failure_chapter_list(failures: &[(u32, String)]) -> String {
    let mut numbers: Vec<u32> = failures.iter().map(|(n, _)| *n).collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ask the user whether to retry the failed chapters or skip the EPUB
/// build. In interactive mode this is a TUI confirm screen; in
/// non-interactive mode it reads a single character from stdin. When stdin
/// is not a TTY (piped / EOF) the default is `Abort` so we never build a
/// broken EPUB unattended.
fn prompt_failure_action(interactive: bool, failures: &[(u32, String)]) -> FailureAction {
    let list = format_failure_chapter_list(failures);
    if interactive {
        let message = format!(
            "{} chapter(s) failed:\n{}\n\nRetry the failed chapters before building the EPUB?",
            failures.len(),
            list
        );
        match novel_downloader::ui::run_confirm("Some chapters failed", &message, true) {
            Ok(novel_downloader::ui::PromptOutcome::Submitted(true)) => FailureAction::Retry,
            _ => FailureAction::Abort,
        }
    } else {
        use std::io::{Write, stdin, stdout};
        eprintln!("\n[FAIL] {} chapter(s) failed: {}", failures.len(), list);
        eprint!("Retry failed chapters before building EPUB? [r]etry / [a]bort: ");
        let _ = stdout().flush();
        let mut line = String::new();
        match stdin().read_line(&mut line) {
            Ok(0) | Err(_) => FailureAction::Abort,
            Ok(_) => match line.trim().to_lowercase().as_str() {
                "r" | "retry" => FailureAction::Retry,
                _ => FailureAction::Abort,
            },
        }
    }
}

/// Execute a fully-resolved interactive plan: download chapters and/or build
/// the EPUB. Returns the process exit code (0 = success, 2 = partial
/// failures, 3 = EPUB build failed).
///
/// `interactive` selects the progress UI: when true, the TUI download screen
/// is shown; when false, an indicatif bar prints to stderr.
async fn execute_plan(
    plan: InteractivePlan,
    adapter: &'static dyn SiteAdapter,
    interactive: bool,
) -> i32 {
    let prompt: Arc<dyn Fn(&std::path::Path) -> ExistingChapterDecision + Send + Sync> =
        Arc::new(cli_existing_chapter_prompt);

    let output_dir: Option<PathBuf>;
    let mut failures: Vec<(u32, String)> = Vec::new();

    if plan.mode == CrawlMode::EpubOnly {
        output_dir = match epub_destination_dir(
            plan.mode,
            &plan.output_root,
            plan.chapter_dir.as_deref(),
            plan.novel_title.as_deref(),
        ) {
            Some(dir) => Some(dir),
            None => {
                eprintln!(
                    "[FAIL] Could not infer chapter directory: no novel title available to derive the directory from"
                );
                return 3;
            }
        };
    } else {
        let numbers = plan.chapter_numbers.clone().unwrap_or_default();
        if !interactive {
            if let (Some(first), Some(last)) = (numbers.first(), numbers.last()) {
                println!(
                    "[INFO] Downloading chapters {} -> {} ({} chapters)",
                    first,
                    last,
                    numbers.len()
                );
            }
            println!("[INFO] Using {} worker(s)", plan.workers);
        }
        let chapters = plan.chapters.clone();

        let outcome = if interactive {
            // Skip the post-download "press Enter" wait when an EPUB build
            // screen is queued, so the user transitions straight from the
            // download screen into the build screen.
            match run_with_tui(&plan, adapter, chapters, Arc::clone(&prompt), !plan.epub).await {
                Ok(o) => o,
                Err(code) => return code,
            }
        } else {
            match run_with_indicatif(&plan, adapter, chapters, Arc::clone(&prompt)).await {
                Ok(o) => o,
                Err(code) => return code,
            }
        };
        if outcome.canceled {
            eprintln!("[INFO] Download canceled by user.");
            return 1;
        }
        output_dir = outcome.output_dir;
        failures = outcome.failures;
    }

    if !failures.is_empty() && !interactive && !plan.epub {
        eprintln!("\nSome chapters failed:");
        for (chapter, message) in &failures {
            eprintln!("  - Chapter {}: {}", chapter, message);
        }
    }

    if plan.epub {
        // Never build an EPUB with missing chapters. Loop: list the failures,
        // ask the user whether to retry the failed chapter numbers or skip
        // the EPUB entirely. Retry overwrites the existing chapter files
        // (which may be partial or empty after the original failure).
        while !failures.is_empty() {
            match prompt_failure_action(interactive, &failures) {
                FailureAction::Abort => {
                    if interactive {
                        let _ = novel_downloader::ui::show_note(
                            "EPUB skipped",
                            &format!(
                                "Skipped EPUB build because {} chapter(s) failed.",
                                failures.len()
                            ),
                        );
                    } else {
                        eprintln!(
                            "[INFO] Skipping EPUB build due to {} failed chapter(s).",
                            failures.len()
                        );
                    }
                    return 2;
                }
                FailureAction::Retry => {
                    let retry_numbers: Vec<u32> = failures.iter().map(|(n, _)| *n).collect();
                    let retry_chapters: Vec<ChapterRef> = plan
                        .chapters
                        .iter()
                        .filter(|chapter| retry_numbers.contains(&chapter.number))
                        .cloned()
                        .collect();
                    let mut retry_plan = plan.clone();
                    retry_plan.if_exists = ExistingFilePolicy::Overwrite;
                    let outcome = if interactive {
                        match run_with_tui(
                            &retry_plan,
                            adapter,
                            retry_chapters,
                            Arc::clone(&prompt),
                            !plan.epub,
                        )
                        .await
                        {
                            Ok(o) => o,
                            Err(code) => return code,
                        }
                    } else {
                        match run_with_indicatif(
                            &retry_plan,
                            adapter,
                            retry_chapters,
                            Arc::clone(&prompt),
                        )
                        .await
                        {
                            Ok(o) => o,
                            Err(code) => return code,
                        }
                    };
                    if outcome.canceled {
                        eprintln!("[INFO] Retry canceled by user.");
                        return 1;
                    }
                    failures = outcome.failures;
                }
            }
        }

        let chapter_dir = match output_dir.clone() {
            Some(d) => d,
            None => {
                eprintln!("[FAIL] No chapter directory available to build EPUB.");
                return 3;
            }
        };
        let novel_main_url = format!("{}/", plan.base_url.trim_end_matches('/'));
        let font_path = plan.font_path.clone();
        // "Unknown Novel" matches what the main-page extractor produced when a
        // page carried no usable title.
        let novel_title = plan
            .novel_title
            .clone()
            .unwrap_or_else(|| "Unknown Novel".to_string());
        let novel_author = plan.novel_author.clone();
        let cover_url = plan.novel_cover_url.clone();
        // In interactive mode the wizard collected (and let the user edit) the
        // title/author, so pass them through verbatim as an explicit override.
        // Non-interactive runs leave this `None`; the title and author already
        // on `BuildEpubParams` are then used as-is.
        let metadata_override = if interactive {
            plan.novel_title
                .clone()
                .and_then(|title| EpubMetadataOverride::new(title, plan.novel_author.clone()))
        } else {
            None
        };
        let build_future = async move {
            build_epub(BuildEpubParams {
                novel_main_url,
                novel_title,
                novel_author,
                cover_url,
                chapter_dir,
                output_epub: None,
                font_path,
                metadata_override,
            })
            .await
        };
        let epub_result = if interactive {
            // Stay inside the TUI: a styled "Building EPUB" screen with the
            // shared spinner runs while the build future resolves. Esc/Ctrl+C
            // aborts the build.
            match novel_downloader::ui::run_loading_screen(
                "Building EPUB",
                "Packaging chapters, font, and cover into an EPUB archive…",
                build_future,
            )
            .await
            {
                Ok(novel_downloader::ui::PromptOutcome::Submitted(inner)) => inner,
                Ok(novel_downloader::ui::PromptOutcome::Back)
                | Ok(novel_downloader::ui::PromptOutcome::Quit) => {
                    eprintln!("[INFO] EPUB build canceled by user.");
                    return 1;
                }
                Err(error) => {
                    eprintln!("[FAIL] EPUB screen error: {}", error);
                    return 3;
                }
            }
        } else {
            build_future.await
        };
        match epub_result {
            Ok(path) => {
                if interactive {
                    let mut body = format!("EPUB created at:\n{}", path.display());
                    if !failures.is_empty() {
                        body.push_str(&format!(
                            "\n\n{} chapter(s) failed during download.",
                            failures.len()
                        ));
                    }
                    let _ = novel_downloader::ui::show_note("Done", &body);
                } else {
                    println!("[OK] EPUB -> {}", path.display());
                }
            }
            Err(error) => {
                if interactive {
                    let _ =
                        novel_downloader::ui::show_note("EPUB build failed", &format!("{}", error));
                } else {
                    eprintln!("[FAIL] EPUB build failed: {}", error);
                }
                return 3;
            }
        }
    }

    if failures.is_empty() { 0 } else { 2 }
}

/// Async entry point: parse CLI, dispatch interactive vs. non-interactive,
/// and run the resulting plan to completion.
async fn run() -> i32 {
    let parsed = from_raw(RawArgs::parse());

    if let Some(message) = validate_shared_options(&parsed.options) {
        eprintln!("{}", message);
        return 1;
    }

    if let Some(base_url) = parsed.base_url.as_deref()
        && let Some(message) = validate_url(base_url, parsed.options.allow_any_host)
    {
        eprintln!("Error: {}", message);
        return 2;
    }

    let interactive = parsed.options.interactive || parsed.base_url.is_none();
    let plan = match parsed.base_url {
        Some(base_url) if !parsed.options.interactive => {
            let adapter = match resolve(&base_url, parsed.options.allow_any_host) {
                Ok(adapter) => adapter,
                Err(error) => {
                    eprintln!("Error: {}", error);
                    return 2;
                }
            };
            match build_non_interactive_plan(base_url, &parsed.options, adapter).await {
                Ok(plan) => plan,
                Err(error) => {
                    eprintln!("Error: {}", error);
                    return 1;
                }
            }
        }
        base_url => match run_interactive_flow(base_url, &parsed.options).await {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                eprintln!("Interactive crawl canceled.");
                return 1;
            }
            Err(error) => {
                eprintln!("Error launching TUI: {}", error);
                return 1;
            }
        },
    };

    let adapter = match resolve(&plan.base_url, parsed.options.allow_any_host) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("Error: {}", error);
            return 2;
        }
    };

    if let Some(notice) = novel_downloader::cli::delay_override_notice(
        adapter.display_name(),
        plan.delay,
        &adapter.rate_policy(),
    ) {
        println!("{notice}");
    }

    execute_plan(plan, adapter, interactive).await
}

/// Process entry point. Spins up a Tokio runtime and exits with the code
/// returned by [`run`].
#[tokio::main]
async fn main() {
    let code = run().await;
    std::process::exit(code);
}
