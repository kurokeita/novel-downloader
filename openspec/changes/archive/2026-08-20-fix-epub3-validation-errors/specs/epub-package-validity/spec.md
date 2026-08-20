## Purpose

Guarantees that a built EPUB passes validation under EPUB 3.3 rules, so
strict readers and store ingestion pipelines accept it. Covers the package
document's mandatory modification metadata and the cover image's file naming
and declared media type.

## ADDED Requirements

### Requirement: The package document carries a modification timestamp

The EPUB package document SHALL contain exactly one `dcterms:modified`
metadata entry, because EPUB 3 rejects a package that omits it or repeats it.

The value SHALL be a UTC instant formatted `CCYY-MM-DDThh:mm:ssZ`, with no
fractional seconds and no numeric offset, since EPUB 3 accepts only that one
form.

#### Scenario: Rendered package document declares the timestamp

- **WHEN** the package document is rendered with a modification timestamp of
  `2026-08-20T08:42:00Z`
- **THEN** the output contains exactly one
  `<meta property="dcterms:modified">2026-08-20T08:42:00Z</meta>` entry
- **AND** that entry appears inside the `<metadata>` element

#### Scenario: Built EPUB declares a well-formed timestamp

- **WHEN** an EPUB is built
- **THEN** its `EPUB/content.opf` carries one `dcterms:modified` entry whose
  value matches `CCYY-MM-DDThh:mm:ssZ`

### Requirement: The cover image uses the canonical extension for its type

The cover image inside the EPUB SHALL be named with the canonical file
extension for its media type, because validators and readers select an image
decoder by file extension and treat an unrecognized extension as a corrupt
image.

For the EPUB 3 core image media types the canonical extensions are `.jpg` for
`image/jpeg`, `.png` for `image/png`, `.gif` for `image/gif`, `.svg` for
`image/svg+xml`, and `.webp` for `image/webp`.

#### Scenario: JPEG cover is named .jpg

- **WHEN** the cover response declares media type `image/jpeg`
- **THEN** the chosen extension is `.jpg`
- **AND** it is never an alternate JPEG alias such as `.jfif` or `.jpe`

#### Scenario: Each core image type maps to its canonical extension

- **WHEN** the cover response declares `image/png`, `image/gif`,
  `image/svg+xml`, or `image/webp`
- **THEN** the chosen extension is `.png`, `.gif`, `.svg`, or `.webp`
  respectively

### Requirement: An unrecognized media type falls back to a recognized URL extension

When the cover response declares no media type, or declares one outside the
EPUB 3 core image types, the extension SHALL be taken from the cover URL's
path, but only when that extension is one a reader recognizes: `.jpg`,
`.jpeg`, `.png`, `.gif`, `.svg` or `.webp`. Any other path extension, or a
path with no extension, SHALL yield `.jpg`.

The allow-list applies to this branch as well as to the media-type branch,
because an unrecognized extension breaks validation the same way whichever
branch produced it. Falling back rather than rejecting keeps a cover embedded
rather than dropped when a server sends an unhelpful or absent
`Content-Type`.

#### Scenario: Empty media type uses a recognized URL path extension

- **WHEN** the cover response declares no media type and the cover URL path
  ends in `.png`
- **THEN** the chosen extension is `.png`

#### Scenario: Unrecognized URL path extension falls back to .jpg

- **WHEN** the cover response declares no media type and the cover URL path
  ends in `.jfif`
- **THEN** the chosen extension is `.jpg`

#### Scenario: Extensionless URL falls back to .jpg

- **WHEN** the cover response declares no media type and the cover URL path
  carries no extension
- **THEN** the chosen extension is `.jpg`

### Requirement: The declared cover media type matches the file extension

The package document's cover manifest entry SHALL declare a media type
consistent with the cover file's extension, so that a validator selecting a
decoder by either signal reaches the same result.

#### Scenario: Manifest entry agrees with the file name

- **WHEN** an EPUB is built with a JPEG cover
- **THEN** `EPUB/content.opf` declares the cover item as `href="cover.jpg"`
  with `media-type="image/jpeg"`
- **AND** the archive contains an entry at `EPUB/cover.jpg`
