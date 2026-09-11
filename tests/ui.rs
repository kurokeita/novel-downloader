use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use novel_downloader::crawler::CrawlStatus;
use novel_downloader::crawler::ExistingFilePolicy;
use novel_downloader::ui::{
    CrawlMode, DownloadLogEntry, DownloadProgress, PathInput, PathInputAction, Select,
    SelectOption, SummaryParams, TextArea, TextAreaAction, TextAreaLayout, TextInput,
    TextInputAction, build_summary, chapter_summary_lines, epub_destination_dir, expand_tilde,
    format_hms, gauge_label, longest_common_prefix, path_completions, prompt_block_height,
    wrap_text, wrapped_cursor_position,
};
use std::time::Duration;

/// Build a `KeyEvent` with no modifiers — a tiny ergonomic helper for the
/// tests below.
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn text_input_appends_chars_typed_by_user() {
    let mut input = TextInput::new();
    input.handle_key(key(KeyCode::Char('h')));
    input.handle_key(key(KeyCode::Char('i')));
    assert_eq!(input.value(), "hi");
}

#[test]
fn text_input_backspace_removes_last_char() {
    let mut input = TextInput::new();
    input.set_value("ab");
    input.handle_key(key(KeyCode::Backspace));
    assert_eq!(input.value(), "a");
    input.handle_key(key(KeyCode::Backspace));
    assert_eq!(input.value(), "");
    input.handle_key(key(KeyCode::Backspace));
    assert_eq!(input.value(), "");
}

#[test]
fn text_input_enter_emits_submit() {
    let mut input = TextInput::new();
    input.set_value("done");
    let action = input.handle_key(key(KeyCode::Enter));
    assert_eq!(action, TextInputAction::Submit);
}

#[test]
fn text_input_esc_emits_cancel() {
    let mut input = TextInput::new();
    let action = input.handle_key(key(KeyCode::Esc));
    assert_eq!(action, TextInputAction::Cancel);
}

#[test]
fn text_input_ctrl_c_emits_quit_not_text_insert() {
    let mut input = TextInput::new();
    let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = input.handle_key(event);
    assert_eq!(action, TextInputAction::Quit);
    assert_eq!(input.value(), "", "Ctrl+C must not insert 'c'");
}

#[test]
fn text_input_plain_c_still_inserts_character() {
    let mut input = TextInput::new();
    input.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert_eq!(input.value(), "c");
}

#[test]
fn text_input_runs_validator_on_submit() {
    let mut input = TextInput::with_validator(|value| {
        if value.is_empty() {
            Some("required".to_string())
        } else {
            None
        }
    });
    let action = input.handle_key(key(KeyCode::Enter));
    assert_eq!(action, TextInputAction::Invalid("required".to_string()));
    assert_eq!(input.error(), Some("required"));
    input.set_value("ok");
    assert_eq!(
        input.handle_key(key(KeyCode::Enter)),
        TextInputAction::Submit
    );
    assert!(input.error().is_none());
}

#[test]
fn select_arrow_keys_move_selection() {
    let mut select: Select<&'static str> = Select::new(vec![
        SelectOption {
            label: "A".into(),
            value: "a",
            hint: None,
        },
        SelectOption {
            label: "B".into(),
            value: "b",
            hint: None,
        },
        SelectOption {
            label: "C".into(),
            value: "c",
            hint: None,
        },
    ]);
    assert_eq!(select.selected_value(), Some(&"a"));
    select.handle_key(key(KeyCode::Down));
    assert_eq!(select.selected_value(), Some(&"b"));
    select.handle_key(key(KeyCode::Down));
    assert_eq!(select.selected_value(), Some(&"c"));
    // Wraps around to the first item on further Down.
    select.handle_key(key(KeyCode::Down));
    assert_eq!(select.selected_value(), Some(&"a"));
    select.handle_key(key(KeyCode::Up));
    assert_eq!(select.selected_value(), Some(&"c"));
}

#[test]
fn select_enter_submits_current_value() {
    let mut select: Select<u8> = Select::new(vec![
        SelectOption {
            label: "1".into(),
            value: 1,
            hint: None,
        },
        SelectOption {
            label: "2".into(),
            value: 2,
            hint: None,
        },
    ]);
    select.handle_key(key(KeyCode::Down));
    let action = select.handle_key(key(KeyCode::Enter));
    assert!(matches!(
        action,
        novel_downloader::ui::SelectAction::Submit(2)
    ));
}

#[test]
fn select_esc_cancels() {
    let mut select: Select<&'static str> = Select::new(vec![SelectOption {
        label: "A".into(),
        value: "a",
        hint: None,
    }]);
    let action = select.handle_key(key(KeyCode::Esc));
    assert!(matches!(action, novel_downloader::ui::SelectAction::Cancel));
}

#[test]
fn select_ctrl_c_emits_quit() {
    let mut select: Select<&'static str> = Select::new(vec![SelectOption {
        label: "A".into(),
        value: "a",
        hint: None,
    }]);
    let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = select.handle_key(event);
    assert!(matches!(action, novel_downloader::ui::SelectAction::Quit));
}

#[test]
fn select_with_initial_value_starts_on_that_option() {
    let select: Select<&'static str> = Select::with_initial(
        vec![
            SelectOption {
                label: "A".into(),
                value: "a",
                hint: None,
            },
            SelectOption {
                label: "B".into(),
                value: "b",
                hint: None,
            },
            SelectOption {
                label: "C".into(),
                value: "c",
                hint: None,
            },
        ],
        &"b",
    );
    assert_eq!(select.selected_value(), Some(&"b"));
}

