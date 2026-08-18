# AGENTS.md

Master instructions for AI coding agents working in this repository. This is
the single source of truth; platform-specific master files (`CLAUDE.md`)
import it.

## What this project is

A Rust crawler + EPUB builder for Vietnamese novel sites
(`metruyenhotvn.com`, `metruyenhotne.com`).
The crate exposes a library (`novel_downloader`) and one binary
(`novel-downloader`) with two surfaces:

- **Non-interactive CLI** — clap-driven, takes a positional novel URL and
  flags. Drives a sequential or parallel runner, emits an `indicatif`
  progress bar with plain-line fallback when stdout is not a TTY.
- **Interactive TUI** — ratatui flow that walks the user through every
  option, then displays a live download progress screen with a gauge,
  rolling activity log, and Esc-to-abort. Triggered by `-i` or by passing
  no positional URL.

## Common commands

```fish
# build
cargo build --release           # → target/release/novel-downloader

# tests (123 currently)
cargo test                      # everything
cargo test --test runner        # one integration file
cargo test resolve             # one test by name pattern
cargo test -- --nocapture       # surface println/eprintln

# lint and format
cargo clippy --all-targets      # MUST be 0 warnings (CI floor)
cargo fmt                       # rustfmt
cargo fmt --check               # check-only

# run
cargo run --release -- <url> --start 1 --end 50           # CLI
cargo run --release -- -i                                  # TUI
cargo run --release -- <url> --epub-only --chapter-dir D   # EPUB-only
```

A throwaway local mock is the easiest way to smoke-test end-to-end:

```fish
# fixture site under /tmp/novel-downloader-mock with /foo/index.html and /foo/chuong-N/index.html
cd /tmp/novel-downloader-mock && python3 -m http.server 8765 &
cargo run --release -- http://localhost:8765/foo --start 1 --end 3 \
    --if-exists overwrite --output-root /tmp/crawl-out --delay 0 \
    --allow-any-host
```

`--allow-any-host` is the hidden escape hatch that bypasses the
supported-host registry (`metruyenhotne.com` / `metruyenhotvn.com`).
Required for `localhost` mocks and integration tests; do not use for
normal crawls.

## Architecture (the "big picture")

The library is a small layered crate where each module has a single role:

``` shell
cli       ─────────────┐
                       ├─→ runner ─→ crawler/* ─→ source/* ─→ utils (http, fs)
ui/* (ratatui)  ───────┘                       ↘ epub/*   ─→ font (TTF parsing)
```

`crawler`, `epub`, `source` and `ui` are directory modules; the entries
below list each submodule's role.

- **`utils`** (`src/utils.rs`) — pure helpers (`clean_text`, `is_noise`,
  `slugify`, `find_font_file`) plus reqwest-backed
  `fetch_html`/`download_binary` and async fs primitives. Everything
  I/O-related funnels through here so tests can swap in `mockito` and
  `tempfile`.

- **`source/`** — the site seam. Adding a site touches only its own
  module and the registry:
  - `mod.rs` — the object-safe `SiteAdapter` trait (`async-trait`) plus
    `Novel`, `ChapterRef`, `ChapterContent`, `RatePolicy` and
    `SourceError` (typed `RateLimited` / `NotFound` / `ClientRejected` /
    `Unentitled`, everything else stays `anyhow` behind `Other`).
  - `registry.rs` — `resolve(url, allow_any_host)` maps a normalized
    host (lower-cased, `www.`-stripped) to its adapter, and
    `validate_url` wraps it for the wizard. Replaces the old
    `sites.rs` allowlist.
  - `metruyenhot/` — the only adapter today. `parser.rs` holds the
    `scraper` selectors, noise filtering, consecutive-line dedup and the
    JS-hidden `contentS` splice; `discovery.rs` walks the chapter-list
    pagination for the highest chapter number; `metadata.rs` holds the
    five main-page extractors. `fetch_novel` fetches the main page once,
    discovers `N`, and synthesizes refs `1..=N` whose locators are
    `<novel>/chuong-<n>/`.

