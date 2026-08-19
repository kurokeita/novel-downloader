## Context

See `proposal.md` — Why. Constraints that shape the approach:

- **The crate persists nothing today.** No config file, no state directory, no
  `dirs` dependency. This change introduces the first one, so the resolver and
  the on-disk format are both new ground rather than extensions of a pattern.
- **`font::extract_font_metadata` is best-effort by design** (`src/font.rs:121`).
  It returns `Err` only when the file cannot be read or is under 12 bytes; a
  malformed `name` table falls back to the file stem. So "is this still a valid
  font?" is answerable only as "can the file be read?" — do not expect it to
  reject a renamed archive.
- **The wizard step machine is already async.** `advance_step`
  (`src/ui/wizard/mod.rs:31`) awaits, and `step_discover` / `step_title` are
  already `async fn`. Making `step_font_choice` and `step_font_path` async costs
  nothing structurally.
- **`run_select` scrolls.** It renders through `render_stateful_widget` with a
  `ListState` (`src/ui/screens/prompts.rs:313`), so ratatui keeps the cursor in
  view and a longer option list needs no widget work. There is no scrollbar or
  "N more" affordance, which is one reason the list is capped.
- **TUI run loops are not unit-tested by convention** — only the underlying
  state machines. Any logic that needs test coverage must sit outside the
  screens.

## Goals / Non-Goals

**Goals:**

- A wizard launch costs one filesystem metadata lookup per remembered entry and
  zero font parses, so the font step feels instant.
- All ordering, pruning, eviction, and serialization logic is reachable from
  `tests/recent_fonts.rs` without a terminal.
- A missing, malformed, or unwritable store degrades to today's behavior instead
  of failing a run.

**Non-Goals:**

- A general application config file. This change stores exactly one thing.
- Recording fonts from non-interactive CLI runs. The store is a wizard
  convenience; `--font-path` alone does not write to it.
- Detecting that a remembered font was *moved* rather than deleted. A moved font
  is simply forgotten and re-added the next time it is picked.
- Locking or merging concurrent writes. Two wizards confirming at once is
  last-writer-wins.

## Decisions

### `src/recent_fonts.rs` owns the store; `src/utils.rs` owns the validation primitive

The store is not a UI concern — it is application state that the UI happens to
be the only current writer of — so it lives at the crate root next to `font.rs`
rather than under `ui/`. The one piece it shares with the wizard is "can this
path be read as a font", which belongs beside `find_font_file` in `utils.rs`
because `find_font_file` is already the function that answers the neighboring
question and already canonicalizes paths.

*Alternative considered:* putting everything in `ui/wizard/`. Rejected — it would
put file-format and filesystem logic inside the one module tree the project
deliberately does not unit-test, and it would have to move the day the CLI wants
to record too.

### Pure functions take a directory; one thin resolver reads the environment

`load(config_dir)` and `record(config_dir, path)` take the directory explicitly;
a separate `config_dir()` resolves `$XDG_CONFIG_HOME` then `$HOME/.config` and
returns `Option<PathBuf>`. Tests drive the pure functions against
`tempfile::tempdir()` and never touch environment variables, which matters
because `std::env::set_var` is process-global and racy under the default
multi-threaded test harness.

`None` from the resolver means persistence is off, which is also the honest
answer on a machine with no `HOME`.

*Alternative considered:* a `NOVEL_DOWNLOADER_CONFIG_DIR` override read inside
the load/record functions. Rejected — it adds public environment surface purely
for tests, and the directory parameter already covers it.

### Validate with `stat` only, refresh on size mismatch

Each entry caches `family_name`, `extension`, and `size`. Validation is one
metadata lookup: present → keep, any error → prune. When the reported size
differs from the cached size, that single entry is re-parsed through
`extract_font_metadata` and refreshed in place. This keeps the common path at
zero reads while still catching an in-place font replacement.

*Alternatives considered:* re-parsing every entry on every load — correct but it
reads megabytes per launch, defeating the point. Recording `mtime` alongside
size — the only extra case it catches is a replacement of byte-identical length,
`Metadata::modified()` is fallible on network and FUSE mounts, and the penalty
for missing that case is a stale display label, never a wrong font embedded,
since the path is what gets read at build time.