#[test]
fn download_progress_records_started_and_completed() {
    let mut progress = DownloadProgress::new(3);
    assert_eq!(progress.total, 3);
    assert_eq!(progress.completed, 0);
    progress.record_started(1);
    assert_eq!(progress.current_chapter, Some(1));
    progress.record_completed(1, CrawlStatus::Written);
    assert_eq!(progress.completed, 1);
    assert_eq!(progress.log.last(), Some(&DownloadLogEntry::Ok(1)));
    progress.record_completed(2, CrawlStatus::Skipped);
    assert_eq!(progress.completed, 2);
    assert_eq!(progress.log.last(), Some(&DownloadLogEntry::Skip(2)));
}

#[test]
fn download_progress_records_failures() {
    let mut progress = DownloadProgress::new(2);
    progress.record_started(1);
    progress.record_failed(1, "HTTP 503".to_string());
    assert_eq!(progress.failed, 1);
    assert_eq!(
        progress.log.last(),
        Some(&DownloadLogEntry::Fail(1, "HTTP 503".to_string()))
    );
    // failed entries also count as "advanced" for percentage.
    assert_eq!(progress.advanced(), 1);
}

#[test]
fn download_progress_finish_marks_done() {
    let mut progress = DownloadProgress::new(1);
    progress.record_started(1);
    progress.record_completed(1, CrawlStatus::Written);
    assert!(!progress.done);
    progress.finish();
    assert!(progress.done);
}

#[test]
fn download_progress_log_caps_to_window() {
    let cap = 5;
    let mut progress = DownloadProgress::with_log_capacity(20, cap);
    for n in 1..=15u32 {
        progress.record_completed(n, CrawlStatus::Written);
    }
    assert_eq!(progress.log.len(), cap);
    // Most recent entries should be retained.
    assert_eq!(progress.log.last(), Some(&DownloadLogEntry::Ok(15)));
    assert_eq!(progress.log.first(), Some(&DownloadLogEntry::Ok(11)));
}

#[test]
fn download_progress_default_log_capacity_is_generous() {
    // Big enough that the activity log can fill a tall terminal without
    // dropping recent entries.
    let progress = DownloadProgress::new(0);
    assert!(
        progress.log_capacity >= 200,
        "default capacity too small: {}",
        progress.log_capacity
    );
}

#[test]
fn format_hms_always_renders_an_hour_field() {
    assert_eq!(format_hms(Duration::from_secs(0)), "00:00:00");
    assert_eq!(format_hms(Duration::from_secs(4 * 60 + 12)), "00:04:12");
    assert_eq!(
        format_hms(Duration::from_secs(3600 + 9 * 60 + 6)),
        "01:09:06"
    );
}

#[test]
fn format_hms_does_not_wrap_past_a_day() {
    // A very long run keeps counting hours rather than rolling over.
    assert_eq!(format_hms(Duration::from_secs(30 * 3600 + 61)), "30:01:01");
}

#[test]
fn gauge_label_shows_tally_elapsed_and_estimate_in_flight() {
    let mut progress = DownloadProgress::new(100);
    let t0 = progress.started_at;
    for n in 1..=10u32 {
        progress.record_completed_at(
            n,
            CrawlStatus::Written,
            t0 + Duration::from_secs(n as u64 * 2),
        );
    }
    let label = gauge_label(&progress, t0 + Duration::from_secs(20));
    assert!(label.contains("10 / 100"), "tally missing: {label}");
    assert!(label.contains("10%"), "percentage missing: {label}");
    assert!(label.contains("00:00:20"), "elapsed missing: {label}");
    assert!(label.contains("00:03:00"), "estimate missing: {label}");
}

#[test]
fn gauge_label_shows_a_placeholder_when_the_estimate_is_withheld() {
    let mut progress = DownloadProgress::new(100);
    let t0 = progress.started_at;
    progress.record_completed_at(1, CrawlStatus::Written, t0 + Duration::from_millis(10));
    let label = gauge_label(&progress, t0 + Duration::from_millis(20));
    assert!(label.contains("1 / 100"), "tally missing: {label}");
    assert!(label.contains("00:00:00"), "elapsed missing: {label}");
    // The segment stays present so the label does not change width mid-run.
    assert!(label.contains("ETA"), "ETA segment missing: {label}");
    assert!(label.contains('—'), "placeholder missing: {label}");
}

#[test]
fn gauge_label_drops_the_estimate_once_the_run_has_ended() {
    let mut progress = DownloadProgress::new(2);
    let t0 = progress.started_at;
    progress.record_completed_at(1, CrawlStatus::Written, t0 + Duration::from_secs(2));
    progress.record_completed_at(2, CrawlStatus::Written, t0 + Duration::from_secs(6));
    progress.finish_at(t0 + Duration::from_secs(7));
    let label = gauge_label(&progress, t0 + Duration::from_secs(300));
    assert!(label.contains("2 / 2"), "tally missing: {label}");
    assert!(
        label.contains("00:00:07"),
        "frozen elapsed missing: {label}"
    );
    assert!(!label.contains("ETA"), "estimate should be gone: {label}");
    assert!(!label.contains('—'), "placeholder should be gone: {label}");
}

#[test]
fn download_progress_estimates_from_steady_throughput() {
    let mut progress = DownloadProgress::new(100);
    let t0 = progress.started_at;
    // Ten chapters, one every two seconds: nine intervals over 18s is 0.5/s.
    for n in 1..=10u32 {
        progress.record_completed_at(
            n,
            CrawlStatus::Written,
            t0 + Duration::from_secs(n as u64 * 2),
        );
    }
    let eta = progress.eta(t0 + Duration::from_secs(20)).unwrap();
    assert_eq!(eta, Duration::from_secs(180));
}

