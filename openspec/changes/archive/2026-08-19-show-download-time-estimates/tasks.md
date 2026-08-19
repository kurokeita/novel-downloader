## 1. Elapsed time and the freeze marker

- [x] 1.1 Add failing tests in `tests/ui.rs` for elapsed time: reported before any
  chapter event, advances by exactly the interval between two reads, frozen at the
  instant the run was marked finished, and frozen on the abort path where the run
  is marked finished before all chapters are accounted for.
- [x] 1.2 Add `started_at: Instant` and `finished_at: Option<Instant>` to
  `DownloadProgress`, set `started_at` in `with_log_capacity`, and implement
  `elapsed(now)` returning `finished_at.unwrap_or(now) - started_at`.
- [x] 1.3 Add `finish_at(now)` and make the existing `finish()` delegate to it with
  `Instant::now()`, so the abort and completion paths in
  `run_download_screen` freeze without changing their call sites.

## 2. Duration rendering

- [x] 2.1 Add failing tests in `tests/ui.rs` for `HH:MM:SS` rendering: four minutes
  twelve seconds renders `00:04:12`, one hour nine minutes six seconds renders
  `01:09:06`, and zero renders `00:00:00`.
- [x] 2.2 Implement the unconditional `HH:MM:SS` formatter in
  `src/ui/widgets/progress.rs` and export it from `ui::widgets`.

## 3. The completion window

- [x] 3.1 Add failing tests in `tests/ui.rs` for window recording: a terminal event
  (completed or failed) records an arrival instant, a `ConcurrencyClamped` note
  records none, and the window never exceeds its cap of 20 while `advanced()`
  keeps counting past it.
- [x] 3.2 Add `completions: VecDeque<Instant>` with a `MAX_RATE_SAMPLES` cap of 20,
  and `record_completed_at(number, status, now)` /
  `record_failed_at(number, message, now)` that push-back and pop-front.
- [x] 3.3 Make `record_completed` / `record_failed` and `from_event` delegate to the
  `_at` variants with `Instant::now()` taken inside the call, so the timestamp is
  stamped while `make_tui_progress_callback` holds the mutex and arrival order
  stays monotonic under parallel workers.

## 4. The remaining-time estimate

- [x] 4.1 Add failing tests in `tests/ui.rs` for every case where the estimate is
  withheld: fewer than two samples, a total chapter count of zero, a finished run,
  and a burst of samples spanning under the two-second minimum.
- [x] 4.2 Add failing tests in `tests/ui.rs` for the cases where an estimate is
  produced: steady throughput yields remaining-count-divided-by-rate, a
  near-instant burst followed by slow steady completions converges on the slow
  rate once the burst samples are evicted, and a second read with no events in
  between reports a larger estimate than the first.
- [x] 4.3 Implement `eta(now) -> Option<Duration>` with a `MIN_RATE_SPAN` of two
  seconds, computing `span = max(newest - oldest, now - oldest)` and
  `rate = (len - 1) / span`, returning `None` for each withheld case including a
  degenerate zero span.

## 5. Download screen label

- [x] 5.1 Add failing tests in `tests/ui.rs` for the gauge label in its three
  states: in flight with an estimate, in flight with the estimate withheld
  (placeholder present, so width is stable), and finished (frozen elapsed, no
  estimate and no placeholder).
- [x] 5.2 Implement a pure `gauge_label(&DownloadProgress, now) -> String` in
  `src/ui/widgets/progress.rs` composing the existing tally and percentage with
  the elapsed time and the estimate.
- [x] 5.3 Replace the inline label construction in `draw_download`
  (`src/ui/screens/download.rs`) with a `gauge_label` call, passing
  `Instant::now()` from the render loop and leaving the `Layout` constraints
  untouched.

## 6. Documentation

- [x] 6.1 Update the `ui/` and `Useful project context` sections of `AGENTS.md` to
  record that `DownloadProgress` now owns run timing, the 20-sample window and
  two-second minimum span, and the reason the estimate is withheld rather than
  guessed.
- [x] 6.2 Confirm every function added in tasks 1-5 carries a `///` doc comment,
  including the two tuning constants.

## 7. Verification

- [x] 7.1 `cargo test` green.
- [x] 7.2 `cargo clippy --all-targets` reports 0 warnings.
- [x] 7.3 `cargo fmt --check` clean.
- [x] 7.4 Eyes-on-terminal check against a local mock site: run the TUI through a
  range large enough to pass the two-second minimum span, confirm the placeholder
  gives way to a real estimate, that the label fits an 80-column terminal, and
  that elapsed time freezes on both the completion and `Esc` abort paths.
