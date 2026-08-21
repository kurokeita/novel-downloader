## Module ownership

New directory `src/source/xtruyen/`, following the split the two existing
adapters use:

| File | Role |
| --- | --- |
| `mod.rs` | the adapter struct and its `SiteAdapter` impl, plus `rate_policy` and its measurement record, and the shared `fetch_page` status mapping |
| `payload.rs` | recovering chapter prose from the encoded inline script: alphabet extraction, character mapping, base64, inflate, paragraph split |
| `discovery.rs` | the chapter index walk, plus the chapter-page parsers it needs (window, forward link, chapter label) |
| `metadata.rs` | novel-page extractors (title, author, status, description, cover, first and latest chapter links) |
| `parser.rs` | pure URL helpers: novel-slug extraction and `rebase_onto` |

`parser.rs` was not in the first draft of this table, which listed four files.
It exists for the same reason `khodocsach/parser.rs` does: the URL helpers are
pure, are needed by both `mod.rs` and `discovery.rs`, and belong with neither.

No new module outside `src/source/`. The wizard and CLI work lands in files
that already own those concerns: `src/ui/wizard/state.rs` for the step
decision, `src/ui/wizard/steps.rs` for the prompts, `src/ui/plan.rs` for
the summary, `src/bin/novel-downloader.rs` for the CLI notice.

## Decision: recover the payload, do not drive a browser

The prose sits in an inline script as a single encoded string. Recovery is
three cheap steps: map each character through a custom 64-symbol alphabet
onto the standard base64 alphabet, base64-decode, zlib-inflate. Both
alphabets are present in the page as plain 64-character string literals
inside an obfuscated array.

The adapter SHALL read both alphabets out of the page it was served and
fall back to compiled-in constants only when the page does not carry them.
Hard-coding alone would break silently on a site redeploy that shuffles the
alphabet, and a compiled-in fallback alone leaves no recovery path; reading
first and falling back covers both. A failed decode is a chapter failure,
never an empty chapter, so a change in the scheme surfaces as failed
chapters with a decode error rather than as an EPUB of blank pages.

Dependencies: `base64` and `flate2` are both already compiled into this
build (`base64 v0.22.1` through `reqwest`, `flate2 v1.1.9` through `zip`'s
`deflate` feature), so declaring them directly in `[dependencies]` adds no
new code to the build. No headless browser, and no JavaScript engine.

Paragraph splitting happens on doubled `<br>`, matching how the site's own
script normalizes the text. The recovered markup contains only `<br>` and
`<div>`; tags are stripped and entities decoded through the same helpers
the other adapters use, so this adapter introduces no second HTML-to-text
path.

## Decision: walk the chapter windows, chained by the next-chapter link

Rejected: synthesizing `1..=N`. Proposal point 2 shows this drops real
chapters.