#[test]
fn download_progress_eta_converges_on_the_slow_rate_after_a_burst() {
    let mut progress = DownloadProgress::new(1000);
    let t0 = progress.started_at;
    for n in 1..=900u32 {
        progress.record_completed_at(
            n,
            CrawlStatus::Skipped,
            t0 + Duration::from_micros(n as u64 * 30),
        );
    }
    // Real downloads follow at roughly one chapter per second. Once 20 of them
    // have evicted every burst sample, the estimate reflects only the slow rate.
    for n in 901..=920u32 {
        progress.record_completed_at(
            n,
            CrawlStatus::Written,
            t0 + Duration::from_secs(n as u64 - 900),
        );
    }
    let now = t0 + Duration::from_secs(20);
    let eta = progress.eta(now).unwrap();
    assert_eq!(progress.advanced(), 920);
    // 80 chapters left at ~1/s, so the estimate must be in that neighborhood,
    // not the near-zero figure the burst alone would imply.
    assert!(
        eta >= Duration::from_secs(70) && eta <= Duration::from_secs(90),
        "expected ~80s, got {eta:?}"
    );
}

#[test]
fn download_progress_eta_grows_while_the_run_is_stalled() {
    let mut progress = DownloadProgress::new(100);
    let t0 = progress.started_at;
    for n in 1..=10u32 {
        progress.record_completed_at(
            n,
            CrawlStatus::Written,
            t0 + Duration::from_secs(n as u64 * 2),
        );
    }
    let before = progress.eta(t0 + Duration::from_secs(20)).unwrap();
    // A rate-limited source can hold the run silent for minutes. The estimate
    // must account for that wait rather than reporting its pre-stall figure.
    let after = progress.eta(t0 + Duration::from_secs(200)).unwrap();
    assert!(
        after > before,
        "expected growth, got {before:?} -> {after:?}"
    );
}

#[test]
fn download_progress_withholds_eta_below_two_samples() {
    let mut progress = DownloadProgress::new(100);
    let t0 = progress.started_at;
    assert_eq!(progress.eta(t0 + Duration::from_secs(5)), None);
    progress.record_completed_at(1, CrawlStatus::Written, t0 + Duration::from_secs(1));
    assert_eq!(progress.eta(t0 + Duration::from_secs(5)), None);
}

#[test]
fn download_progress_withholds_eta_when_no_chapters_are_expected() {
    let mut progress = DownloadProgress::new(0);
    let t0 = progress.started_at;
    progress.record_completed_at(1, CrawlStatus::Written, t0 + Duration::from_secs(1));
    progress.record_completed_at(2, CrawlStatus::Written, t0 + Duration::from_secs(4));
    assert_eq!(progress.eta(t0 + Duration::from_secs(5)), None);
}

#[test]
fn download_progress_withholds_eta_once_the_run_has_ended() {
    let mut progress = DownloadProgress::new(100);
    let t0 = progress.started_at;
    progress.record_completed_at(1, CrawlStatus::Written, t0 + Duration::from_secs(1));
    progress.record_completed_at(2, CrawlStatus::Written, t0 + Duration::from_secs(4));
    assert!(progress.eta(t0 + Duration::from_secs(4)).is_some());
    progress.finish_at(t0 + Duration::from_secs(5));
    assert_eq!(progress.eta(t0 + Duration::from_secs(5)), None);
}

#[test]
fn download_progress_withholds_eta_for_an_instantaneous_burst() {
    // 900 already-on-disk chapters land in milliseconds when resuming with
    // --fast-skip. A whole-run average would report "ETA 0s" here, which reads
    // as "about to finish" exactly when the real download has not started.
    let mut progress = DownloadProgress::new(1000);
    let t0 = progress.started_at;
    for n in 1..=900u32 {
        progress.record_completed_at(
            n,
            CrawlStatus::Skipped,
            t0 + Duration::from_micros(n as u64 * 30),
        );
    }
    assert_eq!(progress.rate_samples(), 20);
    assert_eq!(progress.eta(t0 + Duration::from_millis(50)), None);
}

#[test]
fn download_progress_window_records_terminal_events_only() {
    let mut progress = DownloadProgress::new(10);
    let t0 = progress.started_at;
    progress.record_started(1);
    assert_eq!(progress.rate_samples(), 0);
    progress.record_completed_at(1, CrawlStatus::Written, t0 + Duration::from_secs(1));
    progress.record_failed_at(2, "HTTP 503".to_string(), t0 + Duration::from_secs(2));
    assert_eq!(progress.rate_samples(), 2);
    // A run-scoped note advances no chapter, so it contributes no sample.
    progress.record_note("rate policy capped workers".to_string());
    assert_eq!(progress.rate_samples(), 2);
}

#[test]
fn download_progress_window_caps_while_advanced_keeps_counting() {
    let mut progress = DownloadProgress::new(200);
    let t0 = progress.started_at;
    for n in 1..=60u32 {
        progress.record_completed_at(n, CrawlStatus::Written, t0 + Duration::from_secs(n as u64));
    }
    assert_eq!(progress.advanced(), 60);
    assert_eq!(progress.rate_samples(), 20);
}

#[test]
fn download_progress_reports_elapsed_before_any_chapter_event() {
    let progress = DownloadProgress::new(5);
    let now = progress.started_at + Duration::from_secs(3);
    assert_eq!(progress.elapsed(now), Duration::from_secs(3));
}

#[test]
fn download_progress_elapsed_advances_with_the_clock() {
    let progress = DownloadProgress::new(5);
    let first = progress.elapsed(progress.started_at + Duration::from_secs(2));
    let second = progress.elapsed(progress.started_at + Duration::from_secs(7));
    assert!(second > first);
    assert_eq!(second - first, Duration::from_secs(5));
}

#[test]
fn download_progress_elapsed_freezes_at_the_end_of_the_run() {
    let mut progress = DownloadProgress::new(1);
    progress.record_completed(1, CrawlStatus::Written);
    let ended = progress.started_at + Duration::from_secs(12);
    progress.finish_at(ended);
    assert!(progress.done);
    // Reading long after the run ended still reports the run's own duration.
    let much_later = progress.started_at + Duration::from_secs(600);
    assert_eq!(progress.elapsed(much_later), Duration::from_secs(12));
}

