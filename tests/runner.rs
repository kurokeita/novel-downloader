use novel_downloader::crawler::{CrawlStatus, ExistingChapterDecision, ExistingFilePolicy};
use novel_downloader::runner::{
    ParallelParams, ProgressCallback, ProgressEvent, RunnerOutcome, SequentialParams,
    crawl_chapters_parallel, crawl_chapters_sequential,
};
use novel_downloader::source::ChapterRef;
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
