## Context

See `proposal.md` (Why). These specs now live in the repository they describe. Every path below is relative to this repo's root, and the survey that follows was re-verified against the working tree rather than carried over from the exploration that produced it.

Current state of this repo, as surveyed:

- Rust, edition 2024. ~5,600 lines of `src/`, ~2,500 lines of `tests/`. Deps that matter here: `reqwest` (rustls), `scraper` + `ego-tree`, `ratatui` + `crossterm`, `zip`, `tokio`, `futures`, `clap`.
- The EPUB writer is hand-rolled (`epub/package.rs` emits OPF, NCX, nav and per-chapter XHTML; `zip` packages it). There is no EPUB crate dependency to work around.
- **Chapters already round-trip through disk.** `crawler/parser.rs::build_html_document` writes a normalized HTML file per chapter to `<output_root>/<novel-slug>/chapter_NNNN.html`; `epub/chapters.rs::extract_title_and_body_from_saved_chapter` reads it back at build time. The two halves communicate only through that file format.
- Site coupling is concentrated in six places: `sites.rs` (host allowlist), `crawler/parser.rs` (CSS selectors), `crawler/discovery.rs` (pagination scan for the highest `/chuong-N/`), `epub/metadata.rs` (five main-page extractors), `utils::build_chapter_url` (`base + "/chuong-" + n`), and `utils::fetch_html`.
- `epub/metadata.rs` exports a sixth public function, `pick_cover_extension`, which is **not** a main-page extractor and **not** site-specific: it maps a response Content-Type or URL suffix to a cover file extension, is called from `epub/build.rs`, and is covered by three tests in `tests/epub.rs`. Every source needs it. It stays in the EPUB layer.
- `BuildEpubParams.novel_main_url` is a live URL, not a saved-page path. `epub/build.rs` uses it three ways: it re-fetches the page for metadata, it is the EPUB `dc:identifier`, and it is the NCX `dtb:uid` source URL. Only the first use goes away.
- `BuildEpubParams.metadata_override: Option<EpubMetadataOverride>` lets the interactive flow override title and author. It predates this change and must keep working.
- `runner::ProgressEvent` has exactly three variants (`Started`, `Completed`, `Failed`), every one of them keyed on a `number: u32`. There is no run-scoped variant.
- `ui::plan::SummaryParams` has no field for the resolved source.
- `sites::validate_url` has exactly two call sites: `ui/wizard/steps.rs` and `src/bin/truyenazz-crawl.rs`.

Constraints discovered by probing khodocsach.com live during exploration:

- It serves a JSON API at `/wp-json/app/v1/`. Book lookup accepts either a numeric id or the slug. The chapter listing is paginated and caps `per_page` at 200 regardless of what is requested.
- Chapter content requires a two-hop handshake: `GET /chapters/{id}/ticket` returns `{nonce, exp, sig, uid}`, then `GET /chapters/{id}?nonce=&exp=&sig=` returns the content. `uid` is `0` — no account needed.
- The ticket expires ~62 seconds after issue and is scoped to one chapter (reusing it for a different chapter returns 401). It is *not* bound to User-Agent or session, and may be replayed for its own chapter.
- The edge returns 403 for a request with no User-Agent. Any non-empty UA passes; no Cloudflare challenge is involved.
- **A rate limiter guards the `/ticket` hop.** Short bursts pass (30 concurrent succeeded), but sustained load does not: 80 chapters at concurrency 8 produced 39 × `429 {"code":"rate_limited"}`, and a follow-up burst 20 seconds later was refused 25/25. No `Retry-After` header is sent. The exact sustainable rate was not established — probing was stopped rather than hammer the host further.
- Chapter content is plain text containing no HTML tags at all.

Re-probed while implementing the adapter (PR 7), correcting two of the bullets above:

