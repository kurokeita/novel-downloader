## 1. Font validation primitive

- [x] 1.1 Add failing tests in `tests/utils.rs` for a `utils` function that
      answers "can this path be read as a font", covering a real TTF fixture, a
      missing path, and a non-font file
- [x] 1.2 Implement that function next to `find_font_file` in `src/utils.rs`,
      returning the canonical path plus the extracted `FontMetadata` on success
      and an error otherwise

## 2. Store scaffolding

- [x] 2.1 Add failing tests in a new `tests/recent_fonts.rs` for load/record
      round-tripping through a `tempfile::tempdir()`: path, family name,
      extension, and size survive, and the directory plus `recent-fonts.json`
      are created on first record
- [x] 2.2 Create `src/recent_fonts.rs` with the `RecentFont` entry, the serde
      container carrying `#[serde(default)]`, `load(config_dir)`,
      `record(config_dir, path)`, and export the module from `src/lib.rs`
- [x] 2.3 Add failing tests for the tolerant-read behavior: a missing file, an
      empty file, and malformed JSON each load as an empty list without an
      error, and a later record overwrites the malformed file with a well-formed
      store
- [x] 2.4 Implement the tolerant read so any parse or read failure yields an
      empty list

## 3. Ordering and capacity

- [x] 3.1 Add failing tests for ordering: recording an already-remembered font
      moves it to the front without changing the length; recording a new font at
      capacity puts it first, drops the tail, and keeps the list at 10; two
      equivalent paths for the same file collapse to one entry
- [x] 3.2 Implement move-to-front, front-insert, truncate-to-10, and
      canonical-path dedupe in `src/recent_fonts.rs`

## 4. Validation and pruning on load

- [x] 4.1 Add failing tests for pruning: a deleted middle entry disappears while
      the survivors keep their order; a load that pruned rewrites the store; a
      load that pruned nothing leaves the file untouched
- [x] 4.2 Implement the validation pass in `load`, stat'ing all entries
      concurrently with `futures::join_all` and dropping every entry whose
      metadata lookup fails for any reason
- [x] 4.3 Add failing tests for stale-metadata refresh: an unchanged font is
      served from the cached family name; a font replaced by a different font of
      a different size reports the new family name, records the new size, and
      keeps its position
- [x] 4.4 Implement the size-mismatch refresh, re-parsing only the mismatched
      entry

## 5. Recording rules

- [x] 5.1 Add failing tests for the record guards: a path that cannot be read as
      a font leaves the store unchanged, and the bundled font's own path is never
      stored
- [x] 5.2 Implement both guards in `record`, comparing the canonical path against
      the canonical path returned by `find_font_file(None)` for the bundled-font
      check
- [x] 5.3 Add failing tests for the environment resolver: `$XDG_CONFIG_HOME`
      wins when set and non-empty, `$HOME/.config` is the fallback, and neither
      set yields `None`
- [x] 5.4 Implement the `config_dir()` resolver, keeping it the only part of the
      module that reads the environment

## 6. Wizard integration

- [x] 6.1 Extend `FontChoice` in `src/ui/wizard/state.rs` with a
      `Remembered(PathBuf)` variant and add the cached
      `Option<Vec<RecentFont>>` field to `WizardState`
- [x] 6.2 Make `step_font_choice` async in `src/ui/wizard/steps.rs`, populate the
      cache on first entry, and build the option list as bundled → remembered →
      custom path, each remembered option labeled with its family name and
      hinted with its path
- [x] 6.3 Route a `Remembered` selection straight to `WizardStep::Confirm` with
      `state.font_path` set, and add the third arm to `step_confirm`'s `previous`
      computation so back-navigation from Confirm returns to `FontChoice`
- [x] 6.4 Make `step_font_path` validate the submitted path with the task-1
      primitive, showing a note and re-entering `WizardStep::FontPath` on failure
- [x] 6.5 Record the confirmed font in `run_interactive_flow` after
      `StepResult::Done`, reading `plan.font_path` and skipping when it is `None`
      or when `config_dir()` is `None`

## 7. Documentation

- [x] 7.1 Add `recent_fonts` to the `AGENTS.md` module list and record the store
      location, cap, and prune-on-load behavior in its useful-context section

## 8. Verification

- [x] 8.1 `cargo test` green
- [x] 8.2 `cargo clippy --all-targets` with 0 warnings
- [x] 8.3 `cargo fmt --check` clean
- [x] 8.4 Eyes-on-terminal check of the font step: empty list, a populated list,
      selecting a remembered font, and a rejected bad path