Rejected: the theme's chapter-list endpoint (`POST
/truyen/<slug>/ajax/chapters/`). It answers with the group headers only
(`1-to-100`, `101-to-200`, and so on) and empty child lists, so it yields
boundaries but never slugs. Using it would mean parsing a second document
for numbers the walk below already produces.

Chosen: every chapter page carries, in its chapter-select control, the
complete slug list of the hundred-chapter window that chapter belongs to.
Verified on the reference novel: chapters 1 and 100 both return the window
`chuong-1 .. chuong-100`, chapter 218 returns `chuong-201 .. chuong-218`.
Each chapter page also carries previous and next chapter links; chapter 100
points forward to chapter 101, the final chapter has no next link, and on
`nguyet-nguyet-luan-hoi` chapter 1 points forward to `chuong-1-1`, which is
exactly the case a numeric guess would miss.

The walk is therefore:

1. Fetch the novel main page once. It yields the metadata and both the
   first and the latest chapter link.
2. Fetch the first chapter page. Its select gives that whole window's
   slugs, in order.
3. Fetch the window's last slug and follow its next link to the first slug
   of the following window. Repeat step 2 from there.
4. Stop when a window contains the latest slug from the main page, or when
   a page has no next link.

Cost is roughly two requests per hundred chapters, so a 4,800-chapter novel
indexes in about 96 requests, which at the policy's pace is under a minute.

Loop safety: the walk SHALL stop if a window yields no slug it has not
already seen, and SHALL fail rather than return a partial index if a page
in the middle of the walk cannot be fetched. A truncated index that looks
like a short novel is the failure mode this whole decision exists to avoid.

## Decision: `ChapterRef.number` is the index position, not the parsed slug

A suffixed address is an **extension chapter**: `chuong-1-1` and
`chuong-1-2` continue `chuong-1` rather than duplicating it. All three are
real chapters with their own page, their own label and their own stop in the
site's reading order, so all three are downloaded and none is merged into
another. Merging them into one output chapter was considered and rejected:
the site presents them as separate reading units, `chuong-1-2` carries a
title of its own rather than a continuation of the previous one, and merging
would complicate resume for no reader benefit.

That is also why `ChapterRef.number` cannot be the parsed slug number.
The number drives the `chapter_NNNN.html` file name, and `chuong-1`,
`chuong-1-1` and `chuong-1-2` all parse to 1, so parsing would write three
chapters into one file and lose two of them.

The number is therefore the chapter's 1-based position in the walked index.
Position comes from the site's own sequence, never from sorting parsed
numbers, which is what puts an extension chapter directly after the chapter
it extends. Verified on `nguyet-nguyet-luan-hoi`: the next link on
`chuong-1` points at `chuong-1-1`.

Consequences, all accepted:

- `--start` and `--end` select by position rather than by the number printed
  on the site. On a novel with extension chapters the two drift apart, so
  `--end 100` can stop before the chapter the site labels 100. They coincide
  on the roughly 95% of novels whose addresses are plain numbers.
- If the site inserts a chapter mid-novel, positions after the insertion
  shift, so a later resume writes different file names than the first run.
  This is the price of unique names, and it is preferable to two chapters
  overwriting one file.

`ChapterRef.title` stays `None` from the index. The chapter title is read
from the chapter page on fetch, as metruyenhot already does, so the EPUB
table of contents shows the site's own label (for example `Chương 1 1:
...`) even where the file name is positional.

## Decision: rate policy numbers, and where they come from

```
max_concurrency: 2
min_delay:       500ms
backoff_base:    2s
max_retries:     3
```

The `rate_policy` doc comment SHALL record the measurements from the
proposal, including the negative result: rotating the `User-Agent` per
request does not move the limit, so the khodocsach approach of trading a
rotating header for an unconstrained policy is not available here. What
would invalidate these numbers is a change in how the site's edge buckets
clients, which the next person can retest the same way: a fixed-rate
sequential run at 2 and at 4 requests per second, and a small parallel
burst.

## Decision: lock the pacing prompts instead of clamping behind the user's back

The runner already clamps concurrency to `policy.max_concurrency` and
spaces requests by `policy.min_delay`. Nothing about enforcement changes.
What changes is that the wizard stops asking for values it will override.

`RatePolicy` gains a predicate for "this source fixes the pacing", true
when `max_concurrency` is below `usize::MAX` or `min_delay` is above zero.
Both existing sources are unconstrained on both counts, so they are
unaffected without either being named. No adapter list, no per-host branch.

Wizard routing follows the `step_after_mode` precedent exactly: a pure,
crate-internal function in `src/ui/wizard/state.rs` decides the step after
the end-chapter prompt, returning the worker prompt when pacing is free and
the existing-file prompt when it is fixed. The back target of the
existing-file prompt becomes the end-chapter prompt in the locked case. The
function is pure, so inline `#[cfg(test)]` tests cover it without a
terminal, which is the only way this requirement is testable at all.

Two details of this settled differently once the code was written, and the
build follows the code rather than this paragraph's first draft.

The pacing is **not** cached on `WizardState`. Caching was proposed by analogy
with `recent_fonts`, but that cache exists because it stats the filesystem;
resolving a host to an adapter is a string comparison against a static slice
with no I/O at all. `WizardState::rate_policy` therefore reads it where it is
needed, which also removes the invalidation the cache would have needed when
the URL changes.

There is likewise **no separate note screen**. The enforced numbers are stated
on the confirmation summary instead, as `Workers: 2 (required by xtruyen)` and
the same suffix on the delay line. That satisfies the same requirement with a
surface the user already reads, and avoids an extra screen on every pass
through the step, including back-navigation. The wizard writes the enforced
values into the run's own state at the end of the end-chapter step, so the
summary, the plan and the runner cannot disagree, and `build_summary` needs
only one added flag rather than a reshaped parameter list.

Reporting the pacing is the same defect class the archived
`skip-output-root-prompt-in-epub-only` change fixed for paths: the summary must
not name a value the run will not use. `SummaryParams` is public and this
project keeps no compatibility shims, so it gains a field and the compiler
pushes the call sites.

For the non-interactive CLI the flags stay accepted and are reported when
overridden, rather than becoming a hard error. A rejected `--workers 8`
would break existing invocations for a value the runner can satisfy safely
by clamping, and scripted callers should not have to know each site's
ceiling. The delay floor is currently applied with no message at all, which
is the half of this that is a plain reporting gap.
