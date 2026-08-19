## Why

Every interactive run that embeds a custom EPUB font makes the user retype or
tab-complete the full font path at the `FontChoice` / `FontPath` steps, even
when they use the same two or three fonts forever. Nothing in the crate
persists anything between runs, so the wizard starts from zero each time.

A bad font path is also only discovered at the very end: the wizard accepts any
string, the whole crawl runs, and `utils::find_font_file` fails at EPUB build
time (exit code 3). Remembering fonts requires validating them anyway, so the
same primitive fixes the late failure.

## What Changes

- **New `recent_fonts` module** (library API) — load, validate, prune, and
  record a recency-ordered list of custom font files in
  `~/.config/novel-downloader/recent-fonts.json` (`$XDG_CONFIG_HOME` wins when
  set). Pure functions take the config directory so tests can point at a
  `tempfile::tempdir()`; one thin resolver reads the environment.
- **Cached font metadata** so subsequent TUI launches stay fast: each entry
  stores the canonical path, the family name and extension already parsed by
  `font::extract_font_metadata`, and the file size. Rendering the list needs no
  font parsing at all — only one `stat` per entry.
- **TUI `FontChoice` step lists remembered fonts** between the bundled font and
  the "pick a custom path" option. `FontChoice` grows a `Remembered(PathBuf)`
  variant; picking one skips the `FontPath` step entirely.
- **Invalid entries are pruned silently on load.** An entry whose file cannot
  be stat'ed for any reason — deleted, permission denied, unmounted volume — is
  dropped and the file rewritten. No note, no prompt.
- **Stale cached metadata is refreshed.** When the stat'ed size differs from the
  recorded size, that one entry is re-parsed and its family name refreshed in
  place, keeping its position.
- **Recording happens once, on wizard confirm.** `run_interactive_flow` records
  the plan's `font_path` after `StepResult::Done`. Reuse moves the entry to the
  front; a new font is pushed to the front and the list truncated to 10, so the
  oldest entry falls off silently. The bundled `Bokerlam.ttf` is never recorded
  and never listed.
- **`FontPath` step validates on submit.** A path that is not a readable font is
  rejected with a note and the step re-runs, instead of failing after the crawl.
- No new dependencies. `serde`, `serde_json`, `futures`, and `tokio::fs` are
  already in the tree.
- No breaking changes to the CLI surface. `--font-path` keeps its behavior and
  does not itself write to the store; only a confirmed wizard plan records.

## Capabilities

### New Capabilities

- `recent-fonts`: persisting, validating, pruning and ordering the list of
  custom EPUB fonts a user has previously confirmed in the interactive wizard,
  plus how that list is offered and selected at the font step.

### Modified Capabilities
<!-- None. openspec/specs/ has no committed specs yet, so there is no existing
     capability whose requirements change. -->

## Impact

- **New file:** `src/recent_fonts.rs`, exported from `src/lib.rs`.
- **New test file:** `tests/recent_fonts.rs`.
- **`src/ui/wizard/state.rs`** — `FontChoice` gains `Remembered(PathBuf)`;
  `WizardState` caches the validated list so back-navigation does not re-stat.
- **`src/ui/wizard/steps.rs`** — `step_font_choice` becomes async and builds its
  options from the validated list; `step_font_path` validates on submit;
  `step_confirm`'s back-edge handles the third `FontChoice` arm.
- **`src/ui/wizard/mod.rs`** — records the font after `StepResult::Done`.
- **`src/utils.rs`** — the font-validation primitive used by both the store and
  the `FontPath` step lands next to `find_font_file`.
- **Filesystem:** first persisted state this crate has ever written. When `HOME`
  and `$XDG_CONFIG_HOME` are both unset, persistence is disabled and the wizard
  behaves exactly as it does today.
- **Docs:** `AGENTS.md` module list and useful-context section need the new
  module and the store location.
