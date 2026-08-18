## Why

`truyenazz-crawler` is a working Rust TUI that crawls a novel site and builds an EPUB, but every layer below the UI assumes one site shape: chapter URLs derivable as `<novel>/chuong-N/`, metadata scraped from the novel main page with CSS selectors, and no server-side rate limiting. Adding a second source (khodocsach.com) is impossible without breaking those assumptions, because khodocsach serves a JSON REST API with a signed per-chapter ticket handshake and an aggressive token-bucket rate limiter.

The UI, EPUB writer, font embedding, progress runner and CLI are already site-agnostic and worth keeping verbatim. Only the fetch-and-extract layer is coupled. Extracting that layer behind a `SiteAdapter` trait turns a single-site crawler into a general novel downloader, at the cost of one refactor rather than a fork per site.

## What Changes

- **BREAKING**: project renamed `truyenazz-crawler` → `novel-downloader`; binary renamed `truyenazz-crawl` → `novel-downloader`; library crate `truyenazz_crawler` → `novel_downloader`. The old name points at a site the code does not target, so the rename runs the full depth of the project's identity, not just the manifest.
- **BREAKING**: the GitHub repository is renamed `kurokeita/truyenazz-crawler` → `kurokeita/novel-downloader`, and both distribution channels are renamed as a **clean break**: the Homebrew formula becomes `novel-downloader.rb` with the old one deleted from the tap and no `oldname` alias, and winget publishes under a new `Kurokeita.NovelDownloader` identifier with `Kurokeita.TruyenazzCrawler` abandoned at its last version. Existing users must uninstall and reinstall; neither channel signals this on its own, so the release notes must lead with it.
- **BREAKING**: version goes 1.1.0 → 2.0.0.
- Introduce a `SiteAdapter` trait as the single seam between the core pipeline and any source site. The adapter owns URL validation, novel metadata, the chapter index, chapter fetching, and its own rate policy.
- Replace the `SUPPORTED_HOSTS` constant in `sites.rs` with an adapter registry. The adapter is **resolved automatically from the input URL's host** — no adapter picker in the wizard, no CLI flag.
- Move the existing metruyenhot scraping logic (`crawler/parser.rs` selectors, `crawler/discovery.rs` pagination scan, the five `epub/metadata.rs` main-page extractors, `utils::build_chapter_url`) into a `metruyenhot` adapter, with no user-visible behavior change. `pick_cover_extension` stays in the EPUB layer, being source-independent.
- Add a `khodocsach` adapter driving the site's `/wp-json/app/v1/` REST API: slug/id book lookup, paginated chapter index, per-chapter `ticket` → content handshake, and 429 backoff.
- Replace the derived `chapter_number → URL` model with an **upfront chapter index**: adapters return a `Vec<ChapterRef>` and the core pipeline addresses chapters by opaque locator rather than by arithmetic on the base URL. This is what lets a JSON-index site and a URL-pattern site share one runner.
- Add a per-adapter rate policy (max concurrency, inter-request delay, retry/backoff on 429) honored by the parallel runner. Today concurrency is a single global user-supplied number.
- Move novel metadata acquisition upfront (adapter returns it with the chapter index) instead of re-scraping a saved main page during EPUB assembly.
- `scraper` and `ego-tree` become metruyenhot-only dependencies; the khodocsach adapter parses no HTML.

## Capabilities

### New Capabilities

- `site-adapters`: The pluggable source contract — the `SiteAdapter` trait, the chapter-index model, host-based adapter resolution, the registry, unsupported-host errors, and the per-adapter rate policy the runner must honor.
- `sources/khodocsach`: The khodocsach.com source — REST API endpoints used, book resolution by slug or id, chapter index pagination, the signed ticket handshake and its expiry, rate-limit handling, and content normalization.

### Modified Capabilities

None. The target repository has no existing OpenSpec specs; existing metruyenhot behavior is pinned by its current test suite and is required to be preserved unchanged by this refactor.

## Impact

**Affected code** (paths relative to this repo's root):

- Rewritten behind the adapter seam: `src/sites.rs`, `src/crawler/parser.rs`, `src/crawler/discovery.rs`, `src/crawler/chapter.rs`, `src/crawler/types.rs`, `src/epub/metadata.rs`, `src/utils.rs` (`build_chapter_url`, `fetch_html`).
- Modified: `src/runner.rs` (rate policy, chapter refs instead of numbers), `src/cli.rs` (renamed binary, adapter-aware validation), `src/ui/wizard/steps.rs` and `src/ui/plan.rs` (URL step resolves the adapter and surfaces its name; no new step), `src/lib.rs`, `Cargo.toml`.
- Unchanged: `src/epub/package.rs`, `src/epub/build.rs`, `src/epub/chapters.rs`, `src/font.rs`, `src/ui/widgets/**`, `src/ui/screens/**`.

- Renamed but not otherwise touched: `.github/workflows/release.yml` (asset prefix, Homebrew formula, winget identifier), `README.md`, `CLAUDE.md`, `.claude/skills/release/**`, and this change's own `openspec/config.yaml` context block. 21 tracked files contain the string `truyenazz`; `.github/workflows/ci.yml` is not one of them and needs no edit.
- Deliberately **not** renamed: the `" - truyenazz"` title-suffix regex in `epub/metadata.rs`, which matches metruyenhot page content rather than this project's name.

**Dependencies**: add `serde` + `serde_json` (khodocsach JSON), add `async-trait` (or use RPITIT / boxed futures) for the trait; `scraper` and `ego-tree` become confined to the metruyenhot module by construction, with no Cargo feature gate (see design.md, Dependency scoping).

**Distribution**: existing Homebrew and winget installs break by design and do not auto-migrate. `Cargo.toml` gains the `repository` field it currently lacks.

**Interfaces**: the `novel_downloader` library's public surface changes shape (adapter trait exported, crawler internals no longer public). No stable external consumers known.

**Operational**: khodocsach's rate limiter is strict enough that a full novel download is minutes-to-tens-of-minutes, not seconds. Progress and resumability matter more than for metruyenhot; the existing skip-if-exists behavior covers resume.

**Out of scope**: khodocsach audio novels, authenticated/VIP/paid books, whole-catalog crawling, and any site beyond the two named adapters.
