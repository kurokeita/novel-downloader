## 1. Dependency

- [x] 1.1 Promote `time` to a direct dependency in `Cargo.toml` as
      `default-features = false, features = ["std", "formatting"]`; do NOT
      add `macros`, which would pull `time-macros` (absent from `Cargo.lock`)
      for no gain. Confirm with `cargo tree` that no new crate appears beyond
      the 0.3.47 already pinned

## 2. Cover extension mapping

- [x] 2.1 Add failing tests in `tests/epub.rs`:
      `pick_cover_extension_maps_jpeg_to_jpg` asserting `image/jpeg` yields
      `.jpg` and never `.jfif`, plus
      `pick_cover_extension_maps_core_image_types` covering `image/png`,
      `image/gif`, `image/svg+xml` and `image/webp`
- [x] 2.2 Add a failing test
      `pick_cover_extension_rejects_unrecognized_url_extension` asserting
      that an empty media type with a URL path ending in `.jfif` yields
      `.jpg`, so the fallback branch cannot reintroduce the bug
- [x] 2.3 Replace the `mime_guess::get_mime_extensions_str(..).first()`
      lookup in `src/epub/cover.rs` with an explicit match over the EPUB 3
      core image media types, and gate the URL-path fallback on the same
      allow-list (`.jpg`, `.jpeg`, `.png`, `.gif`, `.svg`, `.webp`), with
      `.jpg` for anything else
- [x] 2.4 Confirm the pre-existing fallback tests at `tests/epub.rs:62-76`
      still pass unchanged: `.bin` + `image/png` still yields `.png`, and the
      `.jpeg` URL-path case stays valid because `epubcheck` accepts `.jpeg`

## 3. Package modification timestamp

- [x] 3.1 Add a failing test
      `content_opf_declares_dcterms_modified_exactly_once` in `tests/epub.rs`
      that passes a fixed timestamp and asserts a single
      `<meta property="dcterms:modified">` entry inside `<metadata>`
- [x] 3.2 Add `modified: String` to `ContentOpfParams` and emit the `<meta>`
      entry from `content_opf` in `src/epub/package.rs`
- [x] 3.3 Update the existing `content_opf_includes_metadata_and_spine` test
      and any other `ContentOpfParams` construction site for the new field

## 4. Stamping the timestamp at build time

- [x] 4.1 Add a failing test that builds an EPUB and asserts its
      `EPUB/content.opf` carries one `dcterms:modified` value matching
      `CCYY-MM-DDThh:mm:ssZ` (no fractional seconds, no numeric offset)
- [x] 4.2 Add a `///`-documented helper in `src/epub/build.rs` that takes
      `OffsetDateTime::now_utc()`, calls `replace_nanosecond(0)` so no
      fractional-seconds component is emitted, formats with the well-known
      `Rfc3339` (which renders a zero offset as `Z`), and pass its result
      into `ContentOpfParams`

## 5. Documentation

- [x] 5.1 Update the `epub/` section of `AGENTS.md`: `content_opf` now takes
      a modification timestamp, `pick_cover_extension` maps media types
      explicitly, and note why the reverse `mime_guess` lookup was removed

## 6. Verification

- [x] 6.1 `cargo test`
- [x] 6.2 `cargo clippy --all-targets` with zero warnings
- [x] 6.3 `cargo fmt --check`
- [x] 6.4 Rebuild one real book with `--epub-only` and confirm `epubcheck`
      reports zero errors, replacing the two errors recorded in proposal.md