#[test]
fn download_progress_aborted_run_freezes_elapsed_too() {
    let mut progress = DownloadProgress::new(10);
    progress.record_completed(1, CrawlStatus::Written);
    // Esc aborts the run: the screen marks it finished with chapters outstanding.
    let aborted_at = progress.started_at + Duration::from_secs(4);
    progress.finish_at(aborted_at);
    assert!(progress.advanced() < progress.total);
    assert_eq!(
        progress.elapsed(progress.started_at + Duration::from_secs(90)),
        Duration::from_secs(4)
    );
}

#[test]
fn longest_common_prefix_handles_empty_and_single() {
    let empty: Vec<String> = vec![];
    assert_eq!(longest_common_prefix(&empty), "");
    assert_eq!(longest_common_prefix(&["only".to_string()]), "only");
}

#[test]
fn longest_common_prefix_returns_shared_start() {
    let inputs = vec![
        "foobar".to_string(),
        "foobaz".to_string(),
        "fooqux".to_string(),
    ];
    assert_eq!(longest_common_prefix(&inputs), "foo");
}

#[test]
fn longest_common_prefix_returns_empty_when_no_overlap() {
    let inputs = vec!["abc".to_string(), "xyz".to_string()];
    assert_eq!(longest_common_prefix(&inputs), "");
}

#[test]
fn path_completions_lists_children_matching_prefix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), b"a").unwrap();
    std::fs::write(dir.path().join("alpine.ttf"), b"a").unwrap();
    std::fs::write(dir.path().join("beta.txt"), b"b").unwrap();

    let prefix = format!("{}/al", dir.path().display());
    let mut completions = path_completions(&prefix);
    completions.sort();
    assert_eq!(completions.len(), 2);
    assert!(completions[0].ends_with("alpha.txt"));
    assert!(completions[1].ends_with("alpine.ttf"));
}

#[test]
fn path_completions_returns_directory_listing_for_trailing_slash() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("one.txt"), b"x").unwrap();
    std::fs::write(dir.path().join("two.txt"), b"x").unwrap();
    let prefix = format!("{}/", dir.path().display());
    let completions = path_completions(&prefix);
    assert_eq!(completions.len(), 2);
}

#[test]
fn path_input_tab_completes_to_common_prefix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), b"a").unwrap();
    std::fs::write(dir.path().join("alpine.ttf"), b"a").unwrap();
    let mut input = PathInput::new();
    let typed = format!("{}/al", dir.path().display());
    input.set_value(&typed);
    input.refresh_completions();
    let action = input.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(action, PathInputAction::Continue);
    // Should have advanced to the longest common prefix `<dir>/alp`.
    assert!(
        input.value().ends_with("alp"),
        "expected value ending in 'alp', got: {}",
        input.value()
    );
}

#[test]
fn path_input_down_key_navigates_suggestions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), b"a").unwrap();
    std::fs::write(dir.path().join("alpine.ttf"), b"a").unwrap();
    let mut input = PathInput::new();
    input.set_value(format!("{}/al", dir.path().display()));
    input.refresh_completions();
    assert!(input.suggestions().len() >= 2);
    assert_eq!(input.highlighted(), None);
    input.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(input.highlighted(), Some(0));
    input.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(input.highlighted(), Some(1));
}

#[test]
fn path_input_enter_on_highlighted_replaces_value() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), b"a").unwrap();
    std::fs::write(dir.path().join("alpine.ttf"), b"a").unwrap();
    let mut input = PathInput::new();
    input.set_value(format!("{}/al", dir.path().display()));
    input.refresh_completions();
    input.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let action = input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, PathInputAction::Continue);
    assert!(input.highlighted().is_none(), "highlight clears after pick");
    let suggested_first_child_name = "alpha.txt";
    assert!(
        input.value().ends_with(suggested_first_child_name)
            || input.value().ends_with("alpine.ttf"),
        "value should be a full child path, got: {}",
        input.value()
    );
}

#[test]
fn path_input_enter_without_highlight_submits() {
    let mut input = PathInput::new();
    input.set_value("/tmp/foo");
    let action = input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, PathInputAction::Submit);
}

#[test]
fn path_input_esc_cancels() {
    let mut input = PathInput::new();
    let action = input.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(action, PathInputAction::Cancel);
}

#[test]
fn path_input_ctrl_c_emits_quit() {
    let mut input = PathInput::new();
    let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = input.handle_key(event);
    assert_eq!(action, PathInputAction::Quit);
    assert_eq!(input.value(), "", "Ctrl+C must not insert 'c'");
}

#[test]
fn chapter_summary_reports_the_count_and_the_sites_own_label() {
    assert_eq!(
        chapter_summary_lines(3610, Some(3610), Some("Chương 3634 : Vĩnh hằng")),
        vec![
            "Chapters: 3610".to_string(),
            "Latest: Chương 3634 : Vĩnh hằng".to_string()
        ],
        "3610 positions can end at a chapter the site calls 3634, so the count and \
         the label are reported rather than the position"
    );
}

#[test]
fn chapter_summary_falls_back_to_the_sequence_number_without_a_title() {
    assert_eq!(
        chapter_summary_lines(218, Some(218), None),
        vec!["Latest chapter: 218".to_string()],
        "a source that learns titles only on fetch has nothing else to show"
    );
}

#[test]
fn chapter_summary_ignores_a_blank_title() {
    assert_eq!(
        chapter_summary_lines(5, Some(5), Some("   ")),
        vec!["Latest chapter: 5".to_string()]
    );
}

#[test]
fn chapter_summary_is_empty_when_nothing_was_discovered() {
    assert!(chapter_summary_lines(0, None, None).is_empty());
}

