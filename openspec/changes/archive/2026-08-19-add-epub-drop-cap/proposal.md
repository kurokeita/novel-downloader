## Why

Generated EPUBs open every chapter with an ordinary indented paragraph, so the
books read as plain text dumps rather than as typeset novels. A drop cap on the
first letter of each chapter is the cheapest typographic cue that a chapter has
begun, and it is the one convention every printed novel already uses.

## What Changes

- The EPUB writer wraps the first letter of a chapter's first paragraph in a
  `<span class="dropcap">` and marks that paragraph with a class, so the
  embedded stylesheet can float the letter across the opening lines.
- The embedded stylesheet gains the drop cap rules: the floated oversized
  letter, and `text-indent: 0` on the paragraph that carries it, which
  overrides the existing `p { text-indent: 2em }`.
- Chapters whose first paragraph does not begin with a letter (dialogue openers
  such as `"Ngươi là ai?"`, an em dash, an HTML entity, or an empty paragraph)
  are left exactly as they are today. Those chapters render without a drop cap
  rather than dropping a punctuation glyph.
- Input in Unicode NFD form is normalized before the first letter is taken, so
  a Vietnamese letter carrying combining marks (`ế`, `ộ`, `ằ`) yields one
  precomposed character in the span instead of a bare base letter with its
  marks stranded outside.
- The behavior is unconditional. No CLI flag, no wizard step, no new field on
  `BuildEpubParams`.

Surfaces changed: **EPUB output only**. The library API, the CLI flags, and the
TUI wizard are all untouched, so there are no breaking changes.

## Capabilities

### New Capabilities

- `epub-drop-cap`: how the EPUB writer selects, marks up, and styles the first
  letter of each chapter, including which chapter openings are skipped.

### Modified Capabilities

None. No existing spec covers EPUB chapter styling.

## Impact

- `src/epub/package.rs`: `chapter_xhtml` gains first-letter detection and span
  injection. Detection is a pure helper in the same module.
- `src/epub/build.rs`: `build_main_css` emits the drop cap rules.
- `tests/epub.rs`: new tests for span injection, the skip cases, NFD input, and
  the emitted CSS.
- Dependencies: none added. `unicode-normalization` is already in `Cargo.toml`
  and supplies the NFC pass.
- Rendering is verified by hand on KOReader, Boox NeoReader, Kindle, and Kobo.
  The three tunable CSS values (`font-size`, `line-height`, and the right
  margin on the floated letter) sit in one block so a device test can be turned
  into a one-line edit.
