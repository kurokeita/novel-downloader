use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::crawler::{
    CrawlChapterParams, CrawlResult, CrawlStatus, ExistingChapterDecision, ExistingFilePolicy,
    crawl_chapter,
};
use crate::source::{ChapterRef, RatePolicy, SiteAdapter, SourceError};

/// One observable progress event emitted by the runners. Consumers (CLI
/// progress bar, TUI progress widget, log printer) receive a stream of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// A chapter download is about to start. `total` is the number of
    /// chapters in this run.
    Started { number: u32, total: u32 },
    /// A chapter completed successfully (written, skipped, or skip-all).
    Completed { number: u32, status: CrawlStatus },
    /// A chapter failed. `message` carries the error text the runner also
    /// pushed onto the outcome's `failures` list — surfaced live so users
    /// see *why* a chapter failed without waiting for the run to finish.
    Failed { number: u32, message: String },
    /// The source's rate policy capped this run below the requested worker
    /// count. Run-scoped rather than chapter-keyed, and emitted once before
    /// the first chapter starts, so the user learns why `--workers` was not
    /// honored.
    ConcurrencyClamped {
        /// Workers the user asked for.
        requested: usize,
        /// Workers actually started.
        effective: usize,
        /// Adapter id whose policy imposed the cap.
        source: &'static str,
    },
}

/// Type alias for a thread-safe progress callback.
pub type ProgressCallback = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

/// Aggregated outcome of running multiple chapter downloads.
#[derive(Debug, Clone, Default)]
pub struct RunnerOutcome {
    /// First successfully resolved per-novel directory, if any chapter ran.
    pub output_dir: Option<PathBuf>,
    /// `(chapter_number, error_message)` for each failure, sorted by chapter
    /// number for parallel runs.
    pub failures: Vec<(u32, String)>,
    /// True when the run was aborted before completion (e.g. user pressed
    /// Esc on the TUI download screen). Distinguishes cancellation from a
    /// successful empty run so callers can pick the right exit code.
    pub canceled: bool,
}

/// Inputs to [`crawl_chapters_sequential`].
pub struct SequentialParams {
    /// Source every chapter in this run belongs to. `'static` because the
    /// registry hands out shared adapters that outlive any run.
    pub adapter: &'static dyn SiteAdapter,
    /// Chapters to fetch, in order. A `--start`/`--end` range is a slice of
    /// the novel's chapter index rather than a computed number sequence.
    pub chapters: Vec<ChapterRef>,
    /// Root directory for the per-novel output folder.
    pub output_root: PathBuf,
    /// Per-call existing-file policy. The runner additionally promotes the
    /// run-wide policy to `SkipAll` once the user (or fast-skip) chooses it.
    pub if_exists: ExistingFilePolicy,
    /// Seconds to sleep between successful chapter writes.
    pub delay: f64,
    /// Pre-discovered novel title, enabling fast-skip without a remote fetch.
    pub novel_title: Option<String>,
    /// When true, short-circuit the network call if the destination exists.
    pub fast_skip: bool,
    /// Callback invoked when the policy is `Ask` and the file exists.
    pub prompt: Arc<dyn Fn(&std::path::Path) -> ExistingChapterDecision + Send + Sync>,
    /// Optional progress observer. `None` disables progress emission.
    pub progress: Option<ProgressCallback>,
}

/// Emit a progress event if a callback is configured.
fn emit(progress: &Option<ProgressCallback>, event: ProgressEvent) {
    if let Some(cb) = progress {
        cb(event);
    }
}

/// Run-wide request pacing: a single "not before" instant every worker
/// respects. `min_delay` spaces requests out, and a rate-limit backoff pushes
/// that instant forward so the whole run slows down rather than the one
/// chapter that happened to be refused — rate limiters are per client.
struct Pacer {
    /// Earliest instant at which the next request may leave.
    next_allowed: tokio::sync::Mutex<Instant>,
    /// Minimum spacing the source's policy asks for.
    min_delay: Duration,
}

impl Pacer {
    /// A pacer enforcing `min_delay` between requests, open immediately.
    fn new(min_delay: Duration) -> Self {
        Self {
            next_allowed: tokio::sync::Mutex::new(Instant::now()),
            min_delay,
        }
    }

