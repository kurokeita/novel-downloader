use novel_downloader::crawler::{CrawlStatus, ExistingChapterDecision, ExistingFilePolicy};
use novel_downloader::runner::{
    ParallelParams, ProgressCallback, ProgressEvent, RunnerOutcome, SequentialParams,
    crawl_chapters_parallel, crawl_chapters_sequential,
};
use novel_downloader::source::metruyenhot::Metruyenhot;
use novel_downloader::source::{
    ChapterContent, ChapterRef, Novel, RatePolicy, SourceError, SourceResult,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Chapter refs pointing at the mock origin server's chapters. Built as
/// literals rather than derived from a base URL, since the runner is not
/// supposed to know how a locator is constructed.
fn chapter_refs(server_url: &str, numbers: &[u32]) -> Vec<ChapterRef> {
    numbers
        .iter()
        .map(|&number| ChapterRef {
            number,
            title: None,
            locator: format!("{}/foo/chuong-{}/", server_url, number),
        })
        .collect()
}

/// Return a small fake chapter HTML for a given chapter number.
fn fake_chapter_html(n: u32) -> String {
    format!(
        r#"<html><body>
  <div class="rv-full-story-title"><h1>Truyện X</h1></div>
  <div class="rv-chapt-title"><h2>Chương {n}</h2></div>
  <div class="chapter-c"><p>Body {n}.</p></div>
</body></html>"#
    )
}

#[tokio::test]
async fn sequential_writes_each_requested_chapter() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("GET", "/foo/chuong-1/")
        .with_status(200)
        .with_body(fake_chapter_html(1))
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/foo/chuong-2/")
        .with_status(200)
        .with_body(fake_chapter_html(2))
        .create_async()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip);
    let outcome: RunnerOutcome = crawl_chapters_sequential(SequentialParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Skip,
        delay: 0.0,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: None,
    })
    .await;
    assert_eq!(outcome.failures.len(), 0);
    let output_dir = outcome.output_dir.expect("output_dir set");
    assert!(output_dir.join("chapter_0001.html").exists());
    assert!(output_dir.join("chapter_0002.html").exists());
}

#[tokio::test]
async fn sequential_collects_failures_per_chapter() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("GET", "/foo/chuong-1/")
        .with_status(200)
        .with_body(fake_chapter_html(1))
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/foo/chuong-2/")
        .with_status(500)
        .create_async()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip);
    let outcome = crawl_chapters_sequential(SequentialParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Skip,
        delay: 0.0,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: None,
    })
    .await;
    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].0, 2);
    assert!(outcome.failures[0].1.contains("HTTP 500"));
}

#[tokio::test]
async fn sequential_propagates_skip_all_decision() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("GET", "/foo/chuong-1/")
        .with_status(200)
        .with_body(fake_chapter_html(1))
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/foo/chuong-2/")
        .with_status(200)
        .with_body(fake_chapter_html(2))
        .create_async()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let novel_dir = dir.path().join("truyen_x");
    tokio::fs::create_dir_all(&novel_dir).await.unwrap();
    tokio::fs::write(novel_dir.join("chapter_0001.html"), b"old")
        .await
        .unwrap();
    tokio::fs::write(novel_dir.join("chapter_0002.html"), b"old")
        .await
        .unwrap();

    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::SkipAll);
    let outcome = crawl_chapters_sequential(SequentialParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Ask,
        delay: 0.0,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: None,
    })
    .await;
    assert_eq!(outcome.failures.len(), 0);
    // Both should be untouched.
    assert_eq!(
        tokio::fs::read_to_string(novel_dir.join("chapter_0001.html"))
            .await
            .unwrap(),
        "old"
    );
    assert_eq!(
        tokio::fs::read_to_string(novel_dir.join("chapter_0002.html"))
            .await
            .unwrap(),
        "old"
    );
}