#[test]
fn build_summary_names_the_source_when_it_fixes_the_pacing() {
    let chapters: Vec<u32> = (1..=3).collect();
    let output_root = std::path::PathBuf::from("/tmp/out");
    let summary = build_summary(SummaryParams {
        source: "xtruyen",
        base_url: "https://xtruyen.vn/truyen/mot-truyen",
        mode: CrawlMode::Crawl,
        output_root: output_root.as_path(),
        chapter_numbers: Some(chapters.as_slice()),
        delay: 0.5,
        workers: 2,
        if_exists: ExistingFilePolicy::Skip,
        chapter_dir: None,
        font_path: None,
        fast_skip: false,
        novel_title: None,
        novel_author: None,
        pacing_fixed_by_source: true,
    });
    assert!(
        summary.contains("Workers: 2 (required by xtruyen)"),
        "the summary must say the worker count is the site's, got:\n{summary}"
    );
    assert!(
        summary.contains("Delay: 0.5s (required by xtruyen)"),
        "the summary must say the delay is the site's, got:\n{summary}"
    );
}

#[test]
fn build_summary_leaves_pacing_unannotated_for_an_unconstrained_source() {
    let chapters: Vec<u32> = (1..=3).collect();
    let output_root = std::path::PathBuf::from("/tmp/out");
    let summary = build_summary(SummaryParams {
        source: "metruyenhot",
        base_url: "https://metruyenhotvn.com/foo",
        mode: CrawlMode::Crawl,
        output_root: output_root.as_path(),
        chapter_numbers: Some(chapters.as_slice()),
        delay: 0.0,
        workers: 8,
        if_exists: ExistingFilePolicy::Skip,
        chapter_dir: None,
        font_path: None,
        fast_skip: false,
        novel_title: None,
        novel_author: None,
        pacing_fixed_by_source: false,
    });
    assert!(summary.contains("Workers: 8\n"), "got:\n{summary}");
    assert!(
        !summary.contains("required by"),
        "an unconstrained source must not claim to require anything, got:\n{summary}"
    );
}

#[test]
fn build_summary_includes_every_chosen_option_for_crawl_epub() {
    let chapters: Vec<u32> = (1..=50).collect();
    let output_root = std::path::PathBuf::from("/tmp/out");
    let font_path = std::path::PathBuf::from("/tmp/MyFont.ttf");
    let summary = build_summary(SummaryParams {
        source: "metruyenhot",
        base_url: "https://metruyenhotvn.com/foo",
        mode: CrawlMode::CrawlEpub,
        output_root: output_root.as_path(),
        chapter_numbers: Some(chapters.as_slice()),
        delay: 0.5,
        workers: 4,
        if_exists: ExistingFilePolicy::Skip,
        chapter_dir: None,
        font_path: Some(font_path.as_path()),
        fast_skip: true,
        novel_title: Some("Tên Truyện"),
        novel_author: Some("Tác Giả"),
        pacing_fixed_by_source: false,
    });
    assert!(summary.contains("Base URL: https://metruyenhotvn.com/foo"));
    assert!(summary.contains("Title: Tên Truyện"));
    assert!(summary.contains("Author: Tác Giả"));
    assert!(summary.contains("Mode: Crawl chapters and build EPUB"));
    assert!(summary.contains("Output root: /tmp/out"));
    assert!(summary.contains("Chapters: 1 -> 50 (50 total)"));
    assert!(summary.contains("Workers: 4"));
    assert!(summary.contains("Delay: 0.5s"));
    assert!(summary.contains("If chapter exists: skip"));
    assert!(
        summary.contains("Fast skip: yes"),
        "expected explicit fast-skip line, got:\n{}",
        summary
    );
    assert!(summary.contains("Build EPUB: yes"));
    assert!(summary.contains("/tmp/MyFont.ttf"));
    assert!(
        summary.contains("EPUB output: /tmp/out/ten_truyen"),
        "expected the per-novel directory beneath the output root, got:\n{}",
        summary
    );
}

#[test]
fn prompt_block_height_grows_with_message_lines() {
    // Borders eat 2 rows; we need at least 3 visible content rows so a
    // single short line is not visually cramped.
    assert!(prompt_block_height("") >= 5);
    assert!(prompt_block_height("one liner") >= 5);
    // 10-line plan summary needs 10 content rows + 2 borders.
    let ten = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj";
    assert!(
        prompt_block_height(ten) >= 12,
        "expected >= 12 rows, got {}",
        prompt_block_height(ten)
    );
}

#[test]
fn build_summary_marks_fast_skip_no_when_disabled() {
    let chapters: Vec<u32> = vec![1, 2];
    let summary = build_summary(SummaryParams {
        source: "metruyenhot",
        base_url: "https://x/",
        mode: CrawlMode::Crawl,
        output_root: std::path::Path::new("output"),
        chapter_numbers: Some(chapters.as_slice()),
        delay: 0.0,
        workers: 1,
        if_exists: ExistingFilePolicy::Ask,
        chapter_dir: None,
        font_path: None,
        fast_skip: false,
        novel_title: None,
        novel_author: None,
        pacing_fixed_by_source: false,
    });
    assert!(summary.contains("Fast skip: no"));
    assert!(summary.contains("Build EPUB: no"));
    assert!(
        !summary.contains("EPUB output:"),
        "a download-only run writes no EPUB, got:\n{}",
        summary
    );
}

#[test]
fn build_summary_names_the_epub_destination_for_build_only() {
    let chapter_dir = std::path::PathBuf::from("/books/my-novel");
    let summary = build_summary(SummaryParams {
        source: "khodocsach",
        base_url: "https://khodocsach.com/my-novel.kds/",
        mode: CrawlMode::EpubOnly,
        output_root: std::path::Path::new("output"),
        chapter_numbers: None,
        delay: 0.0,
        workers: 1,
        if_exists: ExistingFilePolicy::Skip,
        chapter_dir: Some(chapter_dir.as_path()),
        font_path: None,
        fast_skip: false,
        novel_title: Some("My Novel"),
        novel_author: None,
        pacing_fixed_by_source: false,
    });
    assert!(
        summary.contains("EPUB output: /books/my-novel"),
        "expected the chapter directory as the destination, got:\n{}",
        summary
    );
    // The wizard never asks for an output root in this mode, so reporting one
    // would name a directory the user never chose and the run never touches.
    assert!(
        !summary.contains("Output root:"),
        "expected no output root line, got:\n{}",
        summary
    );
}