    /// Wait for this request's turn, then reserve the following slot. The
    /// gate is deliberately held across the sleep: `min_delay` is global
    /// spacing between requests, not a per-worker pause.
    async fn wait_turn(&self) {
        let mut next = self.next_allowed.lock().await;
        let now = Instant::now();
        if *next > now {
            tokio::time::sleep(*next - now).await;
        }
        *next = Instant::now() + self.min_delay;
    }

    /// Push every worker's next turn back by `delay` after a rate-limited
    /// answer, never bringing an already-later instant forward.
    async fn back_off(&self, delay: Duration) {
        let mut next = self.next_allowed.lock().await;
        *next = (*next).max(Instant::now() + delay);
    }
}

/// Describe a chapter failure for the failures list and the progress event.
/// Rate limiting and missing entitlement get their own wording so they do not
/// read like an ordinary transport error.
fn describe_failure(error: &anyhow::Error, retries_spent: u32) -> String {
    match error.downcast_ref::<SourceError>() {
        Some(SourceError::RateLimited {
            source_name,
            message,
        }) => format!(
            "rate limited by {source_name}, giving up after {retries_spent} retries: {message}"
        ),
        Some(SourceError::Unentitled(what)) => {
            format!("not available to this client: {what}")
        }
        _ => error.to_string(),
    }
}

/// Crawl one chapter through the shared pacer, retrying while the source
/// answers [`SourceError::RateLimited`] and the policy still has retries
/// left. Backoff grows with each attempt and is applied run-wide. The error
/// side is already-formatted text, since that is all the runners need.
async fn crawl_chapter_paced(
    params: CrawlChapterParams<'_>,
    pacer: &Pacer,
    policy: RatePolicy,
) -> std::result::Result<CrawlResult, String> {
    let mut retries: u32 = 0;
    loop {
        pacer.wait_turn().await;
        match crawl_chapter(params.clone()).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                let rate_limited = matches!(
                    error.downcast_ref::<SourceError>(),
                    Some(SourceError::RateLimited { .. })
                );
                if rate_limited && retries < policy.max_retries {
                    retries += 1;
                    pacer.back_off(policy.backoff_base * retries).await;
                    continue;
                }
                return Err(describe_failure(&error, retries));
            }
        }
    }
}

/// Crawl chapters one at a time, propagating any `SkipAll` decision to
/// suppress prompts on subsequent existing chapters.
pub async fn crawl_chapters_sequential(params: SequentialParams) -> RunnerOutcome {
    let SequentialParams {
        adapter,
        chapters,
        output_root,
        if_exists,
        delay,
        novel_title,
        fast_skip,
        prompt,
        progress,
    } = params;

    let mut output_dir: Option<PathBuf> = None;
    let mut existing_policy = ExistingFilePolicy::Ask;
    let mut failures: Vec<(u32, String)> = Vec::new();
    let total = chapters.len() as u32;
    let policy = adapter.rate_policy();
    let pacer = Pacer::new(policy.min_delay);

    for chapter in chapters {
        let chapter_number = chapter.number;
        emit(
            &progress,
            ProgressEvent::Started {
                number: chapter_number,
                total,
            },
        );
        let result = crawl_chapter_paced(
            CrawlChapterParams {
                adapter,
                chapter: &chapter,
                output_root: &output_root,
                if_exists,
                existing_policy,
                delay,
                novel_title: novel_title.as_deref(),
                fast_skip,
                prompt: Arc::clone(&prompt),
            },
            &pacer,
            policy,
        )
        .await;
        match result {
            Ok(crawl) => {
                if output_dir.is_none() {
                    output_dir = Some(crawl.output_dir.clone());
                }
                if crawl.status == CrawlStatus::SkipAll {
                    existing_policy = ExistingFilePolicy::SkipAll;
                }
                emit(
                    &progress,
                    ProgressEvent::Completed {
                        number: chapter_number,
                        status: crawl.status,
                    },
                );
            }
            Err(message) => {
                failures.push((chapter_number, message.clone()));
                emit(
                    &progress,
                    ProgressEvent::Failed {
                        number: chapter_number,
                        message,
                    },
                );
            }
        }
    }

    RunnerOutcome {
        output_dir,
        failures,
        canceled: false,
    }
}

