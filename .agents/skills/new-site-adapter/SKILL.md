---
name: new-site-adapter
description: This skill should be used when the user asks to "add a new site", "support another novel site", "write a site adapter", "implement SiteAdapter", "add a source", or names a novel host that novel-downloader does not support yet. Documents the source seam, covering the trait to implement, the module layout, the registry entry, the rate policy, and the fixture convention.
---

# Adding a site adapter

Every novel site lives behind one trait, `SiteAdapter` in `src/source/mod.rs`.
The core pipeline never builds a chapter URL and never parses site HTML, so
adding a site touches its own module, one line of the registry, and the docs.
Nothing in `crawler/`, `runner.rs`, `epub/` or `ui/` should need editing. If a
new site seems to require a change out there, stop and say so: that is a seam
problem, not an adapter problem.

## Files you touch

| Path | Why |
| --- | --- |
| `src/source/<site>/mod.rs` | the adapter struct and its `SiteAdapter` impl |
| `src/source/<site>/*.rs` | pure helpers, split by role (see below) |
| `src/source/registry.rs` | one entry in `ADAPTERS` |
| `tests/source.rs` | adapter tests against `mockito` |
| `tests/fixtures/<site>_*.{json,html}` | recorded responses |
| `README.md` | the supported-hosts list |
| `AGENTS.md` | the `source/` bullet in the architecture section |

## 1. Pick the shape

The two existing adapters are the two shapes worth copying:

- **`metruyenhot`** scrapes HTML. Chapters live at a predictable
  `<novel>/chuong-<n>/`, so `fetch_novel` discovers the highest chapter number
  and **synthesizes** refs `1..=N`. Split: `parser.rs` (selectors, noise
  filtering, dedup), `discovery.rs` (pagination scan), `metadata.rs` (main-page
  extractors).
- **`khodocsach`** calls a JSON API. Chapter ids are opaque database ids, so
  `fetch_novel` **pages the real listing** and cannot synthesize anything.
  Split: `api.rs` (serde response types), `parser.rs` (pure helpers: API base,
  slug extraction, paragraph split).

Synthesize the index only when a chapter number genuinely maps to an address.
The moment ids are opaque, page the listing, and treat a page you cannot fetch
as a failure of the whole index rather than a short novel. Silently truncating
a 2000-chapter book to 200 is worse than erroring.

## 2. Derive the base from the caller's URL

Both adapters compute their request base from the URL they are handed rather
than hard-coding a host. That is the only reason they are testable against a
`mockito` server with no injected base URL. Do the same. A
`const BASE: &str = "https://..."` in an adapter is a bug, not a shortcut.

## 3. Implement the trait methods

`id` / `display_name` / `hosts` / `rate_policy` / `fetch_novel` /
`fetch_metadata` / `fetch_chapter`. Points that are easy to get wrong:

- **`hosts()`** returns lower-case hosts with no `www.` prefix.
  `registry::normalize_host` has already stripped both by the time it compares.
- **`fetch_metadata` is not `fetch_novel` minus the chapters.** It exists so
  `--epub-only` can package an existing directory even when the chapter index
  is unreachable, so it must be the cheap path and must not fetch the listing.
  It is a required method precisely so no adapter can default it into the
  expensive one.
- **`ChapterRef.locator` is opaque and source-owned.** Nothing outside your
  module may interpret it, which also means you may put whatever you need on
  it. khodocsach rides the book title on the locator as a `book` query param
  because its content endpoint omits the title and the crawler needs it for the
  output directory name. That is legitimate, not a hack.
- **`ChapterRef.number` is the display number**, and it drives the
  `chapter_NNNN.html` file name. It does not have to be gapless: khodocsach
  passes the site's own `index`, and one of its novels spreads 1950 chapters
  over indexes 1..1981, so output files skip numbers.
- **`ChapterRef.title`** stays `None` when the index does not carry titles.
  metruyenhot learns the title only on fetch.
- **Map errors to the typed variants** in `SourceError`. `RateLimited` backs
  off the whole run, `Unentitled` fails one chapter and continues, `NotFound`
  and `ClientRejected` get their own wording in `runner::describe_failure`.
  Everything else goes through `Other` and stays an `anyhow::Error`.

## 4. Justify the rate policy in a doc comment

`RatePolicy` is owned by the source, not asked of the user, who has no way to
know a site's limits. Whatever numbers you choose, the `rate_policy` doc
comment must record how you measured them and what would invalidate them. Both
existing policies are unconstrained, but for different reasons: metruyenhot
imposes no observed limit, while khodocsach's limiter buckets by the exact
`User-Agent` header value, which the adapter varies per chapter. Its doc
comment keeps the measured single-header ceiling on record so the constants can
be restored if that ever changes. Do the same. A policy with bare numbers and
no reasoning cannot be revisited by the next person.

## 5. Register the host

Append the adapter to `ADAPTERS` in `src/source/registry.rs`. That is the whole
registration: `supported_hosts`, the wizard's `validate_url`, and the
unsupported-host error message all read from it. Confirm the new host appears
in the error message for an unsupported URL, since that list is generated and a
missing entry means the adapter is not really wired in.

## 6. Tests and fixtures

TDD, as everywhere in this repo: the failing test comes first. Adapter tests go
in `tests/source.rs` and exercise only the public API.

- HTTP is mocked with `mockito::Server::new_async`. Point the adapter at the
  mock by passing a `server.url()`-based novel URL.
- Record fixtures under `tests/fixtures/<site>_<what>.{json,html}` and pull
  them in with `include_str!`.
- **Fixtures mirror the wire format field for field, but the prose fields carry
  invented placeholder text.** No third-party novel text is checked into this
  repo. The khodocsach fixtures do exactly this for `desc` and `content`.
- Cover the failure paths, not just the happy one: a rate-limited response, a
  not-found chapter, and a URL that is on the right host but is not a book page
  (khodocsach rejects those before any request, in `book_slug_from_url`).
- Pure helpers get their own unit tests. Slug extraction, base-URL derivation
  and paragraph splitting are all pure and cheap to test directly.

## 7. Update the docs

- `README.md`: the supported-hosts list, plus a note if the site behaves
  differently in a way a user would notice.
- `AGENTS.md`: a bullet under `source/` describing the module split and any
  non-obvious decision, matching the depth of the two existing entries.

## Definition of done

- `cargo test` green.
- `cargo clippy --all-targets` reports zero warnings.
- Every new fn has a `///` doc comment.
- `git grep` finds no site-specific string outside `src/source/<site>/`.
- One real end-to-end download against the live site, since fixtures prove the
  parser and not the site's actual behavior.