- **`crawler/`** — split across three files, re-exported from
  `crawler/mod.rs`:
  - `parser.rs` — `build_html_document` and `escape_html`: the
    source-independent on-disk chapter document format.
  - `chapter.rs` — `crawl_chapter` orchestrator that owns the
    fetch-write-skip flow, delegating the fetch to the adapter.
  - `types.rs` — `CrawlChapterParams`, `CrawlResult`, `CrawlStatus`,
    `ExistingChapterDecision`, and the **existing-file policy state
    machine** (`ExistingFilePolicy::Ask` / `Skip` / `Overwrite` /
    `SkipAll`). The `Ask` path takes a prompt callback so the TUI and
    the stdin readline can plug in.

- **`runner`** (`src/runner.rs`) — `crawl_chapters_sequential` and
  `crawl_chapters_parallel` consume a `Vec<ChapterRef>` plus the
  resolved adapter and call
  `crawl_chapter` repeatedly via `SequentialParams` / `ParallelParams`.
  **Sequential propagates `SkipAll` run-wide** so once a user picks
  "skip all" the rest of the run never prompts again. Both runners emit
  `ProgressEvent::Started/Completed/Failed` through an optional
  `Arc<dyn Fn(ProgressEvent) + Send + Sync>` callback and return a
  `RunnerOutcome`. The CLI guards against
  `--workers > 1 && --if-exists ask`.

- **`epub/`** — split across four files, re-exported from
  `epub/mod.rs`:
  - `metadata.rs` — `pick_cover_extension` only; the main-page
    extractors moved to the metruyenhot adapter.
  - `chapters.rs` — `list_chapter_files`,
    `extract_title_and_body_from_saved_chapter`, `SavedChapter`.
  - `package.rs` — XHTML/NCX/OPF/nav builders (`chapter_xhtml`,
    `title_page_xhtml`, `nav_xhtml`, `ncx_xml`, `content_opf`,
    `ChapterEntry`, `ContentOpfParams`).
  - `build.rs` — `build_epub` + `BuildEpubParams`: ties metadata,
    chapters, and package together and zips an EPUB 3 (mimetype is the
    first STORE-compressed entry per spec). Bundled `Bokerlam.ttf` is
    embedded when present; cover extension is picked first from the
    response Content-Type, then the URL path, then `.jpg`.

- **`font`** (`src/font.rs`) — best-effort TTF `name`-table parser. On
  malformed input it falls back to the file stem so EPUB build never
  crashes.

- **`cli`** (`src/cli.rs`) — clap derive `RawArgs` and a normalized
  `CliOptions` (with `from_raw` for the binary, `parse_from` for tests).
  Holds the validators (`validate_shared_options`,
  `validate_chapter_range`).

- **`ui/`** — three layers stacked together, organized into
  subdirectories:
  1. `widgets/` — pure state machines, unit tested without a real
     terminal: `TextInput`, `Select`, `PathInput` (with tab completions
     via `path_completions` / `longest_common_prefix`), and
     `DownloadProgress` + `make_tui_progress_callback`.
  2. `screens/` — synchronous ratatui screens, each opens its own
     `TerminalGuard` (raw mode + alt screen) so the TUI is always torn
     down between prompts:
     - `prompts.rs` — `run_text_prompt`, `run_path_prompt`,
       `run_select`, `run_confirm`, `show_note`, `prompt_block_height`.
     - `loading.rs` — `run_loading_screen` for async novel discovery.
     - `download.rs` — `run_download_screen`: the runner is
       `tokio::spawn`ed, a shared `Arc<Mutex<DownloadProgress>>` is
       updated by the progress callback, and the render loop polls
       keys with an 80ms timeout while watching
       `runner_task.is_finished()`.
  3. `wizard/` — `run_interactive_flow` driving the `WizardStep` state
     machine (`state.rs`) through per-step renderers (`steps.rs`) and
     returning an `InteractivePlan`. `plan.rs` defines `CrawlMode`,
     `InteractivePlan`, `SummaryParams`, and `build_summary`.
  - `mod.rs` also exposes the shared `palette`, `styled_block`,
    `header_paragraph`, `footer_hint`, `next_key_event`, `is_ctrl_c`
    helpers and the `PromptOutcome<T>` enum used by every prompt.