#[test]
fn expand_tilde_leaves_values_without_leading_tilde_unchanged() {
    assert_eq!(expand_tilde("/abs/path").as_ref(), "/abs/path");
    assert_eq!(expand_tilde("relative").as_ref(), "relative");
    assert_eq!(expand_tilde("").as_ref(), "");
}

#[test]
fn expand_tilde_resolves_bare_tilde_to_home() {
    // SAFETY: tests run sequentially within this binary; std::env::set_var is
    // fine here because no other test relies on $HOME concurrently.
    unsafe { std::env::set_var("HOME", "/Users/tester") };
    assert_eq!(expand_tilde("~").as_ref(), "/Users/tester");
}

#[test]
fn expand_tilde_resolves_tilde_slash_prefix() {
    unsafe { std::env::set_var("HOME", "/Users/tester") };
    assert_eq!(
        expand_tilde("~/Downloads").as_ref(),
        "/Users/tester/Downloads"
    );
    assert_eq!(
        expand_tilde("~/a/b/c.txt").as_ref(),
        "/Users/tester/a/b/c.txt"
    );
}

#[test]
fn expand_tilde_does_not_touch_tilde_user_form() {
    // We only resolve the current user's $HOME, not ~someone-else.
    unsafe { std::env::set_var("HOME", "/Users/tester") };
    assert_eq!(expand_tilde("~bob/foo").as_ref(), "~bob/foo");
}

#[test]
fn epub_destination_dir_is_the_chapter_dir_for_build_only() {
    assert_eq!(
        epub_destination_dir(
            CrawlMode::EpubOnly,
            std::path::Path::new("output"),
            Some(std::path::Path::new("/books/my-novel")),
            Some("My Novel"),
        ),
        Some(std::path::PathBuf::from("/books/my-novel"))
    );
}

#[test]
fn epub_destination_dir_infers_the_per_novel_dir_when_no_chapter_dir_is_given() {
    // Mirrors the non-interactive `--epub-only` path, which has no
    // `--chapter-dir` to work from.
    assert_eq!(
        epub_destination_dir(
            CrawlMode::EpubOnly,
            std::path::Path::new("output"),
            None,
            Some("My Novel"),
        ),
        Some(std::path::PathBuf::from("output/my_novel"))
    );
}

#[test]
fn epub_destination_dir_is_under_the_output_root_for_crawl_epub() {
    assert_eq!(
        epub_destination_dir(
            CrawlMode::CrawlEpub,
            std::path::Path::new("output"),
            None,
            Some("My Novel"),
        ),
        Some(std::path::PathBuf::from("output/my_novel"))
    );
}

#[test]
fn epub_destination_dir_ignores_a_chapter_dir_when_chapters_are_downloaded() {
    // Crawl-and-build writes chapters beneath the output root, so a chapter
    // directory the run never reads must not be reported as the destination.
    assert_eq!(
        epub_destination_dir(
            CrawlMode::CrawlEpub,
            std::path::Path::new("output"),
            Some(std::path::Path::new("/books/elsewhere")),
            Some("My Novel"),
        ),
        Some(std::path::PathBuf::from("output/my_novel"))
    );
}

#[test]
fn epub_destination_dir_is_none_when_no_epub_is_built() {
    assert_eq!(
        epub_destination_dir(
            CrawlMode::Crawl,
            std::path::Path::new("output"),
            None,
            Some("My Novel"),
        ),
        None
    );
}

#[test]
fn epub_destination_dir_is_none_when_the_title_is_unknown() {
    assert_eq!(
        epub_destination_dir(
            CrawlMode::CrawlEpub,
            std::path::Path::new("output"),
            None,
            None,
        ),
        None
    );
}

// ---- TextArea: the multi-line editor used by the description prompt ----

/// A layout roomy enough that wrapping never interferes with the tests that
/// care only about editing.
const WIDE: TextAreaLayout = TextAreaLayout { width: 80 };

/// A layout narrow enough to force wrapping.
fn narrow(width: usize) -> TextAreaLayout {
    TextAreaLayout { width }
}

/// Build a `KeyEvent` carrying the Ctrl modifier.
fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

#[test]
fn text_area_typing_inserts_at_the_cursor_and_advances_it() {
    let mut area = TextArea::new();
    area.set_value("ac");
    area.handle_key(key(KeyCode::Left), WIDE);
    area.handle_key(key(KeyCode::Char('b')), WIDE);
    assert_eq!(area.value(), "abc");
    assert_eq!(area.cursor(), 2);
}

#[test]
fn text_area_delete_backwards_removes_the_character_before_the_cursor() {
    let mut area = TextArea::new();
    area.set_value("abc");
    area.handle_key(key(KeyCode::Left), WIDE);
    area.handle_key(key(KeyCode::Backspace), WIDE);
    assert_eq!(area.value(), "ac");
    assert_eq!(area.cursor(), 1);
}

#[test]
fn text_area_delete_backwards_at_the_start_is_a_no_op() {
    let mut area = TextArea::new();
    area.set_value("abc");
    area.handle_key(key(KeyCode::Home), WIDE);
    area.handle_key(key(KeyCode::Backspace), WIDE);
    assert_eq!(area.value(), "abc");
    assert_eq!(area.cursor(), 0);
}

#[test]
fn text_area_delete_forwards_removes_the_character_at_the_cursor() {
    let mut area = TextArea::new();
    area.set_value("abc");
    area.handle_key(key(KeyCode::Left), WIDE);
    area.handle_key(key(KeyCode::Delete), WIDE);
    assert_eq!(area.value(), "ab");
    assert_eq!(area.cursor(), 2);
}

#[test]
fn text_area_delete_forwards_at_the_end_is_a_no_op() {
    let mut area = TextArea::new();
    area.set_value("abc");
    area.handle_key(key(KeyCode::Delete), WIDE);
    assert_eq!(area.value(), "abc");
    assert_eq!(area.cursor(), 3);
}

