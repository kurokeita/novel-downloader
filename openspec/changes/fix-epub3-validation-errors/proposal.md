## Why

Every EPUB this project has ever produced fails `epubcheck` under EPUB 3.3
rules with two errors. Both were reproduced on three separately built books:

```
ERROR(RSC-005): EPUB/content.opf(3,55): package dcterms:modified meta element
                must occur exactly once
ERROR(PKG-021): EPUB/cover.jfif(-1,-1): Corrupted image file encountered.
```

The package document claims `version="3.0"` while omitting metadata that
EPUB 3 requires, and the cover image is written under a file extension that
readers and validators do not recognize. Neither error is cosmetic: strict
readers and store ingestion pipelines reject on both.

## What Changes

- `content_opf` emits exactly one `<meta property="dcterms:modified">` in
  `<metadata>`, formatted as `CCYY-MM-DDThh:mm:ssZ` (UTC, no fractional
  seconds), which is what EPUB 3.3 mandates.
- **BREAKING** (library API): `ContentOpfParams` gains a `modified: String`
  field so the timestamp is passed in rather than read from the clock inside
  the renderer. Keeps `content_opf` pure and unit-testable. `build_epub`
  stamps the value.
- `pick_cover_extension` stops deriving the extension from a reverse MIME
  lookup and maps the EPUB 3 core image media types explicitly
  (`image/jpeg` to `.jpg`, `image/png` to `.png`, `image/gif` to `.gif`,
  `image/svg+xml` to `.svg`, `image/webp` to `.webp`).
- The URL-path fallback is now gated on the same allow-list (`.jpg`,
  `.jpeg`, `.png`, `.gif`, `.svg`, `.webp`) instead of passing any path
  extension through, since a cover URL ending in `.jfif` with no
  `Content-Type` would otherwise reproduce the bug through the other branch.
  The final `.jpg` fallback is unchanged.
- New direct dependency on `time` for the UTC clock, as
  `default-features = false, features = ["std", "formatting"]`. Already in
  `Cargo.lock` as a transitive dependency of `ratatui-widgets`, and its one
  extra transitive dependency (`itoa`) is there too, so no new crate.

No CLI flags change. No TUI wizard steps change. Existing EPUB files on disk
are not migrated; they need a rebuild.

## Capabilities

### New Capabilities

- `epub-package-validity`: the built EPUB validates against EPUB 3.3 rules.
  Covers the mandatory `dcterms:modified` package metadata and the cover
  image file naming and declared media type.

### Modified Capabilities

None. `epub-drop-cap` covers chapter body rendering only and is untouched.

## Impact

- `src/epub/package.rs`: `content_opf`, `ContentOpfParams`.
- `src/epub/cover.rs`: `pick_cover_extension`.
- `src/epub/build.rs`: stamps `modified` when constructing
  `ContentOpfParams`.
- `Cargo.toml`: `time` promoted to a direct dependency.
- `tests/epub.rs`: existing `pick_cover_extension` and `content_opf` tests
  need updating alongside the new ones.
- Root cause note for the cover bug: `mime_guess::get_mime_extensions_str`
  returns extensions in alphabetical order, so `.first()` on the
  `image/jpeg` list yields `jfif` ahead of `jpe`, `jpeg` and `jpg`. Taking
  the first entry of a reverse MIME lookup assumes a canonical ordering that
  `mime_guess` never promised, which is why the fix removes the round trip
  rather than special-casing JPEG.
