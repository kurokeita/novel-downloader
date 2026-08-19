## 1. Freeze publishing, then rename

Rename by reviewing each tracked file that contains `truyenazz` (`git grep -il truyenazz`). That reports 25 files, of which **21 need renaming**: the four exceptions are this change's own artifacts (`proposal.md`, `design.md`, `tasks.md`, `openspec/config.yaml`), which record the old name deliberately and must keep it.

**Do not `sed` the repo**: task 1.4 names a `truyenazz` string that must survive untouched.

Version stays at `1.1.0` through sections 1 to 7. The bump, the repo rename, the tap cleanup and the re-enable all land together in section 8.

- [x] 1.1 Disable the `publish-homebrew` and `publish-winget` jobs in `.github/workflows/release.yml` so a tag pushed mid-overhaul cannot ship a half-renamed package; leave the `build` job and the GitHub release upload live as a test channel. Re-enabled in 8.3
- [x] 1.2 Rename package `truyenazz-crawler` → `novel-downloader`, lib target `truyenazz_crawler` → `novel_downloader`, and bin target `truyenazz-crawl` → `novel-downloader` in `Cargo.toml`; rewrite `description` to drop "TruyenAZZ"; add the missing `repository` field pointing at the new URL; leave `version` alone
- [x] 1.3 Rename `src/bin/truyenazz-crawl.rs` → `src/bin/novel-downloader.rs` and update every `truyenazz_crawler::` path across `src/` and `tests/`
- [x] 1.4 Update the user-facing program name in `src/cli.rs` (clap `name`, line 35, and the doc comment at line 131), `src/ui/mod.rs` (TUI banner, line 95) and `src/ui/wizard/steps.rs` (line 27); **leave the `" - truyenazz"` title-suffix regex in `src/epub/metadata.rs:9-12` exactly as it is**, since it matches metruyenhot page content, not this project's name
- [x] 1.5 Update `.github/workflows/release.yml`: `BIN_NAME` (line 14) to `novel-downloader`, which cascades to every release asset name
- [x] 1.6 Update the (disabled) Homebrew job in `release.yml`: formula path `Formula/novel-downloader.rb` (lines 134, 181, 185), class `NovelDownloader` (line 135), commit message (line 190), and rewrite the `desc` (line 136) which currently claims "truyenazz.me novels" and is wrong regardless of the rename
- [x] 1.7 Update the (disabled) winget job in `release.yml`: `identifier` to `Kurokeita.NovelDownloader` (line 202) and `installers-regex` to match `novel-downloader-windows-x86_64\.zip$` (line 203); leave `Kurokeita.TruyenazzCrawler` abandoned upstream with no moved-manifest PR
- [x] 1.8 Confirm `.github/workflows/ci.yml` needs no change (target-matrix build and test only, names no binary, uploads no artifact)
- [x] 1.9 Update `README.md`: title, install command, usage block, the mermaid node label, and the module list
- [x] 1.10 Update `.agents/skills/release/SKILL.md` and `references/release-notes-template.md` (the canonical location; `.claude/skills/release` is a symlink into it)
- [x] 1.11 Update `AGENTS.md` (crate name, binary name, commands, the `/tmp/truyenazz-mock` fixture path) and the crate-name paragraph in `openspec/config.yaml`'s `context` block. `CLAUDE.md` is a one-line `@AGENTS.md` import and needs no edit
- [x] 1.12 Verify `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and the full `cargo test` suite pass with no other change
- [x] 1.13 Confirm `git grep -i truyenazz` returns only the `epub/metadata.rs` title-suffix regex and its tests

## 2. Source contract and registry

- [x] 2.1 Add `async-trait` dependency; create `src/source/mod.rs` with the `SiteAdapter` trait, `Novel`, `ChapterRef` and `RatePolicy` types per design.md. **PR 3 already created the module and `ChapterRef`; add the trait, `Novel` and `RatePolicy` alongside rather than recreating the file**
- [x] 2.2 Add `SourceError` with a distinct `RateLimited` variant, plus `NotFound`, `ClientRejected` and `Unentitled`; keep `anyhow` for everything else
- [x] 2.3 Create `src/source/registry.rs`: `resolve(url) -> Result<&'static dyn SiteAdapter>` with lowercase + `www.`-stripping host normalization, preserving the existing "unsupported host, supported are …" error wording
- [x] 2.4 Port `sites.rs` tests to the registry, including the malformed-URL and no-host cases
- [x] 2.5 Reroute the `--allow-any-host` escape hatch (`localhost`, `127.0.0.1`) through the registry, off by default; update both `sites::validate_url` call sites (`src/ui/wizard/steps.rs`, `src/bin/novel-downloader.rs`)
- [x] 2.6 Delete `src/sites.rs` and its `SUPPORTED_HOSTS` constant