#[tokio::test]
async fn parallel_runs_all_chapters_with_multiple_workers() {
    let mut server = mockito::Server::new_async().await;
    for n in 1..=4 {
        server
            .mock("GET", format!("/foo/chuong-{}/", n).as_str())
            .with_status(200)
            .with_body(fake_chapter_html(n))
            .create_async()
            .await;
    }
    let dir = tempfile::tempdir().unwrap();
    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip);
    let outcome = crawl_chapters_parallel(ParallelParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2, 3, 4]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Skip,
        workers: 3,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: None,
    })
    .await;
    assert_eq!(outcome.failures.len(), 0);
    let output_dir = outcome.output_dir.expect("output_dir set");
    for n in 1..=4 {
        assert!(output_dir.join(format!("chapter_{:04}.html", n)).exists());
    }
}

#[tokio::test]
async fn parallel_collects_failures_sorted_by_chapter() {
    let mut server = mockito::Server::new_async().await;
    let _ok = server
        .mock("GET", "/foo/chuong-1/")
        .with_status(200)
        .with_body(fake_chapter_html(1))
        .create_async()
        .await;
    let _bad2 = server
        .mock("GET", "/foo/chuong-2/")
        .with_status(404)
        .create_async()
        .await;
    let _bad3 = server
        .mock("GET", "/foo/chuong-3/")
        .with_status(500)
        .create_async()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip);
    let outcome = crawl_chapters_parallel(ParallelParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2, 3]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Skip,
        workers: 2,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: None,
    })
    .await;
    assert_eq!(outcome.failures.len(), 2);
    assert_eq!(outcome.failures[0].0, 2);
    assert_eq!(outcome.failures[1].0, 3);
}

#[tokio::test]
async fn sequential_emits_progress_events_for_each_chapter() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("GET", "/foo/chuong-1/")
        .with_status(200)
        .with_body(fake_chapter_html(1))
        .create_async()
        .await;
    let _m2 = server
        .mock("GET", "/foo/chuong-2/")
        .with_status(500)
        .create_async()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip);
    let events: Arc<Mutex<Vec<ProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let progress: ProgressCallback = Arc::new(move |event| captured.lock().unwrap().push(event));

    let _ = crawl_chapters_sequential(SequentialParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Skip,
        delay: 0.0,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: Some(progress),
    })
    .await;
    let captured = events.lock().unwrap();
    let starts = captured
        .iter()
        .filter(|e| matches!(e, ProgressEvent::Started { .. }))
        .count();
    let completes = captured
        .iter()
        .filter(|e| matches!(e, ProgressEvent::Completed { .. }))
        .count();
    let fails = captured
        .iter()
        .filter(|e| matches!(e, ProgressEvent::Failed { .. }))
        .count();
    assert_eq!(starts, 2, "one Started per chapter");
    assert_eq!(completes, 1, "chapter 1 completes");
    assert_eq!(fails, 1, "chapter 2 fails");

    let completed_status = captured.iter().find_map(|e| match e {
        ProgressEvent::Completed { status, .. } => Some(*status),
        _ => None,
    });
    assert_eq!(completed_status, Some(CrawlStatus::Written));

    // The Failed event must carry the error reason so the UI can show it live.
    let failed_message = captured.iter().find_map(|e| match e {
        ProgressEvent::Failed { number, message } if *number == 2 => Some(message.clone()),
        _ => None,
    });
    assert!(
        failed_message
            .as_deref()
            .is_some_and(|m| m.contains("HTTP 500")),
        "expected the Failed event to carry the HTTP 500 reason, got: {failed_message:?}"
    );
}