#[test]
fn text_area_home_and_end_reach_the_ends_of_the_whole_value() {
    // Whole-value, not row-relative: the value spans rows and has a break,
    // and both keys still land at the very ends.
    let layout = narrow(10);
    let mut area = TextArea::new();
    area.set_value("abcde\nxy");
    area.handle_key(key(KeyCode::Home), layout);
    assert_eq!(area.cursor(), 0);
    area.handle_key(key(KeyCode::End), layout);
    assert_eq!(area.cursor(), 8);
}

#[test]
fn text_area_cursor_movement_stops_at_the_boundaries() {
    let mut area = TextArea::new();
    area.set_value("ab");
    area.handle_key(key(KeyCode::Home), WIDE);
    area.handle_key(key(KeyCode::Left), WIDE);
    assert_eq!(area.cursor(), 0);
    assert_eq!(area.value(), "ab");
    area.handle_key(key(KeyCode::End), WIDE);
    area.handle_key(key(KeyCode::Right), WIDE);
    assert_eq!(area.cursor(), 2);
    assert_eq!(area.value(), "ab");
}

#[test]
fn text_area_submit_returns_the_value() {
    let mut area = TextArea::new();
    area.set_value("blurb");
    assert_eq!(
        area.handle_key(key(KeyCode::Enter), WIDE),
        TextAreaAction::Submit
    );
    assert_eq!(area.value(), "blurb");
}

#[test]
fn text_area_back_reports_a_back_navigation() {
    let mut area = TextArea::new();
    assert_eq!(
        area.handle_key(key(KeyCode::Esc), WIDE),
        TextAreaAction::Cancel
    );
}

#[test]
fn text_area_ctrl_c_quits() {
    let mut area = TextArea::new();
    assert_eq!(
        area.handle_key(ctrl(KeyCode::Char('c')), WIDE),
        TextAreaAction::Quit
    );
}

#[test]
fn text_area_submit_never_inserts_a_line_break() {
    let mut area = TextArea::new();
    area.set_value("ab");
    area.handle_key(key(KeyCode::Left), WIDE);
    area.handle_key(key(KeyCode::Enter), WIDE);
    assert_eq!(area.value(), "ab");
    assert!(!area.value().contains('\n'));
    assert_eq!(area.cursor(), 1);
}

#[test]
fn text_area_ctrl_j_inserts_a_line_break_without_submitting() {
    let mut area = TextArea::new();
    area.set_value("ab");
    let action = area.handle_key(ctrl(KeyCode::Char('j')), WIDE);
    assert_eq!(action, TextAreaAction::Continue);
    assert_eq!(area.value(), "ab\n");
    assert_eq!(area.cursor(), 3);
}

#[test]
fn text_area_alt_enter_is_ignored() {
    // Dropped on purpose: Ctrl+J is the one line-break key.
    let mut area = TextArea::new();
    area.set_value("ab");
    let action = area.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT), WIDE);
    assert_eq!(action, TextAreaAction::Continue);
    assert_eq!(area.value(), "ab");
}

#[test]
fn text_area_ignores_an_unbound_ctrl_chord_instead_of_inserting_its_letter() {
    // Ctrl+A arrives as `Char('a')` with Control, the same shape as Ctrl+J.
    let mut area = TextArea::new();
    area.set_value("ab");
    let action = area.handle_key(ctrl(KeyCode::Char('a')), WIDE);
    assert_eq!(action, TextAreaAction::Continue);
    assert_eq!(area.value(), "ab");
    assert_eq!(area.cursor(), 2);
}

/// `Bế` spelled the decomposed way: a bare `e` followed by the circumflex and
/// the acute as separate combining marks, which is how some sources ship it.
const DECOMPOSED: &str = "B\u{0065}\u{0302}\u{0301}";
/// The same text with the vowel precomposed into one `char`.
const COMPOSED: &str = "B\u{1EBF}";

#[test]
fn text_area_composes_a_decomposed_value_on_load() {
    let mut area = TextArea::new();
    area.set_value(DECOMPOSED);
    assert_eq!(area.value(), COMPOSED);
    assert_eq!(area.cursor(), 2);
}

#[test]
fn text_area_delete_backwards_removes_a_whole_decomposed_vowel() {
    let mut area = TextArea::new();
    area.set_value(DECOMPOSED);
    area.handle_key(key(KeyCode::Backspace), WIDE);
    assert_eq!(area.value(), "B");
    assert!(
        !area
            .value()
            .chars()
            .any(|c| c == '\u{0302}' || c == '\u{0301}'),
        "a combining mark was left stranded: {:?}",
        area.value()
    );
}

#[test]
fn text_area_composes_a_mark_typed_after_its_base_letter() {
    let mut area = TextArea::new();
    area.handle_key(key(KeyCode::Char('e')), WIDE);
    area.handle_key(key(KeyCode::Char('\u{0301}')), WIDE);
    assert_eq!(area.value(), "\u{00E9}");
    assert_eq!(area.cursor(), 1);
}

#[test]
fn text_area_moving_down_and_up_keeps_the_column() {
    let layout = narrow(5);
    let mut area = TextArea::new();
    area.set_value("abcdefghij");
    area.handle_key(key(KeyCode::Home), layout);
    area.handle_key(key(KeyCode::Right), layout);
    area.handle_key(key(KeyCode::Right), layout);
    assert_eq!(area.cursor(), 2);
    area.handle_key(key(KeyCode::Down), layout);
    assert_eq!(area.cursor(), 7);
    area.handle_key(key(KeyCode::Up), layout);
    assert_eq!(area.cursor(), 2);
}

#[test]
fn text_area_moving_onto_a_shorter_row_stops_at_its_end() {
    let layout = narrow(10);
    let mut area = TextArea::new();
    area.set_value("abcde\nxy");
    area.handle_key(key(KeyCode::Home), layout);
    for _ in 0..4 {
        area.handle_key(key(KeyCode::Right), layout);
    }
    area.handle_key(key(KeyCode::Down), layout);
    assert_eq!(area.cursor(), 8);
}

