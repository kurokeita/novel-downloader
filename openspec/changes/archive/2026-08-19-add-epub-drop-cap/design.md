## Context

See `proposal.md` - Why for motivation. The constraints that shape this design
are the reading devices and the shape of the data reaching the EPUB writer.

**Target readers.** KOReader (crengine), Boox NeoReader, Kindle, and Kobo. Three
of the four are not browser engines. Kindle converts a sideloaded EPUB to its
own format before rendering, and Kobo's sideloaded path goes through an Adobe
derived renderer. This rules out CSS that only browsers implement well.

**What the writer receives.** `chapter_xhtml(title, body_html)` in
`src/epub/package.rs` is handed `body_html` from
`extract_title_and_body_from_saved_chapter`, which is the inner HTML of the
crawler's `.chapter-content` div after a `scraper` reparse. In practice that is
a run of `<p>` elements holding escaped text and nothing else. The writer
currently splices it into a document template without inspecting it.

**Styling lives in one place.** `build_main_css` in `src/epub/build.rs` produces
the entire stylesheet as one format string, including `p { text-indent: 2em }`.
Every document in the archive links it, chapters and title page alike.

**Text is Vietnamese.** Letters carry stacked diacritics, and the encoding form
reaching the writer is not guaranteed to be NFC.

## Goals / Non-Goals

**Goals:**

- One drop cap per chapter that survives all four target renderers, degrading to
  ordinary text rather than to broken layout where it does not.
- A first-letter selection rule that is correct for Vietnamese regardless of the
  input's normalization form.
- CSS values that can be retuned after a device test without restructuring code.

**Non-Goals:**

- Raised caps, sunken caps, or small-caps continuation of the first words. One
  floated initial only.
- Per-book or per-run configurability. See `proposal.md` - What Changes.
- Correct rendering when the reader is configured to ignore embedded styles.
  KOReader can be told to do this; nothing in the file can override it.
- Drop caps on chapter openings that begin with punctuation. Skipping is
  specified behavior, not a limitation to work around later.

## Decisions

### Explicit span over `::first-letter`

The drop cap letter is wrapped in `<span class="dropcap">` during document
generation, and the paragraph carrying it is marked with its own class.

*Alternative considered: `p:first-of-type::first-letter`.* Attractive because it
needs no markup change and no Rust-side text handling, and because the CSS spec
already folds preceding punctuation into the pseudo-element, which would have
handled dialogue openings for free. Rejected on reader support. Kindle's
converter has historically dropped `::first-letter`, and the pseudo-element plus
`float` combination is the least reliable part of the older Adobe and crengine
CSS implementations. The span is what commercially published EPUBs use, which
means it is the path these renderers are actually exercised against.

*Failure modes differ, and that asymmetry reinforced the choice.* Where
`::first-letter` is unsupported the paragraph silently renders as plain text.
Where `float` is unsupported on a span, an oversized letter sits inline and
forces open the leading line. But an engine that supports neither is one where
the span approach fails no worse, and every engine that supports one supports
`float` more widely than the pseudo-element.

### Skip openings that do not begin with a letter

A floated span renders to the left of everything preceding it in the line box.
So for a paragraph opening with a quotation mark, there is no arrangement where
the quote stays small and stays before the cap: either the punctuation joins the
span and renders oversized, or it ends up beside the initial rather than in front
of it.

*Alternative considered: fold leading punctuation into the span,* matching what
`::first-letter` would have done. Rejected because a three-line-tall quotation
mark or dialogue dash is worse typography than no drop cap, and Vietnamese web
novels open chapters with dialogue often enough that this is a common case, not
an edge case.

The trade-off accepted is visual inconsistency between chapters: prose openings
get a cap, dialogue openings do not. The alternative was consistency at the cost
of an ugly result on the inconsistent chapters.

*Second-order benefit:* the guard is "does the opening begin with an alphabetic
character", which also rejects entity references (`&quot;`), digits, whitespace,
and empty paragraphs. One condition covers four skip cases, so there is a single
place where the decision is made and a single place to change it.

### NFC normalization instead of grapheme segmentation

The opening text is normalized to NFC before the first character is taken.