#[tokio::test]
async fn parallel_emits_progress_events_for_each_chapter() {
    let mut server = mockito::Server::new_async().await;
    for n in 1..=3 {
        server
            .mock("GET", format!("/foo/chuong-{}/", n).as_str())
            .with_status(200)
            .with_body(fake_chapter_html(n))
            .create_async()
            .await;
    }
    let dir = tempfile::tempdir().unwrap();
    let prompt = Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip);
    let events: Arc<Mutex<Vec<ProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let progress: ProgressCallback = Arc::new(move |event| captured.lock().unwrap().push(event));

    let _ = crawl_chapters_parallel(ParallelParams {
        adapter: &Metruyenhot,
        chapters: chapter_refs(&server.url(), &[1, 2, 3]),
        output_root: dir.path().to_path_buf(),
        if_exists: ExistingFilePolicy::Skip,
        workers: 2,
        novel_title: None,
        fast_skip: false,
        prompt,
        progress: Some(progress),
    })
    .await;
    let captured = events.lock().unwrap();
    let starts = captured
        .iter()
        .filter(|e| matches!(e, ProgressEvent::Started { .. }))
        .count();
    let completes = captured
        .iter()
        .filter(|e| matches!(e, ProgressEvent::Completed { .. }))
        .count();
    assert_eq!(starts, 3);
    assert_eq!(completes, 3);
}

/// An adapter the tests drive directly: it serves canned chapter content
/// without HTTP, counts attempts and peak concurrency, and can return the
/// typed [`SourceError`]s the runner is supposed to act on. `Box::leak`ed by
/// [`leak`] because the runners take a `&'static dyn SiteAdapter`.
struct FakeSource {
    /// Policy the runner reads for the clamp, pacing, and retry budget.
    policy: RatePolicy,
    /// Total `fetch_chapter` calls across the run.
    attempts: AtomicUsize,
    /// Fetches currently inside `fetch_chapter`.
    in_flight: AtomicUsize,
    /// Highest `in_flight` ever observed, i.e. the effective concurrency.
    peak_in_flight: AtomicUsize,
    /// The first N attempts return `RateLimited` before any succeed.
    rate_limit_first: usize,
    /// Chapters that always answer `Unentitled`.
    unentitled: Vec<u32>,
}

impl FakeSource {
    /// A source with the given policy that always serves content.
    fn new(policy: RatePolicy) -> Self {
        Self {
            policy,
            attempts: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            peak_in_flight: AtomicUsize::new(0),
            rate_limit_first: 0,
            unentitled: Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl novel_downloader::source::SiteAdapter for FakeSource {
    /// Stable id, also the `source` field of the clamp event.
    fn id(&self) -> &'static str {
        "fake"
    }

    /// Display name, unused by the runner.
    fn display_name(&self) -> &'static str {
        "Fake Source"
    }

    /// No real hosts: this adapter is never registered.
    fn hosts(&self) -> &'static [&'static str] {
        &[]
    }

    /// The policy the test configured.
    fn rate_policy(&self) -> RatePolicy {
        self.policy
    }

    /// Unused by the runner.
    async fn fetch_novel(&self, _url: &str) -> SourceResult<Novel> {
        unreachable!("the runner never fetches a novel")
    }

    /// Unused by the runner.
    async fn fetch_metadata(&self, _url: &str) -> SourceResult<Novel> {
        unreachable!("the runner never fetches metadata")
    }

    /// Count the attempt, track concurrency, then answer per configuration.
    async fn fetch_chapter(&self, chapter: &ChapterRef) -> SourceResult<ChapterContent> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak_in_flight.fetch_max(now, Ordering::SeqCst);
        // Long enough that genuinely concurrent workers overlap here.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);

        if self.unentitled.contains(&chapter.number) {
            return Err(SourceError::Unentitled(format!(
                "chapter {}",
                chapter.number
            )));
        }
        if attempt <= self.rate_limit_first {
            return Err(SourceError::RateLimited {
                source_name: "fake",
                message: "slow down".to_string(),
            });
        }
        Ok(ChapterContent {
            novel_title: "Truyện X".to_string(),
            chapter_title: format!("Chương {}", chapter.number),
            paragraphs: vec![format!("Body {}.", chapter.number)],
        })
    }
}

/// Leak a fake source so it satisfies the runners' `&'static` bound.
fn leak(source: FakeSource) -> &'static FakeSource {
    Box::leak(Box::new(source))
}