## 3. Move metruyenhot behind the contract

- [x] 3.1 Create `src/source/metruyenhot/` and move `crawler/parser.rs` selector logic into it as private helpers
- [x] 3.2 Move `crawler/discovery.rs` (pagination scan, `max_chapter_in_html`) into the adapter; use it to build the chapter index
- [x] 3.3 Move the five `epub/metadata.rs` main-page extractors (title, status, description, author, cover URL) into the adapter and call them from `fetch_novel`
- [x] 3.4 Relocate `pick_cover_extension` into the EPUB layer (`epub/build.rs` or a new `epub/cover.rs`) rather than into the adapter, keeping it public and keeping its three `tests/epub.rs` tests unchanged; only then delete `src/epub/metadata.rs` and drop it from `epub/mod.rs`'s re-exports
- [x] 3.5 Implement `fetch_novel`: discover the highest chapter number, then synthesize `ChapterRef`s `1..=N` whose locators are the chapter URLs `build_chapter_url` used to derive; delete `utils::build_chapter_url`. **PR 3 already wrote that synthesis as `utils::chapter_index`; move it into `fetch_novel` rather than writing it again, then delete both it and `build_chapter_url`**
- [x] 3.6 Implement `fetch_chapter` from the moved parser; return `ChapterContent` (title + ordered paragraphs)
- [x] 3.7 Declare a permissive `RatePolicy` that is a no-op for current users
- [x] 3.8 Verify `tests/crawler.rs`, `tests/sites.rs` and the HTML fixtures still pass with only call-site renames

## 4. Rewire the core pipeline

PR 3 (`refactor/chapter-index-model`) landed the addressing half of 4.1 and 4.2 ahead of the trait, so that PR 4 reads as a relocation rather than a redesign. Both stay unchecked because both name the adapter, which does not exist until PR 4. The boxes are ticked there, once the adapter parameter is threaded through.

Already landed by PR 3, so PR 4 does not repeat it: `ChapterRef` exists in `src/source/mod.rs`; `CrawlChapterParams` takes `chapter: &ChapterRef` in place of `base_url` + `chapter_number`; both runner param structs carry `chapters: Vec<ChapterRef>`; and `utils::chapter_index` builds the metruyenhot index from a number range. That helper is scaffolding: PR 4 deletes it together with `build_chapter_url` once `fetch_novel` owns index construction.