*Alternative considered: `unicode-segmentation` for true grapheme clusters.*
Rejected because `unicode-normalization` is already a dependency and NFC is
sufficient here. Every Vietnamese letter has a precomposed form, so after NFC a
letter and its diacritics are one `char` and the naive "take the first `char`"
becomes correct. Grapheme segmentation would be the right tool for a script
where no precomposed form exists, which is not the input this crate handles.

Taking the first `char` without normalizing would split a decomposed `Ế` into a
bare `E` inside the span with its combining marks stranded immediately after the
closing tag, where they would attach to whatever glyph follows.

### Rewriting the body string rather than reparsing it

The first-paragraph rewrite operates on the `body_html` string. The writer looks
for the first paragraph's opening tag, inspects the text that follows, and
splices in the span when the guard passes.

*Alternative considered: reparse with `scraper`, mutate, reserialize.*
`scraper` is already a dependency and this would be structurally safer. Rejected
because `scraper`'s tree is not built for mutation and reserializing risks
changing escaping and whitespace throughout the body, which would make the
"identical to before" guarantee in the specs hard to hold. String splicing
touches exactly the bytes it inserts.

The safety this gives up is real but bounded: the input is the crawler's own
output, which is a flat run of `<p>` elements with escaped text and no nested
markup. If the guard cannot find that shape, it does nothing and the body is
passed through untouched.

### CSS structure

The stylesheet gains one block whose three tunable values sit together:

```css
p.dropcap-para { text-indent: 0; }
.dropcap {
  float: left;
  font-size: 3.2em;      /* tune: cap height across ~3 lines */
  line-height: 0.8;      /* tune: raise if stacked marks clip the heading */
  margin-right: 0.08em;  /* tune: gap to the wrapped text */
}
```

Values are starting points, not derived results. Cap height depends on the
embedded font's metrics, and the interaction between `line-height` here and the
body's `line-height: 1.8` is what decides how many lines the cap actually spans.
Both need a device to measure. Keeping the three together and commented makes
the post-test adjustment a one-line edit.

Scoping falls out of the markup: `.dropcap` only ever appears in chapter
documents, because only `chapter_xhtml` emits it. No `body` class or document
level selector is needed to keep the title page clean.

## Risks / Trade-offs

- **A target renderer ignores `float` on an inline span.** Mitigation: an
  oversized letter sits inline and opens up the first line, which task 5.4's
  device pass catches on the device that shows it, before the change is
  archived. If one renderer is the sole failure, the options are to accept it or
  to drop the feature; there is no third recipe with better coverage.

- **Stacked Vietnamese diacritics clip into the chapter heading.** `Ế`, `Ộ`, and
  `Ằ` extend well above the cap height that `line-height: 0.8` reserves.
  Mitigation: the `line-height` knob exists for exactly this. A chapter whose
  first letter is unaccented will not reveal the problem, so the device pass
  must use a chapter that opens with an accented capital.

- **`body_html` turns out to hold `&quot;` rather than a literal quote.**
  `scraper`'s serializer does not re-escape quotes in text nodes, so a literal
  is expected, but this is unverified. Mitigation: both forms are covered by the
  skip guard and both are tested in task 1.1, so the outcome does not change
  behavior either way.

- **A future source adapter emits a different body shape,** for example a
  leading `<div>` wrapper or inline markup before the first text. Mitigation:
  the guard fails to match and the chapter renders without a drop cap.
  Degradation is silent, which is the intended direction, but it means a new
  adapter should be eyeballed once rather than assumed to inherit the feature.

- **Behavior is unconditional, so a reader who dislikes drop caps has no
  escape.** Mitigation: accepted deliberately, see `proposal.md`. Rebuilding an
  EPUB from already-downloaded chapters is a seconds-long local operation, so
  reverting the decision later costs a rebuild, not a re-crawl.

## Migration Plan

None. The change alters only the contents of newly generated EPUB files. No
persisted state, no public API signature, and no CLI flag changes, so there is
nothing to migrate and nothing to roll back beyond reverting the commit.
Previously built EPUBs are unaffected and can be regenerated with
`--epub-only --chapter-dir <dir>` if the new styling is wanted.