/// A policy that imposes nothing, matching metruyenhot's.
fn permissive_policy() -> RatePolicy {
    RatePolicy {
        max_concurrency: usize::MAX,
        min_delay: std::time::Duration::ZERO,
        max_retries: 0,
        backoff_base: std::time::Duration::ZERO,
    }
}

/// Collect every emitted event for later assertions.
fn recording_callback() -> (ProgressCallback, Arc<Mutex<Vec<ProgressEvent>>>) {
    let events: Arc<Mutex<Vec<ProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let callback: ProgressCallback = Arc::new(move |event| sink.lock().unwrap().push(event));
    (callback, events)
}

/// Locators the fake source never dereferences.
fn fake_refs(numbers: &[u32]) -> Vec<ChapterRef> {
    numbers
        .iter()
        .map(|&number| ChapterRef {
            number,
            title: None,
            locator: format!("fake://chuong-{number}"),
        })
        .collect()
}

/// Parallel params with the boilerplate filled in.
fn fake_parallel(
    adapter: &'static FakeSource,
    chapters: Vec<ChapterRef>,
    output_root: std::path::PathBuf,
    workers: usize,
    progress: Option<ProgressCallback>,
) -> ParallelParams {
    ParallelParams {
        adapter,
        chapters,
        output_root,
        if_exists: ExistingFilePolicy::Overwrite,
        workers,
        novel_title: None,
        fast_skip: false,
        prompt: Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip),
        progress,
    }
}

/// Sequential params with the boilerplate filled in.
fn fake_sequential(
    adapter: &'static FakeSource,
    chapters: Vec<ChapterRef>,
    output_root: std::path::PathBuf,
    progress: Option<ProgressCallback>,
) -> SequentialParams {
    SequentialParams {
        adapter,
        chapters,
        output_root,
        if_exists: ExistingFilePolicy::Overwrite,
        delay: 0.0,
        novel_title: None,
        fast_skip: false,
        prompt: Arc::new(|_: &std::path::Path| ExistingChapterDecision::Skip),
        progress,
    }
}

#[tokio::test]
async fn parallel_clamps_workers_to_the_policy_max_concurrency() {
    let adapter = leak(FakeSource::new(RatePolicy {
        max_concurrency: 2,
        ..permissive_policy()
    }));
    let dir = tempfile::tempdir().unwrap();
    let (callback, events) = recording_callback();
    let outcome = crawl_chapters_parallel(fake_parallel(
        adapter,
        fake_refs(&[1, 2, 3, 4, 5, 6]),
        dir.path().to_path_buf(),
        8,
        Some(callback),
    ))
    .await;

    assert_eq!(outcome.failures.len(), 0);
    assert!(
        adapter.peak_in_flight.load(Ordering::SeqCst) <= 2,
        "peak concurrency was {}",
        adapter.peak_in_flight.load(Ordering::SeqCst)
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, ProgressEvent::ConcurrencyClamped { .. })),
        "the clamp must be reported, not applied silently"
    );
}

#[tokio::test]
async fn parallel_emits_one_clamp_event_naming_the_source() {
    let adapter = leak(FakeSource::new(RatePolicy {
        max_concurrency: 3,
        ..permissive_policy()
    }));
    let dir = tempfile::tempdir().unwrap();
    let (callback, events) = recording_callback();
    crawl_chapters_parallel(fake_parallel(
        adapter,
        fake_refs(&[1, 2]),
        dir.path().to_path_buf(),
        7,
        Some(callback),
    ))
    .await;

    let events = events.lock().unwrap();
    let clamps: Vec<ProgressEvent> = events
        .iter()
        .filter(|event| matches!(event, ProgressEvent::ConcurrencyClamped { .. }))
        .cloned()
        .collect();
    assert_eq!(
        clamps,
        vec![ProgressEvent::ConcurrencyClamped {
            requested: 7,
            effective: 3,
            source: "fake",
        }]
    );
}