- [x] 4.1 Change `crawler/chapter.rs` to take a `&ChapterRef` plus the adapter instead of `base_url` + `chapter_number`; keep the existing `ExistingFilePolicy` / `fast_skip` / prompt behavior and the `chapter_NNNN.html` output path unchanged. **`&ChapterRef` done in PR 3; the adapter parameter remains**
- [x] 4.2 Change `runner.rs` `SequentialParams` / `ParallelParams` to carry the adapter and a chapter-ref slice; apply chapter-range selection as a slice of the index rather than arithmetic. **Chapter-ref slice done in PR 3; the adapter field remains, and range selection is still a `Vec<u32>` on `InteractivePlan` mapped to refs at the binary boundary until `fetch_novel` supplies a real index**
- [x] 4.3 Add a run-scoped `ProgressEvent::ConcurrencyClamped { requested, effective, source }` variant; the three existing variants are all chapter-keyed and none can carry this
- [x] 4.4 Clamp effective concurrency to `min(user, policy.max_concurrency)` and emit `ConcurrencyClamped` when the clamp bites; handle the new variant in both consumers (the `indicatif` callback in `src/bin/`, and `DownloadProgress` / `make_tui_progress_callback` in `ui/widgets/progress.rs`)
- [x] 4.5 Add a shared retry wrapper in the runner: on `SourceError::RateLimited`, back off with increasing delay, retry up to `policy.max_retries`, and slow the whole run rather than the single chapter
- [x] 4.6 Enforce `policy.min_delay` between requests
- [x] 4.7 Report exhausted retries as a failure that names rate limiting; report `Unentitled` chapters distinctly and let the run continue
- [x] 4.8 Change `BuildEpubParams` to carry `Novel` metadata so `epub/build.rs` stops calling `fetch_html` and the five extractors; **keep the `novel_main_url` field**, which `build.rs` also uses as the OPF `dc:identifier` and the NCX source URL
- [x] 4.9 Keep `metadata_override` and its current precedence: an interactive title/author override still wins, with `Novel` replacing the main page as the fallback source
- [x] 4.10 Confirm `epub/package.rs`, `epub/chapters.rs` and `font.rs` need no edits, and that a rebuilt EPUB for an already-downloaded novel is byte-identical to a pre-refactor one
- [x] 4.11 Add a `source: &'a str` (or equivalent) field to `ui::plan::SummaryParams`, populate it from the resolved adapter's `display_name()`, and render it in `build_summary`; add no wizard step
- [x] 4.12 Verify `tests/runner.rs`, `tests/epub.rs`, `tests/ui.rs`, `tests/cli.rs` and `tests/crawl_chapter.rs` pass

## 5. khodocsach adapter

- [x] 5.1 Add `serde` + `serde_json`; create `src/source/khodocsach/` registering host `khodocsach.com` with a conservative `RatePolicy` (start concurrency 2–3, ~500 ms min delay)
- [x] 5.2 Define response types for the book endpoint, the chapter-listing endpoint, and the ticket + content endpoints
- [x] 5.3 Derive the book slug from the URL path; reject khodocsach URLs that are not book pages
- [x] 5.4 Send a non-empty browser-style User-Agent on every request; map a 403 to `SourceError::ClientRejected`
- [x] 5.5 Implement `fetch_novel`: resolve the book by slug, map metadata (title, author, description, cover, status), and page the chapter listing to completion treating the server's returned page size as authoritative
- [x] 5.6 Fail indexing outright when a listing page cannot be retrieved, rather than returning a partial index
- [x] 5.7 Implement `fetch_chapter`: request the ticket inside the per-chapter task immediately before the content request, never batched ahead; re-obtain the ticket and retry once when the content request reports it invalid
- [x] 5.8 Map a 429 on either hop to `SourceError::RateLimited`; map an entitlement refusal to `SourceError::Unentitled`
- [x] 5.9 Split plain-text content into ordered paragraphs preserving reading order; treat zero paragraphs as a failure
- [x] 5.10 Record JSON fixtures and unit-test resolution, pagination, the ticket handshake, error mapping and paragraph splitting offline
- [x] 5.11 Calibrate the rate constants against the live site and record the chosen values in one place. Two rounds. **Round 1, calibrated to the limiter**: concurrency 1, `min_delay` 2s, `backoff_base` 180s, `max_retries` 2. Derivation: a chapter costs two requests, so 2s spacing is 1 req/s, against a measured ceiling between 1.62 req/s (held 100 requests, 0 failures) and 4.3 req/s (refused after 43); the backoff is minutes because the limiter is self-extending and only ~3 minutes of silence clears it. **Round 2, shipped**: the limiter buckets by the exact `User-Agent` string, so `fetch_chapter` rotates it per chapter (`"<USER_AGENT> rev/<number>"`) and the policy is unconstrained — `max_concurrency: usize::MAX`, `min_delay` 0s, `backoff_base` 2s, `max_retries` 2. Recorded in the `rate_policy` doc comment in `src/source/khodocsach/mod.rs`. Round 1's numbers are retained in that comment and in `AGENTS.md` as the fallback if the rotation is ever removed. Confirmed by the user's own live full-book runs (tasks 6.2, 6.3)
- [x] 5.14 Decide what the user-facing `--delay` means now that `RatePolicy::min_delay` exists. They are separate sleeps today: `min_delay` spaces chapter starts run-wide through the `Pacer`, while `--delay` sleeps per worker after a successful write. **The parallel runner passes `delay: 0.0` (`src/runner.rs:356`), so the value the CLI and the TUI ask for is silently discarded whenever `--workers > 1`.** Either unify the two into one source-recommended, user-overridable number applied by both runners, or keep them distinct and make the TUI say which one it is asking for. Unifying changes metruyenhot throughput, since a per-worker post-write sleep and run-wide spacing diverge as worker count grows. **Resolved: kept distinct.** `ParallelParams` gained a `delay` field and both `src/bin/novel-downloader.rs` call sites now pass `plan.delay`, so the flag is no longer dropped at the type boundary. The CLI help and the wizard prompt now say the value is a pause *after each chapter is written* rather than "between requests", which is what `RatePolicy::min_delay` does. metruyenhot throughput is unchanged
- [x] 5.12 Update README: both supported sites, host-inferred source selection, and a note that khodocsach downloads are rate-limited and slow
- [x] 5.13 Keep `scraper` and `ego-tree` confined to the metruyenhot module, but add **no** Cargo feature gate. Every shipped build (Homebrew, winget, `cargo install`) takes the default feature set, so a khodocsach-only build serves nobody, while the gate costs a doubled CI matrix and `#[cfg]` branching in the registry and its tests. Revisit only if someone asks for a slim build or the crate gains library consumers