/// Inputs to [`crawl_chapters_parallel`].
pub struct ParallelParams {
    /// Source every chapter in this run belongs to. `'static` because the
    /// registry hands out shared adapters that outlive any run.
    pub adapter: &'static dyn SiteAdapter,
    /// Chapters to fetch (order is not preserved across workers, but
    /// failures are sorted on return).
    pub chapters: Vec<ChapterRef>,
    /// Output root directory.
    pub output_root: PathBuf,
    /// Per-call existing-file policy. Must NOT be `Ask` when running in
    /// parallel — the CLI guards against this.
    pub if_exists: ExistingFilePolicy,
    /// Concurrent worker count (>= 1).
    pub workers: usize,
    /// Pre-discovered novel title, enabling fast-skip.
    pub novel_title: Option<String>,
    /// When true, short-circuit on existing files.
    pub fast_skip: bool,
    /// Prompt callback (kept for API symmetry; the CLI never lets `Ask` reach
    /// the parallel path).
    pub prompt: Arc<dyn Fn(&std::path::Path) -> ExistingChapterDecision + Send + Sync>,
    /// Optional progress observer. `None` disables progress emission.
    pub progress: Option<ProgressCallback>,
}

/// Crawl chapters concurrently using a shared FIFO queue and `workers` async
/// workers. Failures are returned sorted by chapter number.
pub async fn crawl_chapters_parallel(params: ParallelParams) -> RunnerOutcome {
    let ParallelParams {
        adapter,
        chapters,
        output_root,
        if_exists,
        workers,
        novel_title,
        fast_skip,
        prompt,
        progress,
    } = params;

    let total = chapters.len() as u32;
    let policy = adapter.rate_policy();
    let requested = workers.max(1);
    let effective = requested.min(policy.max_concurrency.max(1));
    if effective < requested {
        emit(
            &progress,
            ProgressEvent::ConcurrencyClamped {
                requested,
                effective,
                source: adapter.id(),
            },
        );
    }
    let pacer = Arc::new(Pacer::new(policy.min_delay));
    let queue = Arc::new(tokio::sync::Mutex::new(VecDeque::from(chapters)));
    let output_dir: Arc<tokio::sync::Mutex<Option<PathBuf>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let failures: Arc<tokio::sync::Mutex<Vec<(u32, String)>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let output_root = Arc::new(output_root);
    let novel_title = Arc::new(novel_title);

    let mut handles = Vec::new();
    for _ in 0..effective {
        let queue = Arc::clone(&queue);
        let pacer = Arc::clone(&pacer);
        let output_dir = Arc::clone(&output_dir);
        let failures = Arc::clone(&failures);
        let output_root = Arc::clone(&output_root);
        let novel_title = Arc::clone(&novel_title);
        let prompt = Arc::clone(&prompt);
        let progress = progress.clone();

        handles.push(tokio::spawn(async move {
            loop {
                let chapter = match queue.lock().await.pop_front() {
                    Some(c) => c,
                    None => break,
                };
                let chapter_number = chapter.number;

                emit(
                    &progress,
                    ProgressEvent::Started {
                        number: chapter_number,
                        total,
                    },
                );
                let result = crawl_chapter_paced(
                    CrawlChapterParams {
                        adapter,
                        chapter: &chapter,
                        output_root: output_root.as_path(),
                        if_exists,
                        existing_policy: ExistingFilePolicy::Ask,
                        delay: 0.0,
                        novel_title: novel_title.as_deref(),
                        fast_skip,
                        prompt: Arc::clone(&prompt),
                    },
                    &pacer,
                    policy,
                )
                .await;
                match result {
                    Ok(crawl) => {
                        let mut od = output_dir.lock().await;
                        if od.is_none() {
                            *od = Some(crawl.output_dir.clone());
                        }
                        emit(
                            &progress,
                            ProgressEvent::Completed {
                                number: chapter_number,
                                status: crawl.status,
                            },
                        );
                    }
                    Err(message) => {
                        failures
                            .lock()
                            .await
                            .push((chapter_number, message.clone()));
                        emit(
                            &progress,
                            ProgressEvent::Failed {
                                number: chapter_number,
                                message,
                            },
                        );
                    }
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let mut failures = failures.lock().await.clone();
    failures.sort_by_key(|(n, _)| *n);
    let output_dir = output_dir.lock().await.clone();
    RunnerOutcome {
        output_dir,
        failures,
        canceled: false,
    }
}
