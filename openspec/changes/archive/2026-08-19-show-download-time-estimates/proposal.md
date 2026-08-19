## Why

The non-interactive CLI's `indicatif` bar already reports both elapsed time and
an ETA (`{elapsed_precise}` / `{eta_precise}` in `build_progress_bar`). The
interactive TUI download screen reports only a chapter counter and a percentage,
so a user who picked the wizard has no way to tell whether a 1900-chapter run
has ten minutes left or two hours. Closing that gap is a parity fix between the
two surfaces, not a new capability of the crawler.

A naive `elapsed / advanced` estimate is not good enough here. Two of the most
common run shapes advance the counter in an instant burst that carries no
information about the remaining work:

- **Resume with `--fast-skip`**, chapters already on disk complete with no
  network request at all, so 900 of 1000 chapters can land in milliseconds.
- **Parallel fresh run**, the first `--workers` completions all land together
  after one chapter latency.

Both make a naive estimate print `ETA 00:00:00` precisely when the real work is
about to begin, which is worse than printing nothing.

## What Changes

- `DownloadProgress` starts tracking time: when the run began, when it ended,
  and the arrival instants of recent terminal (completed or failed) events.
- Elapsed time is exposed as a reader and **freezes** once the run finishes, so
  the terminal "Done" screen shows the run's real duration rather than a clock
  that keeps counting while the user reads it. Aborting with `Esc` freezes it
  too, since the abort path already marks the state finished.
- A remaining-time estimate is derived from a **sliding window of the most
  recent completion instants** rather than a whole-run average, so a skip burst
  or a worker ramp is flushed out of the estimate instead of poisoning it for
  the rest of the run.
- The estimate is **withheld rather than guessed** when the available samples
  cannot support it, too few samples, or samples spanning too short a wall-clock
  interval. The screen shows a placeholder in that case.
- The TUI download gauge label grows two segments: elapsed time always, and the
  estimate while the run is in flight. No new rows, so the screen layout is
  unchanged.
- `HH:MM:SS` is the rendering format for both durations, matching the CLI bar.

Not breaking for any caller that constructs `DownloadProgress` through `new` or
`with_log_capacity`, which is every current call site. New public fields do mean
struct-literal construction from outside the crate would no longer compile; the
type is documented as callback-owned state, so this is noted rather than worked
around. No CLI flag, wizard step, runner, `ProgressEvent`, or EPUB output
changes.

## Capabilities

### New Capabilities

- `download-progress`: How the interactive download screen reports run
  progress, the chapter tally, elapsed time, and the remaining-time estimate,
  including when that estimate is withheld because the samples cannot support
  it.

### Modified Capabilities

None. `recent-fonts` is untouched.

## Impact

- `src/ui/widgets/progress.rs`, `DownloadProgress` gains time state, two
  readers, and a duration formatter. Timestamps are taken inside the existing
  event-application path, which the progress callback already runs with the
  shared mutex held, so arrival order stays monotonic under parallel workers
  with no sorting.
- `src/ui/screens/download.rs`, gauge label composition only.
- `tests/ui.rs`, new `download_progress_*` cases, including the skip-burst
  recovery case that motivates the sliding window.
- No new dependencies; `std::time::Instant` and `VecDeque` cover it.
- `src/bin/novel-downloader.rs` and the `indicatif` path are untouched, so the
  non-interactive surface keeps its current behavior.
