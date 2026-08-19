## 1. Drop cap letter selection

- [ ] 1.1 Add failing tests in `tests/epub.rs` for the pure selection helper:
      a letter opening yields the letter plus the rest of the text, a
      quotation-mark opening yields nothing, a dash opening yields nothing, an
      `&quot;` entity opening yields nothing, a digit opening yields nothing, a
      whitespace opening yields nothing, and empty text yields nothing
- [ ] 1.2 Add a failing test that a first paragraph opening with a decomposed
      `Ế` (base `E` plus combining marks) yields the single precomposed
      character `Ế`, with the remainder starting at the next letter
- [ ] 1.3 Implement the selection helper in `src/epub/package.rs`: NFC-normalize
      the leading text via the existing `unicode-normalization` dependency, take
      the first character, and return `None` unless it is alphabetic

## 2. Span injection into chapter markup

- [ ] 2.1 Add failing tests in `tests/epub.rs` against `chapter_xhtml`: a
      letter-opening body emits one `dropcap` span inside the first paragraph
      and the drop cap class on that paragraph; a three-paragraph body emits
      exactly one span; a dialogue-opening body emits none; a body with no
      paragraph element is passed through unchanged
- [ ] 2.2 Implement first-paragraph rewriting in `chapter_xhtml`, leaving the
      body untouched whenever the helper from group 1 returns `None`
- [ ] 2.3 Add a failing test that `title_page_xhtml` output holds no `dropcap`
      span and no drop cap class, then confirm it passes without new code

## 3. Stylesheet rules

- [ ] 3.1 Add a failing test in `tests/epub.rs` asserting the built stylesheet
      contains the float rule for the `dropcap` class and the zero text-indent
      rule for the drop cap paragraph
- [ ] 3.2 Extend `build_main_css` in `src/epub/build.rs` with one drop cap
      block holding the three tunable values (font size, line height, right
      margin) and a comment naming them as the device-tuning knob

## 4. End-to-end check

- [ ] 4.1 Add a failing test that builds an EPUB from saved chapter fixtures and
      asserts the first chapter document in the archive carries the drop cap
      span while the title page does not
- [ ] 4.2 Update `AGENTS.md` so the `epub/` module notes describe the drop cap
      behavior and the skip rule

## 5. Verification

- [ ] 5.1 `cargo test`
- [ ] 5.2 `cargo clippy --all-targets` with zero warnings
- [ ] 5.3 `cargo fmt --check`
- [ ] 5.4 Build a real EPUB and eyeball chapter openings on KOReader, Boox
      NeoReader, Kindle, and Kobo; confirm the cap spans about three lines and
      that stacked Vietnamese diacritics (`Ế`, `Ộ`, `Ằ`) do not clip into the
      chapter heading, then tune the three CSS values if they do
