## Why

`xtruyen.vn` is a WordPress site on the Madara theme holding a large
Vietnamese novel catalog. A live investigation of one novel
(`tinh-day-da-thanh-me-chong-ca-nha-theo-ta-an-thit-uong-ruou`, 218
chapters) established three things that shape the work, all of them
measured rather than assumed:

1. **Chapter prose is not in the served DOM.** The reading container is an
   empty spinner filled by client-side script. The text ships in the same
   response as an obfuscated string in an inline script, recoverable by
   mapping its characters through a custom 64-symbol alphabet onto standard
   base64, base64-decoding, then zlib-inflating. Verified on chapters 1,
   100 and 218. The recovered markup carries only `<br>` and `<div>` tags,
   with paragraphs separated by doubled `<br>`. Advertising markup lives in
   separate script variables injected client-side, so the recovered payload
   needs no noise filtering beyond dropping one known wrapper class.

2. **The chapter index cannot be synthesized from a chapter count.**
   Chapters are addressed as `/truyen/<slug>/chuong-<n>/`, which invites the
   metruyenhot approach of discovering the highest number and synthesizing
   `1..=N`. That is wrong here. Sampling 100,000 chapter URLs from the
   site's own chapter sitemaps found 5,129 (roughly 5%) whose final path
   segment is not a bare number. On `nguyet-nguyet-luan-hoi`, `chuong-1`,
   `chuong-1-1` and `chuong-1-2` are three distinct chapters that all
   return 200, so synthesizing `1..=N` silently omits real chapters rather
   than merely producing dead URLs.

3. **The site enforces a hard rate limit that a header cannot dodge.**
   Measured against live chapter URLs: 2 requests per second ran 50 for 50
   clean; 4 requests per second returned `429` from roughly the sixteenth
   to nineteenth request onward; eight concurrent workers were refused
   almost immediately. Rotating the `User-Agent` per request, the trick that
   makes the khodocsach policy unconstrained, made no difference at all
   (fixed header: 18 successes then `429`; rotating header: 17 successes
   then `429`). The limiter buckets by address, not by header, so the
   adapter has to declare real limits. Refusals clear in under a second and
   do not extend themselves, so backoff recovers quickly.

The third point has a consequence beyond the adapter. Today a source's
`RatePolicy` is enforced silently inside the runner while the interactive
wizard still asks the user for a worker count and an inter-request delay,
and the confirmation summary still reports whatever they typed. For a
source with real limits that means the wizard collects two values it will
override and then displays numbers the run will not use. This change closes
that gap rather than adding a second site that quietly ignores its own
prompts.

## What Changes

- A new source SHALL register the `xtruyen.vn` host, resolve a novel URL
  into metadata plus a complete chapter index, and fetch chapter text.
- The source SHALL recover chapter prose from the encoded inline payload
  described above, and SHALL treat a chapter whose payload cannot be
  decoded as a failed chapter rather than as an empty one.
- The source SHALL read the real chapter listing rather than synthesizing a
  numeric range, so chapters whose address carries no distinct number of
  its own are downloaded like any other.
- The source SHALL declare a rate policy with a genuine concurrency ceiling
  and a genuine minimum delay, and its doc comment SHALL record the
  measurements above and what would invalidate them.
- **The interactive wizard SHALL NOT prompt for a worker count or an
  inter-request delay when the resolved source constrains either one.** It
  SHALL instead state the values the run will use and why they are fixed.
  Sources that constrain neither, which is both existing sources, keep both
  prompts unchanged.
- The confirmation summary SHALL report the pacing the run will actually
  use, so it cannot disagree with the runner.
- The non-interactive CLI SHALL tell the user when a supplied `--workers`
  or `--delay` was overridden by the resolved source, naming the value in
  force. Today the worker clamp is announced through a run-scoped progress
  event and the delay floor is applied with no message at all.

Deliberately out of scope, both noticed during the investigation:

- Reusing the index walk's already-fetched pages as chapter content. Each
  chapter page is roughly 184 KB for roughly 7 KB of prose, so the walk
  pays for pages the download later fetches again. The pipeline indexes
  fully before downloading, so a cache that survives that boundary is a
  larger change than this one.
- Any general per-host request cache or on-disk HTTP cache.

Breaking changes: the confirmation summary needs the resolved pacing, and
`SummaryParams` is public with no compatibility shims in this project, so
its shape is expected to change. No CLI flag is renamed or removed.

## Capabilities

### New Capabilities

- `sources/xtruyen`: how the application obtains novels from `xtruyen.vn`,
  covering URL acceptance, the chapter index walk, recovery of the encoded
  chapter payload, and the pacing the site requires.

### Modified Capabilities

- `wizard-prompt-flow`: gains the rule that pacing prompts are not shown
  when the resolved source fixes those values, in the same spirit as the
  existing rule that build-only mode is not asked for an output root.
- `site-adapters`: gains the rule that pacing enforced on the user's behalf
  is reported rather than applied silently. The existing clamping and
  backoff requirements are unchanged.

## Impact

Surfaces affected: **library API** (`SummaryParams`), **TUI wizard steps**,
**CLI messaging**, and one new library module. No change to the EPUB
artifact.

- `src/source/xtruyen/` (new): the adapter, its payload decoder, its index
  walk, and its main-page metadata extractors.
- `src/source/registry.rs`: one entry in `ADAPTERS`. The supported-host
  list, the wizard's URL validator and the unsupported-host error message
  all read from it.
- `src/ui/wizard/state.rs`: the pure step-routing decision that skips the
  pacing prompts, alongside `step_after_mode`.
- `src/ui/wizard/steps.rs`: the skipped prompts and the back target that
  replaces them.
- `src/ui/plan.rs`: `build_summary` reports resolved pacing.
- `src/bin/novel-downloader.rs`: the override message for `--workers` and
  `--delay`.
- `src/runner.rs`: unchanged behavior. It already clamps concurrency to the
  policy and paces through a shared `Pacer`, which is why this change is
  about what the user is asked and told, not about adding enforcement.
- `README.md` supported-hosts list and the `source/` bullet in `AGENTS.md`.

Test-coverage note the specs and tasks phases both depend on: no test in
`tests/` drives a wizard step function, because those functions own a real
terminal. The existing precedent is to extract the decision into a pure
function and test that, which is how `step_after_mode` is covered by inline
tests in `src/ui/wizard/state.rs`. The pacing-prompt decision follows the
same shape. The decoder, the index walk and the metadata extractors are all
pure or `mockito`-drivable and are covered in `tests/source.rs`.