- **The chapter listing is ordered newest-first**, not in reading order: page 1 of an 899-chapter book returned `index` 899 down to 700. The adapter must sort ascending before numbering. The original survey did not record this.
- **The 403-without-User-Agent behavior no longer reproduces.** A request sending no `User-Agent` header returned `200` with the normal book payload. A browser-style UA is still sent on every request and `403` is still mapped to `ClientRejected`, but that mapping is now defensive rather than load-bearing.
- The book endpoint returns its payload **unenveloped** (the book object at the top level), while every listing wraps its rows in `{data, pagination}`. `pagination` is `{page, per_page, total, total_pages}`.
- The chapter-listing route is registered for a **numeric** book id only (`/books/(?P<id>\d+)/chapters`), while the book route accepts a slug (`/books/(?P<id>[\w-]+)`). Resolving the slug to an id is therefore a required first hop, not an optimization.
- **Book permalinks carry a `.kds` extension that the API slug does not.** `https://khodocsach.com/nguoi-tim-xac.kds/` is the canonical page (200) for the book whose API `slug` is `nguoi-tim-xac`; the bare `/nguoi-tim-xac/` returns 301 to the `.kds` form, and this holds for every book checked. Since the route pattern `[\w-]+` cannot match a dot, passing the permalink segment through unchanged yields `404 rest_no_route`. Slug derivation must split at the first dot.
- **The listing's `index` field is not always contiguous.** `nguoi-tim-xac` reports 1950 chapters whose indexes span 1..1981, missing 809-814 and 1217-1241. The displayed chapter titles stay continuous across those holes. `ChapterRef.number` carries `index` regardless, so output filenames can skip numbers.
- The content response carries `can_read: bool`. That is the entitlement signal `Unentitled` maps from; a gated chapter's HTTP status was not probed, since no VIP or purchasable book was available to test against.

## Goals / Non-Goals

**Goals:**

- One seam — the source trait — such that adding a third site touches no file outside its own module and the registry.
- Zero behavior change for metruyenhot, verified by its existing test suite rather than by inspection.
- The EPUB, font, widget and screen layers compile untouched.
- Rate policy is data owned by each source, not a number the user is asked to guess.

**Non-Goals:**

- Reworking the on-disk chapter file format. It is already source-independent and it is what makes resume-by-skip work; changing it would invalidate users' partial downloads for no gain.
- Rewriting the EPUB writer, the TUI, or the progress runner's event model.
- A plugin system, dynamic loading, or a config file of site definitions. Two compiled-in sources need none of that.
- Establishing khodocsach's exact rate ceiling in this design. It is an empirical constant to be tuned during implementation.

## Decisions

### Trait shape: object-safe, `async-trait`, opaque locator

```rust
#[async_trait]
pub trait SiteAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn hosts(&self) -> &'static [&'static str];
    fn rate_policy(&self) -> RatePolicy;

    async fn fetch_novel(&self, url: &str) -> Result<Novel>;
    async fn fetch_chapter(&self, r: &ChapterRef) -> Result<ChapterContent>;
}

pub struct Novel {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub cover_url: Option<String>,
    pub chapters: Vec<ChapterRef>,
}

pub struct ChapterRef {
    pub number: u32,           // 1-based, assigned from index position
    pub title: Option<String>, // when the index carries it; metruyenhot learns it only on fetch
    pub locator: String,       // URL, numeric id, whatever — the owning source's business
}

pub struct RatePolicy {
    pub max_concurrency: usize,
    pub min_delay: Duration,
    pub max_retries: u32,
    pub backoff_base: Duration,
}
```

The registry stores `&'static dyn SiteAdapter`, so the trait must be object-safe — which rules out generic methods and RPITIT, and is why `async-trait` (one boxed allocation per call, irrelevant next to an HTTP round trip) is preferred over `impl Future` in trait position. Alternative considered: an enum `Source { Metruyenhot(..), Khodocsach(..) }` with a match in each method. It avoids the dependency and the allocation, but every new site edits every match arm, which is exactly the coupling being removed.

`locator` is a `String` rather than an enum or a generic associated type. An enum would force the core to know each site's addressing scheme — the coupling again, relocated. A GAT would break object safety. The cost is that a source must tolerate a malformed locator at runtime; since locators are only ever produced by the same source that consumes them, this is an internal invariant, not user input.