#[tokio::test]
async fn parallel_stays_silent_when_the_policy_allows_the_requested_workers() {
    let adapter = leak(FakeSource::new(permissive_policy()));
    let dir = tempfile::tempdir().unwrap();
    let (callback, events) = recording_callback();
    crawl_chapters_parallel(fake_parallel(
        adapter,
        fake_refs(&[1, 2]),
        dir.path().to_path_buf(),
        4,
        Some(callback),
    ))
    .await;

    assert!(
        !events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, ProgressEvent::ConcurrencyClamped { .. })),
        "permissive policy must not clamp"
    );
}

#[tokio::test]
async fn sequential_retries_a_rate_limited_chapter_until_it_succeeds() {
    let mut source = FakeSource::new(RatePolicy {
        max_retries: 3,
        backoff_base: std::time::Duration::from_millis(5),
        ..permissive_policy()
    });
    source.rate_limit_first = 2;
    let adapter = leak(source);
    let dir = tempfile::tempdir().unwrap();
    let outcome = crawl_chapters_sequential(fake_sequential(
        adapter,
        fake_refs(&[1]),
        dir.path().to_path_buf(),
        None,
    ))
    .await;

    assert_eq!(outcome.failures, vec![]);
    assert_eq!(adapter.attempts.load(Ordering::SeqCst), 3);
    let output_dir = outcome.output_dir.expect("output_dir set");
    assert!(output_dir.join("chapter_0001.html").exists());
}

#[tokio::test]
async fn sequential_reports_rate_limiting_after_the_retry_budget_is_spent() {
    let mut source = FakeSource::new(RatePolicy {
        max_retries: 2,
        backoff_base: std::time::Duration::from_millis(5),
        ..permissive_policy()
    });
    source.rate_limit_first = usize::MAX;
    let adapter = leak(source);
    let dir = tempfile::tempdir().unwrap();
    let outcome = crawl_chapters_sequential(fake_sequential(
        adapter,
        fake_refs(&[1]),
        dir.path().to_path_buf(),
        None,
    ))
    .await;

    // One initial attempt plus max_retries.
    assert_eq!(adapter.attempts.load(Ordering::SeqCst), 3);
    assert_eq!(outcome.failures.len(), 1);
    let message = &outcome.failures[0].1;
    assert!(
        message.to_lowercase().contains("rate limit"),
        "failure must name rate limiting, got: {message}"
    );
    assert!(
        message.contains('2'),
        "failure must name the spent retry budget, got: {message}"
    );
}

#[tokio::test]
async fn sequential_reports_an_unentitled_chapter_distinctly_and_keeps_going() {
    let mut source = FakeSource::new(permissive_policy());
    source.unentitled = vec![1];
    let adapter = leak(source);
    let dir = tempfile::tempdir().unwrap();
    let outcome = crawl_chapters_sequential(fake_sequential(
        adapter,
        fake_refs(&[1, 2]),
        dir.path().to_path_buf(),
        None,
    ))
    .await;

    assert_eq!(outcome.failures.len(), 1, "only chapter 1 fails");
    assert_eq!(outcome.failures[0].0, 1);
    let message = &outcome.failures[0].1;
    assert!(
        message.to_lowercase().contains("not available"),
        "unentitled failures must read differently from other errors, got: {message}"
    );
    assert!(
        !message.to_lowercase().contains("rate limit"),
        "unentitled is not rate limiting, got: {message}"
    );
    let output_dir = outcome.output_dir.expect("chapter 2 still ran");
    assert!(output_dir.join("chapter_0002.html").exists());
}

#[tokio::test]
async fn sequential_spaces_requests_by_the_policy_min_delay() {
    let adapter = leak(FakeSource::new(RatePolicy {
        min_delay: std::time::Duration::from_millis(60),
        ..permissive_policy()
    }));
    let dir = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    crawl_chapters_sequential(fake_sequential(
        adapter,
        fake_refs(&[1, 2, 3]),
        dir.path().to_path_buf(),
        None,
    ))
    .await;

    // Three requests are separated by two gaps of at least min_delay each.
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(120),
        "run took {:?}, too fast to have honored min_delay",
        started.elapsed()
    );
}
