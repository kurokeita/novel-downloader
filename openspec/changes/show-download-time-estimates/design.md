## Context

See `proposal.md` — Why for the motivation, and
`specs/download-progress/spec.md` for the behavior contract. This document
records the estimator's tuning decisions, which the spec deliberately leaves
out so that tuning can change without a spec change.

Two facts about the existing code shape everything below.

`DownloadProgress` (`src/ui/widgets/progress.rs`) is a plain state bag with
public fields, constructed only through `new` / `with_log_capacity` at
`src/bin/novel-downloader.rs:266` and in `tests/ui.rs`. It is mutated by exactly
one writer: the closure built by `make_tui_progress_callback`, which locks the
shared mutex and then calls `from_event` with the guard held. Timestamps taken
inside `from_event` are therefore already serialized, and arrival order is
monotonic even with eight workers — no sorting, no per-worker bookkeeping.

`run_download_screen` (`src/ui/screens/download.rs`) clones the whole state under
the lock once per ~80ms redraw tick and renders from the clone. Anything added to
the struct is cloned at that rate, which bounds how much state is reasonable to
add.

`src/ui/screens/loading.rs:32` already keeps a screen-local
`started_at: Instant`, so a clock inside the `ui` tree is established practice.

## Goals / Non-Goals

**Goals:**

- Keep the estimator a pure function of recorded instants, so every scenario in
  the spec is testable through the public API without a real terminal, a real
  clock, or a sleeping test.
- Degrade to "no estimate" rather than to a wrong estimate.
- Add no dependency and no new module.

**Non-Goals:**

- Estimating per-source throughput, or carrying learned rates between runs.
- Changing the `indicatif` path in `src/bin/novel-downloader.rs`. It already
  reports elapsed time and an ETA; parity means the TUI catches up, not that the
  two surfaces share an implementation.
- Distinguishing written from skipped chapters in the estimator. The sliding
  window makes that classification unnecessary, which is the point of choosing
  it — see Decisions.

## Decisions

### Ownership: `DownloadProgress` holds the clock, not the screen

The `ui::widgets` module owns the time state and the estimator;
`ui::screens::download` only composes a label from it. The alternative — a
screen-local `Instant` as in `loading.rs` — needs no new struct fields, but puts
the estimator inside the render loop, which the project does not unit-test. The
spec's fifteen scenarios only become testable if the logic sits in the widget.

Added state:

```
started_at:   Instant             set in new(); public like every other field
completions:  VecDeque<Instant>   arrival instants of terminal events, cap 20
finished_at:  Option<Instant>     freeze marker
```

`VecDeque` because the window is a strict push-back / pop-front queue. `front()`
is the oldest sample and `back()` the newest, which holds only because arrivals
are serialized under the mutex, as described in Context. Twenty `Instant`s cloned
per redraw tick is negligible next to the existing 500-entry log window.

### Clock injection: `*_at(now)` twins, tests offset from `started_at`

`from_event` and `finish` stamp `Instant::now()` internally and delegate to
`record_completed_at(number, status, now)` / `finish_at(now)`. Readers take the
instant as an argument: `elapsed(now)`, `eta(now)`. Tests drive only the `_at`
methods and the readers, so no test calls `Instant::now()` for anything but a
base offset.

This mirrors `recent_fonts::load(config_dir)`, where the environment-dependent
input is a parameter and only one thin wrapper reads the real environment.

No third constructor is added. `Instant` has no arbitrary constructor in `std`,
so tests take `progress.started_at` as their epoch and add `Duration`s to it.
The alternative — a `with_started_at` constructor, or threading a clock closure
through `make_tui_progress_callback` — buys nothing, since `started_at` is
already a public field on a struct whose fields are all public.

The cost is two entry points per mutation, and the discipline that
`Instant::now()` appears in exactly two places. Accepted as the smaller wart.

### Estimator: sliding window over recent arrivals, not a lifetime average

```
span = max(newest - oldest, now - oldest)
rate = (len - 1) / span
eta  = (total - advanced) / rate
```

Rejected: `elapsed / advanced * remaining`. It is three lines instead of about
fifteen, but it is wrong in the common case rather than the rare one. Resuming a
1000-chapter novel with 900 chapters already on disk and `--fast-skip` set makes
it report `ETA 00:00:00` for the entire real download — off by roughly 450x
against a measured khodocsach rate of ~0.8 chapters/s (1.62 req/s at two requests
per chapter, per `AGENTS.md`). Resuming is the normal way this tool is used.