### The chapter index replaces derived URLs

This is the load-bearing decision. Today `crawl_chapter` takes a `chapter_number: u32` and calls `build_chapter_url(base, n)`. khodocsach cannot do that — chapter ids (`51024`, `51025`, …) are opaque and only the index knows them.

So `fetch_novel` returns the whole index upfront and the runner iterates `ChapterRef`s. metruyenhot's adapter synthesizes the index by discovering the highest chapter number exactly as `discovery.rs` does today, then generating `1..=N` refs whose locators are the URLs it would have built anyway. Its network behavior is unchanged; only the moment of URL construction moves.

Bonus: this deletes the `--start`/`--end` range special-casing from the fetch path. A range is now a slice of the index.

Alternative considered: keep `chapter_number` and give the adapter a `resolve(n) -> locator` hook backed by an internal cache. Rejected — it hides a mandatory bulk fetch behind a per-item API, and makes "how many chapters are there" a second, separate question the adapter must answer consistently.

### Host resolution, no picker

Per the user's decision. `registry::resolve(url) -> Result<&'static dyn SiteAdapter>` normalizes the host (lowercase, strip `www.`) and looks it up across every adapter's `hosts()`. This subsumes `sites.rs::ensure_supported` and keeps its error text shape (name the host, list the supported ones), so the existing wizard validator and its tests keep working with a changed call target.

The wizard gains no step. The URL step already validates; it now also reports which source resolved, on the summary screen.

### Metadata moves upfront; `epub/metadata.rs` dissolves

Its five `extract_*` functions (title, status, description, author, cover URL) become private helpers inside the metruyenhot adapter, called during `fetch_novel`. `BuildEpubParams` gains the `Novel` metadata, so `epub/build.rs` no longer re-fetches the main page. This removes the EPUB layer's last piece of HTML knowledge.

`pick_cover_extension` does **not** move. It is source-independent, `build.rs` calls it after downloading the cover bytes, and both sources need it. It relocates to `epub/build.rs` or a small `epub/cover.rs`, keeping its tests. Only the five extractors leave; the file is deleted once they and `pick_cover_extension` are both rehoused.

**The novel URL stays in `BuildEpubParams`.** Dropping `novel_main_url` alongside the re-fetch would silently change EPUB bytes, because `build.rs` also uses it as the `dc:identifier` in the OPF and as the source URL in the NCX. Those two uses are preserved verbatim, so a rebuilt EPUB is byte-identical to a pre-refactor one for the same novel. Only the `fetch_html` call and the extractor calls disappear. This is what makes the phase-7 EPUB diff a meaningful check rather than an expected-to-differ comparison.

**`metadata_override` wins over source metadata.** The existing `Option<EpubMetadataOverride>` field stays and keeps its current precedence: when the interactive flow supplies a title or author, it overrides what the source returned; when it is `None`, the source's values are used. Today the fallback is "extract from the main page"; after the change it is "read from `Novel`". No user-visible difference.

### Rate policy is clamped by the runner, not by the user

`crawl_chapters_parallel` currently takes the user's concurrency verbatim. It will take `min(user_concurrency, policy.max_concurrency)` and announce the reduction rather than applying it silently. metruyenhot's policy is permissive enough to be a no-op for existing users; khodocsach's is conservative.

Announcing it needs a **new** `ProgressEvent` variant. All three existing variants (`Started`, `Completed`, `Failed`) are keyed on `number: u32` and describe one chapter; a clamp is a property of the run. Add `ProgressEvent::ConcurrencyClamped { requested: usize, effective: usize, source: &'static str }`. Both consumers (`indicatif` in the binary, `DownloadProgress` in the TUI) match on the enum, so both gain an arm: the CLI prints one line before the bar starts, the TUI pushes one entry onto its activity log. Rendering it as a `Failed` event instead would be wrong twice over, since it inflates the failure count and needs a chapter number that does not exist.

429 handling lives in a shared retry wrapper in the runner rather than inside each adapter, so that backoff can be *global* — one chapter hitting the limit should slow the whole run, since the limiter is per-client, not per-chapter. Adapters signal it by returning a typed `SourceError::RateLimited` rather than an opaque `anyhow::Error`; this is the one place the error type needs to be structured.

**The ticket must be acquired inside the per-chapter task**, immediately before the content request — never batched ahead. At concurrency 4 with backoff, queue delay can exceed the ~62s ticket lifetime, so pre-fetched tickets would expire in the queue.

### Dependency scoping, without a feature gate

`scraper` and `ego-tree` become metruyenhot-module-only by construction, since the adapter move puts every HTML-parsing call inside `src/source/metruyenhot/`. `serde`/`serde_json` are added for khodocsach.

They are **not** put behind a Cargo feature. An earlier draft gated them so a khodocsach-only build could drop both, on the reasoning that it was cheap now and expensive to retrofit. Both halves fail on inspection. Nobody makes that build: this ships as an end-user CLI through Homebrew, winget and `cargo install`, all of which take the default feature set, and the proposal notes no library consumers. And the gate is not what is expensive to retrofit. The module boundary is, and that lands here regardless; adding `optional = true` plus a few `#[cfg]` attrs afterwards is mechanical.

Against a saving nobody collects, the gate costs a doubled CI matrix on top of five existing targets, `#[cfg]` branching in the registry and in the host-resolution error text, and a non-default build configuration that only CI ever exercises. Revisit the day someone asks for a slim build, or the day this crate gains a real library consumer.

### The rename is a clean break, and it reaches past `Cargo.toml`

`truyenazz-crawler` names a site the code no longer targets, so the rename is the point of this change rather than cosmetics on top of it. The name is published in more places than the crate manifest, and the survey found all of them:

| Surface | Current | Becomes |
| --- | --- | --- |
| Cargo package / lib / bin | `truyenazz-crawler` / `truyenazz_crawler` / `truyenazz-crawl` | `novel-downloader` / `novel_downloader` / `novel-downloader` |
| GitHub repo | `kurokeita/truyenazz-crawler` | `kurokeita/novel-downloader` |
| Release asset prefix | `BIN_NAME: truyenazz-crawl` | `novel-downloader` |
| Homebrew formula | `Formula/truyenazz-crawler.rb`, class `TruyenazzCrawler` | `Formula/novel-downloader.rb`, class `NovelDownloader` |
| Winget identifier | `Kurokeita.TruyenazzCrawler` | `Kurokeita.NovelDownloader` |
| Clap program name and TUI banner | `truyenazz-crawl` | `novel-downloader` |

**Clean break on both distribution channels.** Per the user's decision: the old Homebrew formula is deleted from the tap with no `oldname` alias, and the old winget identifier is abandoned at its last published version with no moved-manifest PR. Existing users uninstall and reinstall under the new name. This trades a one-time migration cost for zero carried-forward naming debt, and it avoids putting an upstream `microsoft/winget-pkgs` review on the release path. The break must be stated at the top of the release notes, because neither channel will surface it on its own: a `brew upgrade` simply stops seeing the package.

**Version becomes 2.0.0.** Renaming the library crate, the binary and the adapter-facing public API is breaking three times over. The current 1.1.0 cannot carry it.

**`Cargo.toml` gains a `repository` field.** It has none today. The rename is when to add it, pointing at the new URL.

**One `truyenazz` string is not branding and must survive.** `epub/metadata.rs:9-12` holds a `Lazy<Regex>` matching a trailing `" - truyenazz"` suffix on scraped novel titles. That is metruyenhot's page data, not this project's name. It moves into the metruyenhot adapter with the other extractors and its pattern stays byte-for-byte. A blanket find-and-replace across the repo would silently corrupt every scraped title, and no test failure would be obvious about why.

**`.github/workflows/ci.yml` needs no rename.** It is a target-matrix build-and-test workflow that names no binary and uploads no artifact. Only `release.yml` carries the name.

## Risks / Trade-offs

- **A blanket `truyenazz` find-and-replace corrupts scraped titles** → The `" - truyenazz"` suffix regex in `epub/metadata.rs` matches site content, not a project name. Renaming it would leave the suffix on every novel title in every EPUB, and the existing tests assert on cleaned titles rather than on the pattern, so the failure would point somewhere unhelpful. Rename by reviewing each of the 21 files the survey listed, not with `sed`.
- **The clean break is invisible to existing installs** → A deleted Homebrew formula and an abandoned winget identifier both fail quietly; nobody gets told to reinstall. The 2.0.0 release notes must lead with the rename and the reinstall command for each channel.
- **Silent metruyenhot regression during the refactor** → Do the refactor as a pure move with no khodocsach code present, and require the existing suite green before the new adapter is written. The suite is substantial (`tests/crawler.rs`, `tests/epub.rs`, `tests/runner.rs`, `tests/sites.rs`, plus HTML fixtures) and is the actual safety net.
- **khodocsach's sustainable rate is unknown** → Ship conservative defaults (concurrency 2–3, ~500 ms min delay), make them constants in one place, and tune against the live site. Treat the numbers as calibration, not architecture.
- **A long run is a long window for 429s to compound** → Global backoff, plus resume-by-skip already covers a killed run. A user who gets rate-limited can re-run and pick up where they stopped.
- **Chapters published mid-run shift nothing, but a re-run re-indexes** → `number` comes from index position; if the site inserts a chapter, a later run renumbers subsequent chapters and the skip-if-exists check will mismatch filenames. Accepted: it affects only novels updating between runs, and the failure mode is a redundant download, not corruption.
- **`async-trait` boxing on every chapter** → Irrelevant at HTTP timescales. Noted only to close the question.
- **A khodocsach-only build is not possible without a feature gate** → Accepted. No such build is shipped or requested, and the module boundary keeps the option open. This risk replaces the earlier "feature-gating `scraper` can break the default build silently", which was a risk the gate itself created.

## Migration Plan

Four phases, each independently green:

1. **Rename.** `truyenazz-crawler` → `novel-downloader` across the manifest, the binary, every `truyenazz_crawler::` path, the clap program name, the TUI banner, `release.yml` and its two packaging jobs, the GitHub repo, README, `CLAUDE.md`, the release skill, and this change's own `openspec/config.yaml` context. Version to 2.0.0. No logic touched, but **not** a blind find-and-replace: the `" - truyenazz"` title-suffix regex is site data and stays. Tests green.
2. **Introduce the seam, one adapter.** Add the trait, `Novel`/`ChapterRef`/`RatePolicy`, the registry, and move all existing metruyenhot logic behind it. Rewire the runner to iterate refs, and `BuildEpubParams` to take metadata while keeping `novel_main_url` for the identifier and NCX. Delete `sites.rs`'s constant, `build_chapter_url`, and the five `epub/metadata.rs` extractors as public surface; `pick_cover_extension` survives the file. **Existing test suite must pass unmodified except for call-site renames, and `tests/epub.rs`'s `pick_cover_extension` tests must not move.**
3. **Add khodocsach.** New module, `serde` types for the three endpoints, ticket handshake, rate policy, 429 error typing. Unit-test against recorded JSON fixtures the way metruyenhot is tested against HTML fixtures.
4. **Release 2.0.0.** Version bump, repo rename, re-enable the two publish jobs frozen at the start, then retire the old Homebrew formula once the new one is live.

Rollback: phases 2–4 are additive within one repo; reverting is a git revert of the phase. No data migration — the on-disk chapter format is unchanged, so downloads made by the current version remain resumable by the new one.

## Open Questions

- Exact khodocsach rate constants (concurrency, min delay, backoff curve). Deferrable: they are tunable constants that change no interface and no task.
- Whether `--allow-any-host` should map local URLs to a specific adapter or to a test double. Only affects integration-test wiring, resolvable when phase 2's tests are written.