#[test]
fn text_area_moving_up_from_the_first_row_reaches_the_start() {
    let mut area = TextArea::new();
    area.set_value("abcde");
    area.handle_key(key(KeyCode::Left), WIDE);
    area.handle_key(key(KeyCode::Left), WIDE);
    assert_eq!(area.cursor(), 3);
    area.handle_key(key(KeyCode::Up), WIDE);
    assert_eq!(area.cursor(), 0);
    assert_eq!(area.value(), "abcde");
}

#[test]
fn text_area_moving_down_from_the_last_row_reaches_the_end() {
    let mut area = TextArea::new();
    area.set_value("abcde");
    area.handle_key(key(KeyCode::Home), WIDE);
    area.handle_key(key(KeyCode::Right), WIDE);
    assert_eq!(area.cursor(), 1);
    area.handle_key(key(KeyCode::Down), WIDE);
    assert_eq!(area.cursor(), 5);
    assert_eq!(area.value(), "abcde");
}

#[test]
fn text_area_clamping_holds_at_the_end_of_a_full_last_row() {
    // The value exactly fills its rows, so the end cursor is reported on a
    // phantom row below the last one. Down must stay put, Up must still reach
    // the start.
    let layout = narrow(3);
    let mut area = TextArea::new();
    area.set_value("abc");
    area.handle_key(key(KeyCode::Down), layout);
    assert_eq!(area.cursor(), 3);
    area.handle_key(key(KeyCode::Up), layout);
    assert_eq!(area.cursor(), 0);
}

#[test]
fn wrap_text_keeps_a_short_value_on_one_line() {
    assert_eq!(wrap_text("abc", 10), vec!["abc".to_string()]);
}

#[test]
fn wrap_text_gives_an_empty_value_one_empty_line() {
    assert_eq!(wrap_text("", 10), vec![String::new()]);
}

#[test]
fn wrap_text_breaks_at_a_space_and_keeps_every_character() {
    let value = "hello world again";
    let lines = wrap_text(value, 11);
    assert_eq!(lines.concat(), value, "wrapping dropped characters");
    assert!(lines.iter().all(|l| l.chars().count() <= 11));
    assert!(lines.len() > 1);
    assert!(lines[0].ends_with(' '), "line broke mid-word: {lines:?}");
}

#[test]
fn wrap_text_hard_breaks_a_word_longer_than_the_width() {
    let value = "abcdefgh";
    let lines = wrap_text(value, 3);
    assert_eq!(lines, vec!["abc", "def", "gh"]);
    assert_eq!(lines.concat(), value);
}

#[test]
fn wrap_text_starts_a_new_row_at_a_line_break() {
    let value = "ab\ncd";
    let lines = wrap_text(value, 10);
    assert_eq!(lines, vec!["ab\n", "cd"]);
    assert_eq!(lines.concat(), value);
}

#[test]
fn wrap_text_gives_consecutive_breaks_an_empty_row() {
    let value = "a\n\nb";
    let lines = wrap_text(value, 10);
    assert_eq!(lines, vec!["a\n", "\n", "b"]);
    assert_eq!(lines.concat(), value);
}

#[test]
fn wrap_text_leaves_a_row_after_a_trailing_break() {
    // Without the trailing row the cursor at the end of the value would have
    // nowhere to sit and would be drawn back on the line above the break.
    let value = "ab\n";
    let lines = wrap_text(value, 10);
    assert_eq!(lines, vec!["ab\n", ""]);
    assert_eq!(lines.concat(), value);
}

#[test]
fn wrap_text_wraps_each_segment_between_breaks() {
    let value = "hello world\nx";
    let lines = wrap_text(value, 6);
    assert_eq!(lines.concat(), value);
    assert!(
        lines
            .iter()
            .all(|l| l.trim_end_matches('\n').chars().count() <= 6)
    );
    assert_eq!(lines.last().unwrap(), "x");
}

#[test]
fn wrapped_cursor_position_maps_an_index_on_the_first_line() {
    let lines = wrap_text("abcdefgh", 3);
    assert_eq!(wrapped_cursor_position(&lines, 1, 3), (0, 1));
}

#[test]
fn wrapped_cursor_position_maps_an_index_on_a_later_line() {
    let lines = wrap_text("abcdefgh", 3);
    assert_eq!(wrapped_cursor_position(&lines, 4, 3), (1, 1));
}

#[test]
fn wrapped_cursor_position_moves_past_the_end_of_a_full_line() {
    // The value exactly fills its lines, so an end-of-value cursor has no
    // column left on the last one and belongs at the start of a fresh row.
    let lines = wrap_text("abcdef", 3);
    assert_eq!(lines.len(), 2);
    assert_eq!(wrapped_cursor_position(&lines, 6, 3), (2, 0));
}

#[test]
fn wrapped_cursor_position_sits_after_the_last_character_of_a_short_line() {
    let lines = wrap_text("abcde", 3);
    assert_eq!(wrapped_cursor_position(&lines, 5, 3), (1, 2));
}

#[test]
fn wrapped_cursor_position_lands_at_the_start_of_the_row_after_a_break() {
    let lines = wrap_text("ab\ncd", 10);
    assert_eq!(wrapped_cursor_position(&lines, 3, 10), (1, 0));
}

#[test]
fn wrapped_cursor_position_sits_before_a_break_at_the_end_of_its_row() {
    let lines = wrap_text("ab\ncd", 10);
    assert_eq!(wrapped_cursor_position(&lines, 2, 10), (0, 2));
}

#[test]
fn wrapped_cursor_position_lands_on_the_row_a_trailing_break_opened() {
    let lines = wrap_text("ab\n", 10);
    assert_eq!(wrapped_cursor_position(&lines, 3, 10), (1, 0));
}
