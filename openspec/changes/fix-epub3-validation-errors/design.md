## Context

See proposal.md for motivation. Two constraints shape the approach:

- `content_opf` is a pure string renderer today, and `tests/epub.rs` asserts
  on its exact output. Reading a clock inside it would make those assertions
  time-dependent.
- The crate has no direct date or time dependency. `time 0.3.47` is already
  in `Cargo.lock` as a transitive dependency of `ratatui-widgets`.

## Goals / Non-Goals

**Goals:**

- `epubcheck` reports zero errors on a freshly built EPUB.
- `content_opf` stays pure, so its golden-output tests stay deterministic.
- Kill the whole class of extension bug, not just the JPEG instance.

**Non-Goals:**

- Reproducible builds. A modification timestamp makes each rebuild differ
  byte-for-byte by design.
- Validating the EPUB from inside the test suite. `epubcheck` is a Java tool
  and is not a test dependency; verification stays a manual step.
- Rewriting or migrating EPUB files already on disk.

## Decisions

### The timestamp is an input to `content_opf`, not a call inside it

`ContentOpfParams` gains `modified: String`. `build_epub` in
`src/epub/build.rs` stamps it, since that module already owns "assemble the
archive now" concerns and already reaches for the filesystem and the network.

Alternative considered: call the clock inside `content_opf`. Rejected because
every existing golden-output assertion in `tests/epub.rs` would then have to
match a moving value, and the renderer would stop being a pure function of
its inputs for no gain.

### `Rfc3339` with the nanoseconds zeroed, not `format_description!`

EPUB 3 accepts exactly one form, `CCYY-MM-DDThh:mm:ssZ`.

`Rfc3339` renders a zero UTC offset as `Z`, which is what we want. Verified
in the crate's own doc example: `datetime!(1985-04-12 23:20:50.52 +00:00)`
formats as `1985-04-12T23:20:50.52Z`. What it also does is emit a
fractional-seconds component whenever `nanosecond()` is non-zero
(`time-0.3.47/src/formatting/formattable.rs:312`), which `now_utc()` almost
always is, and EPUB 3 rejects fractional seconds. So the timestamp goes
through `replace_nanosecond(0)` before formatting.

Alternative considered: `format_description!` with a literal `Z`. Rejected on
dependency cost. That macro needs the `macros` feature, which pulls
`time-macros`, and `time-macros` is not in `Cargo.lock` today. `Rfc3339`
needs only `formatting`, whose one extra dependency (`itoa`) is already
there. So the macro route would add a proc-macro crate to save nothing.

Second alternative: build the string with one `format!` over the
`OffsetDateTime` accessors, needing only the default `std` feature. Also
zero new crates, and marginally more minimal. Rejected only because the
`Rfc3339` path reuses the crate's tested formatter instead of restating the
shape by hand; the trade is one `Result` unwrap on a `replace_nanosecond(0)`
call that cannot fail.

### `time` over `chrono` or hand-rolled arithmetic

`time` is already compiled in this dependency graph, so promoting it to a
direct dependency with `default-features = false, features = ["std",
"formatting"]` adds no new crate. `chrono` would add a tree for the same
single call. Deriving a civil date from `SystemTime` by hand means writing
calendar arithmetic, which is a bug farm for one line of output.

### An explicit media-type table in `src/epub/cover.rs`

`pick_cover_extension` keeps its home; no new module. It replaces the
`mime_guess::get_mime_extensions_str(..).first()` lookup with a match over
the EPUB 3 core image media types. The reverse lookup is deleted rather than
patched, because its list is alphabetical and any media type with an odd
first alias would reproduce the same failure. Explicit mapping also documents
which image types this project intends to emit.

The same allow-list guards the URL-path fallback. Without it the fix would
only cover the branch that happened to fail in the reported case: a cover URL
ending in `.jfif` served with no `Content-Type` would still produce a `.jfif`
file. `.jpeg` is in the allow-list alongside `.jpg`, because `epubcheck`
accepts both (verified by repacking a built EPUB under each name); only
unrecognized extensions such as `.jfif` fail.

The forward lookup in `content_opf` (`mime_guess::from_ext`) stays: it is now
only ever handed a canonical extension, which `mime_guess` maps correctly.

## Risks / Trade-offs

- Two rebuilds of the same book now produce different bytes → accepted, and
  is what the EPUB 3 metadata means. Nothing in the project compares EPUB
  files by hash.
- A media type outside the core set (for example `image/avif`) now lands on
  `.jpg` once its URL extension fails the allow-list, so an AVIF cover gets a
  misleading name → accepted. `epubcheck` would flag a non-core image type
  whatever the file is called, and no supported site serves one today. The
  alternative, widening the allow-list speculatively, buys nothing until a
  real cover needs it.
- `ratatui-widgets` depends on `time` with `default-features = false`, so
  `std` and `formatting` are additive here → both are feature-only additions
  whose transitive dependency (`itoa`) is already in `Cargo.lock`, so this
  costs a recompile of `time`, not a new crate.
- **BREAKING** `ContentOpfParams` gains a required field. Per project
  convention there is no compat shim; the compiler pushes the two call sites.

## Migration Plan

No data or config migration. EPUB files already built stay invalid until
rebuilt with `--epub-only --chapter-dir <dir>`, which needs no re-crawl.