The `max` in `span` is the stall stretch. Without it the window freezes during a
rate-limit backoff and keeps reporting the pre-stall figure; khodocsach's penalty
is cleared only by roughly three minutes of silence, so this is a real run shape,
not a hypothetical. With it, the estimate grows honestly while nothing moves.

### Window cap 20, count-based

At khodocsach's ~0.8 chapters/s a 20-sample window holds ~25s of history; on an
unconstrained parallel metruyenhot run it holds a few seconds and reads noisier.

Rejected: a time-based window (keep samples newer than ~30s), which would
self-size across both sources. It needs age-pruning on every push plus a `now`
argument on the writer, and the minimum-span guard below already removes the
pathologies a bigger window would have been protecting against. Count-based is
the smaller change; revisit if a real run reads wrong.

### Minimum span of 2s — the guard that does the real work

Below two seconds of sample span, no estimate is reported.

This is not polish. Both of the burst shapes in the proposal fill a 20-sample
window in milliseconds, so a sample-count threshold alone does not catch them:

```
900 skips land in 30ms   → 20 stamps spanning 30ms → rate ~660/s  → ETA 0s
8 workers, first wave    → 8 stamps spanning 4ms   → rate ~1750/s → ETA 0s
```

A confidently wrong `ETA 00:00:00` reads as "about to finish" and is worse than
a placeholder, which reads as "not known yet" and is true. One comparison against
a constant buys both.

Recovery after a skip burst is not instant, and the spec does not promise that it
is. The burst's stamps evict one per subsequent completion, so the estimate is
optimistic for up to 20 real chapters (~25s on khodocsach) before it converges.
Optimistic-and-shrinking beats zero-and-wrong.

### `HH:MM:SS` unconditionally

Chosen because it is *less* code, not only for parity with `elapsed_precise` on
the CLI bar. A 1900-chapter khodocsach run is roughly 40 minutes and plenty of
runs cross an hour, so an `MM:SS` form would need an hour-fallback branch anyway.
The unconditional form has no branch and matches the other surface for free.

### Label composition, no layout change

The two durations go into the existing `Gauge` label, so the `Layout`
constraints in `draw_download` are untouched:

```
running, estimating    312 / 1000 (31%)  ⏱ 00:04:12  ETA 00:09:06
running, withheld      312 / 1000 (31%)  ⏱ 00:04:12  ETA —
finished              1000 / 1000 (100%)  ⏱ 00:12:34
```

Rejected: a third line in the status block, which means growing its
`Constraint::Length(4)` to `5` and pushing every chunk below it. The label
already carries the tally and percentage, so the durations belong beside them.

The placeholder rather than an omitted segment keeps the label width stable
between redraws while a run is in flight; the finished state drops the segment
entirely because nothing redraws after it.

## Risks / Trade-offs

- **Estimate stays optimistic for up to 20 chapters after a skip burst** → The
  minimum-span guard suppresses the worst of it (`ETA 0s` never shows), and the
  window self-heals. Not mitigated further; a shorter window would trade this for
  a noisier steady-state reading.
- **Estimate turns pessimistic for up to 20 chapters after a rate-limit stall**,
  because the stretched span stays in the window → Left alone. Pessimistic is the
  safe direction of error. Optional future fix, deliberately skipped now:
  age-prune samples older than ~60s, at the cost of the estimate blanking briefly
  after every stall.
- **Gauge label is ~55 characters and ratatui centers then clips it** → Fits an
  80-column terminal after the screen's margins and block borders; a 40-column
  terminal loses the tails. Accepted, consistent with the rest of the TUI.
- **Adding public fields breaks struct-literal construction from outside the
  crate** → No such call site exists; every caller uses `new` or
  `with_log_capacity`. Called out in `proposal.md` rather than worked around,
  per the project's no-compat-shims rule.
- **`Instant::now()` in two places invites a third** → The `_at` methods are the
  only ones tests touch, so a stray `Instant::now()` in the estimator would make
  a scenario untestable and show up as friction immediately.

## Migration Plan

None. Additive to one widget and one label; no persisted state, no wire format,
no flags. Reverting is deleting the fields and the label segments.