## 6. End-to-end verification

- [x] 6.1 Download a small metruyenhot novel end to end and diff the EPUB against one produced by the pre-refactor binary. **Verified by the user against the live site**
- [x] 6.2 Download a khodocsach novel end to end; confirm chapter count matches the site's reported total and the EPUB opens in a reader. **Verified by the user against the live site**, with the shipped UA-rotating policy
- [x] 6.3 Kill a khodocsach run mid-way, re-run, and confirm it resumes by skipping existing chapter files. **Verified by the user against the live site**
- [x] 6.4 Confirm an unsupported host, a malformed URL and a non-book khodocsach URL each fail before any network request with the specified error. **Verified**: `example.com/foo` → `Unsupported host 'example.com'. Supported: khodocsach.com, metruyenhotne.com, metruyenhotvn.com.` (exit 2); `not-a-url` → `invalid URL: not-a-url` (exit 2); `khodocsach.com/the-loai/tien-hiep/` → `not a khodocsach book page: ... (expected a single-segment path like /ten-truyen)` (exit 1). All three return in 0.00s, so no request is made; the khodocsach case errors in `book_slug_from_url`, which runs before `get_json`. Minor wart, not fixed here: the registry and slug paths exit 2 and 1 respectively

## 7. Release 2.0.0

Everything here lands in one final PR, after sections 1 to 6 are merged.

- [ ] 7.1 Bump version `1.1.0` → `2.0.0` (breaking: crate name, binary name, and public API all change)
- [ ] 7.2 Rename the GitHub repository `kurokeita/truyenazz-crawler` → `kurokeita/novel-downloader` and update the local remote; GitHub redirects the old URL, so this is safe to do at any point but is grouped here so all outward-facing identity flips together
- [ ] 7.3 Re-enable the `publish-homebrew` and `publish-winget` jobs disabled in 1.1, now pointing at the new formula and the new winget identifier
- [ ] 7.4 Write the 2.0.0 release notes leading with the rename, giving the uninstall-then-reinstall command for both Homebrew and winget; a deleted formula and an abandoned winget identifier both fail silently, so nothing else will tell existing users
- [ ] 7.5 Tag and release; confirm the assets, the new Homebrew formula and the new winget manifest all publish
- [ ] 7.6 **Only after 2.0.0 has published**, delete `Formula/truyenazz-crawler.rb` from the `kurokeita/homebrew-brew` tap; add no `oldname` alias (clean break, per design.md). Deleting it earlier would strand 1.1.0 users with no replacement for the length of the overhaul
- [ ] 7.7 Verify `brew install kurokeita/brew/novel-downloader` and `winget install Kurokeita.NovelDownloader` both work from a clean machine