The binary (`src/bin/novel-downloader.rs`) only orchestrates: parse CLI →
either build a non-interactive plan (with `discover_last_chapter_number`

- end-clamping) or `run_interactive_flow` → execute the plan with the TUI
download screen if interactive, else with the indicatif bar → optionally
build the EPUB → exit `0` (success), `2` (partial failures), or `3`
(EPUB build failed).

## Style and discipline

- **TDD is the workflow.** Every new function enters the codebase via a
  test in `tests/<module>.rs` (or a new file alongside) that fails
  first, then a minimal implementation. The
  `superpowers:test-driven-development` skill is the reference; the
  project-local `.agents/skills/rust-testing/SKILL.md` documents
  Rust-specific patterns.
- **Doc-comment every fn.** This is a user-confirmed override of the
  default "no comments" guidance. One line is fine when intent is
  obvious; expand only when there is a non-obvious WHY.
- **Edition 2024 let-chains.** Prefer
  `if let Some(x) = ... && cond { ... }` over nested `if let { if cond }`.
- **Parameter structs** for >5-arg functions
  (`SequentialParams`, `BuildEpubParams`, `ContentOpfParams`,
  `SummaryParams`). Destructure at the top of the function.
- **Pre-compile regexes** with `once_cell::sync::Lazy<Regex>` at the
  module scope.
- **Errors:** `anyhow::Result` for application code; `thiserror` if
  library-style typed errors are needed.
- **No backwards-compat shims.** Edit the API freely; let the compiler
  push call sites.
- **Never auto-commit.** The user owns when to commit. Commit prefixes:
  `feat:` / `fix:` / `refactor:` / `docs:`. **Never** add
  `Co-Authored-By: Claude` trailers — see the user's global CLAUDE.md.

## Test conventions

- Integration tests live under `tests/<module>.rs` and exercise only
  the public API. Current files: `cli.rs`, `crawler.rs`,
  `crawl_chapter.rs`, `epub.rs`, `font.rs`, `runner.rs`, `ui.rs`,
  `utils.rs`. Shared HTML fixtures live under `tests/fixtures/`.
- HTTP is mocked with `mockito::Server::new_async`.
- Filesystem with `tempfile::tempdir()`.
- Test names spell out the behavior:
  `crawl_chapter_writes_html_when_file_missing`,
  `parallel_collects_failures_sorted_by_chapter`.
- The TUI run loop is **not** unit-tested (real terminal required);
  only the underlying state machines (`TextInput`, `Select`,
  `PathInput`, `DownloadProgress`).

## Definition of done

- `cargo test` green (123+ tests).
- `cargo clippy --all-targets` 0 warnings.
- `cargo build --release` succeeds.
- Every new fn has a `///` doc comment.
- For UI-touching changes, eyes-on-terminal verification —
  `cargo test` proves logic but not what the screen looks like.

## Useful project context

- **Bundled font:** `Bokerlam.ttf` at the repo root is embedded into the
  EPUB. `utils::find_font_file` looks (in order): explicit `--font-path`,
  exe dir, exe parent, cwd. Missing font is non-fatal — EPUB falls back
  to a generic serif family.
- **Chapter URL convention:** `<base>/chuong-<N>/`. Output files are
  named `chapter_NNNN.html` (zero-padded).
- **Default output:** `./output/<novel_slug>/`. Override with
  `--output-root`.
- **Fast skip:** when `--fast-skip` is set and the destination chapter
  file already exists, the network fetch is skipped entirely.
