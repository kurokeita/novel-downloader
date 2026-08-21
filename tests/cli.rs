use novel_downloader::cli::{
    CliOptions, chapter_range, delay_override_notice, parse_from, validate_chapter_range,
    validate_shared_options,
};
use novel_downloader::crawler::ExistingFilePolicy;
use novel_downloader::source::RatePolicy;
use std::time::Duration;

#[test]
fn parse_from_uses_defaults_when_only_base_url_given() {
    let parsed = parse_from(["novel-downloader", "https://metruyenhotvn.com/foo"]).unwrap();
    assert_eq!(
        parsed.base_url.as_deref(),
        Some("https://metruyenhotvn.com/foo")
    );
    assert_eq!(parsed.options.output_root, "output");
    assert_eq!(parsed.options.workers, 1);
    assert!(!parsed.options.epub);
    assert!(!parsed.options.epub_only);
    assert!(!parsed.options.fast_skip);
    assert!(!parsed.options.interactive);
    assert!((parsed.options.delay - 0.5).abs() < 1e-9);
    assert_eq!(parsed.options.if_exists, ExistingFilePolicy::Ask);
    assert!(parsed.options.start.is_none());
    assert!(parsed.options.end.is_none());
    assert!(parsed.options.chapter_dir.is_none());
    assert!(parsed.options.font_path.is_none());
}

#[test]
fn parse_from_accepts_full_flag_set() {
    let parsed = parse_from([
        "novel-downloader",
        "https://metruyenhotvn.com/foo",
        "--start",
        "10",
        "--end",
        "12",
        "--output-root",
        "/tmp/out",
        "--delay",
        "1.5",
        "--workers",
        "4",
        "--epub",
        "--font-path",
        "/tmp/Bokerlam.ttf",
        "--if-exists",
        "skip",
        "--fast-skip",
        "-i",
    ])
    .unwrap();
    assert_eq!(parsed.options.start, Some(10));
    assert_eq!(parsed.options.end, Some(12));
    assert_eq!(parsed.options.output_root, "/tmp/out");
    assert!((parsed.options.delay - 1.5).abs() < 1e-9);
    assert_eq!(parsed.options.workers, 4);
    assert!(parsed.options.epub);
    assert_eq!(
        parsed.options.font_path.as_deref(),
        Some("/tmp/Bokerlam.ttf")
    );
    assert_eq!(parsed.options.if_exists, ExistingFilePolicy::Skip);
    assert!(parsed.options.fast_skip);
    assert!(parsed.options.interactive);
}

#[test]
fn parse_from_accepts_epub_only_with_chapter_dir() {
    let parsed = parse_from([
        "novel-downloader",
        "https://x.me/foo",
        "--epub-only",
        "--chapter-dir",
        "/tmp/chapters",
    ])
    .unwrap();
    assert!(parsed.options.epub_only);
    assert_eq!(parsed.options.chapter_dir.as_deref(), Some("/tmp/chapters"));
}

#[test]
fn parse_from_rejects_invalid_if_exists_value() {
    let err = parse_from([
        "novel-downloader",
        "https://x.me/foo",
        "--if-exists",
        "bogus",
    ])
    .unwrap_err();
    assert!(err.to_lowercase().contains("if-exists") || err.contains("bogus"));
}

#[test]
fn parse_from_allows_interactive_without_base_url() {
    let parsed = parse_from(["novel-downloader", "-i"]).unwrap();
    assert!(parsed.base_url.is_none());
    assert!(parsed.options.interactive);
}

#[test]
fn validate_chapter_range_rejects_zero_or_negative() {
    assert!(validate_chapter_range(0, 5).is_some());
    assert!(validate_chapter_range(2, 0).is_some());
}

#[test]
fn validate_chapter_range_rejects_start_greater_than_end() {
    assert!(validate_chapter_range(10, 5).is_some());
}

#[test]
fn validate_chapter_range_accepts_valid_range() {
    assert!(validate_chapter_range(1, 1).is_none());
    assert!(validate_chapter_range(1, 100).is_none());
}

#[test]
fn chapter_range_inclusive_returns_full_sequence() {
    assert_eq!(chapter_range(2, 5), vec![2, 3, 4, 5]);
    assert_eq!(chapter_range(7, 7), vec![7]);
}

#[test]
fn validate_shared_options_rejects_zero_workers() {
    let opts = CliOptions {
        workers: 0,
        if_exists: ExistingFilePolicy::Skip,
        ..CliOptions::default()
    };
    assert!(validate_shared_options(&opts).is_some());
}

#[test]
fn validate_shared_options_rejects_parallel_workers_with_ask_policy() {
    let opts = CliOptions {
        workers: 4,
        if_exists: ExistingFilePolicy::Ask,
        ..CliOptions::default()
    };
    let msg = validate_shared_options(&opts).unwrap();
    assert!(msg.contains("--workers"));
}

#[test]
fn validate_shared_options_accepts_parallel_workers_with_skip_policy() {
    let opts = CliOptions {
        workers: 4,
        if_exists: ExistingFilePolicy::Skip,
        ..CliOptions::default()
    };
    assert!(validate_shared_options(&opts).is_none());
}

#[test]
fn parse_from_defaults_allow_any_host_to_false() {
    let parsed = parse_from(["novel-downloader", "https://metruyenhotvn.com/foo"]).unwrap();
    assert!(!parsed.options.allow_any_host);
}

#[test]
fn parse_from_accepts_allow_any_host_flag() {
    let parsed = parse_from([
        "novel-downloader",
        "https://metruyenhotvn.com/foo",
        "--allow-any-host",
    ])
    .unwrap();
    assert!(parsed.options.allow_any_host);
}

/// A policy shaped like xtruyen's, which requires half a second per request.
const PACED_POLICY: RatePolicy = RatePolicy {
    max_concurrency: 2,
    min_delay: Duration::from_millis(500),
    max_retries: 3,
    backoff_base: Duration::from_secs(2),
};

/// The policy metruyenhot and khodocsach declare, which constrains nothing.
const OPEN_POLICY: RatePolicy = RatePolicy {
    max_concurrency: usize::MAX,
    min_delay: Duration::ZERO,
    max_retries: 2,
    backoff_base: Duration::from_secs(2),
};

#[test]
fn delay_override_notice_reports_the_raised_delay_and_names_the_source() {
    let notice = delay_override_notice("xtruyen", 0.0, &PACED_POLICY)
        .expect("raising the delay must be reported, not applied silently");
    assert!(notice.contains("xtruyen"), "got {notice}");
    assert!(
        notice.contains("0.5"),
        "the notice must name the delay in force, got {notice}"
    );
}

#[test]
fn delay_override_notice_is_silent_when_the_request_is_within_policy() {
    assert_eq!(delay_override_notice("xtruyen", 2.0, &PACED_POLICY), None);
    assert_eq!(
        delay_override_notice("metruyenhot", 0.0, &OPEN_POLICY),
        None
    );
}