### Stat all entries concurrently with `futures::join_all`, no timeout

A path on a stale SMB/NFS/sshfs mount does not fail fast: the kernel retries the
mount before returning an error, so a single lookup can block for tens of
seconds. Serially, ten such entries stack ten hangs; concurrently the whole pass
costs the slowest single lookup. `futures` is already a dependency.

No timeout and no loading screen: the failure mode is a pause on a rare setup,
not corruption, and wrapping the pass in `run_loading_screen` would flash a
spinner on every launch to insure against a case most users never hit. If it
turns out to bite, the loading screen is a drop-in — the pass is already a
single future.

### Cap at 10, evict from the tail

Position carries recency, so no timestamp field is needed. Recording an existing
entry removes it from its position and pushes it to the front, leaving the length
unchanged; recording a new entry pushes to the front and truncates to 10. The cap
exists because the option list has no "N more" affordance and because an
uncapped list makes the stat pass unbounded.

### Remembered fonts flatten into the existing `FontChoice` select

`FontChoice` becomes `Default | Remembered(PathBuf) | Custom`, and the select is
built as bundled → remembered → custom-path. `Select::with_initial` needs
`PartialEq`, which the new variant satisfies by comparing paths. Choosing
`Remembered` sets `state.font_path` and jumps to `Confirm`; `step_confirm`'s
`previous` computation gains a third arm that returns to `FontChoice` rather than
`FontPath`.

*Alternative considered:* a separate `FontRecent` wizard step. Rejected — one
more step, two more back-navigation edges, and a screen that has nothing to show
when the list is empty.

### The validated list is computed once and cached in `WizardState`

`WizardState` holds `Option<Vec<RecentFont>>`, populated the first time the font
step runs. Esc from `FontPath` back to `FontChoice` re-renders from the cache, so
a wizard run performs exactly one stat pass and one possible rewrite.

### Recording happens in `run_interactive_flow`, not in `step_confirm`

`step_confirm` is synchronous and returns `StepResult::Done`. Rather than making
it async purely to write a file, `run_interactive_flow` records after it receives
`Done`, reading `plan.font_path`. One call site, and it is the only place that
knows a plan was actually confirmed rather than merely assembled.

### The bundled font is excluded by canonical-path comparison

`find_font_file(None)` already resolves the bundled `Bokerlam.ttf` through the
exe directory, its parent, and the cwd. Comparing canonical paths against that
result is exact, where a `file_name == "Bokerlam.ttf"` test would also hide a
user's own unrelated file of that name.

## Risks / Trade-offs

- **An ejected external volume permanently forgets its fonts** → Accepted
  deliberately: unmounted and deleted are indistinguishable on macOS, both
  surface as a plain not-found. The entry returns the next time the font is
  picked.
- **A stale network mount pauses the font step with no spinner and no Esc** →
  `join_all` bounds the pause to the single slowest lookup instead of the sum;
  the loading screen stays available as a follow-up if it proves real.
- **A same-size in-place font replacement keeps a stale family name** → Only the
  label is wrong; the path still governs what is embedded. Re-parsing every load
  would cost megabytes per launch.
- **Two concurrent wizard confirmations lose one record** → Last writer wins. A
  lock file for a ten-entry convenience list is not worth the failure modes it
  introduces.
- **First-ever persisted file, so a future schema change has no version field to
  branch on** → `#[serde(default)]` on the container and on optional fields plus
  "unparseable means empty" covers additive growth; a genuinely breaking format
  change can rename the file.
- **Validating at the `FontPath` prompt reads the whole font to confirm it** →
  One read of a file the user just chose, at a point where they are already
  waiting on their own keystroke.

## Migration Plan

No migration. The store does not exist before this change, and its absence is
already the "empty list" path, so the first run after upgrading behaves exactly
like the last run before it. Rollback is deleting
`~/.config/novel-downloader/recent-fonts.json`; a downgraded binary ignores the
file entirely.
