## 1. Fixtures

- [x] 1.1 Record `tests/fixtures/xtruyen_novel.html` from the reference novel
      page, keeping the metadata block, the first and latest chapter links,
      and the cover element, and replacing the description text with invented
      placeholder prose
- [x] 1.2 Record `tests/fixtures/xtruyen_chapter.html` keeping the chapter
      label, the chapter-select window, the previous and next links, and an
      encoded payload built from **invented placeholder prose** through the
      site's own scheme (custom alphabet, then base64, then zlib), so no
      third-party novel text enters the repository
- [x] 1.3 Record `tests/fixtures/xtruyen_chapter_suffixed.html` for a window
      containing suffixed chapter addresses, which is the case the index walk
      exists for
- [x] 1.4 Note in a comment beside the fixtures how the encoded payload was
      produced, so a future edit can regenerate it

## 2. Payload decoding

- [x] 2.1 Add failing tests in `tests/source.rs` for the decoder: a payload
      built through the scheme round-trips to its paragraphs; the two
      alphabets are read from the page when present; compiled-in defaults are
      used when the page omits them
- [x] 2.2 Add failing tests for the failure paths: an absent payload and a
      payload that fails to inflate both yield an error rather than an empty
      paragraph list
- [x] 2.3 Declare `base64` and `flate2` in `[dependencies]`, both already
      present transitively so the build gains no new code
- [x] 2.4 Add the `///`-documented decoder in `src/source/xtruyen/payload.rs`
      that the tests drive, splitting paragraphs on doubled `<br>` and
      reusing the existing tag-strip and entity-decode helpers
- [x] 2.5 Add a failing test asserting advertising markup present on the page
      does not appear in the decoded paragraphs

## 3. Novel page metadata

- [x] 3.1 Add failing tests in `tests/source.rs` for the main-page extractors
      against `xtruyen_novel.html`: title, author, status, description, cover
      location, first chapter link, latest chapter link
- [x] 3.2 Add failing tests for the absent-cover and absent-author cases
- [x] 3.3 Add the `///`-documented extractors in
      `src/source/xtruyen/metadata.rs`

## 4. Chapter index walk

- [x] 4.1 Add a failing test in `tests/source.rs`, served by
      `mockito::Server::new_async`, asserting a two-window novel indexes to
      every chapter in reading order
- [x] 4.2 Add a failing test asserting extension chapters appear as their own
      entries, sit immediately after the chapter they extend, and receive
      distinct sequence numbers despite sharing a parsed number
- [x] 4.3 Add a failing test asserting sequence numbers run from 1 with no
      gaps, and that the locator is the chapter's own address
- [x] 4.4 Add a failing test asserting a page that cannot be retrieved
      mid-walk fails the whole index rather than returning a short novel
- [x] 4.5 Add a failing test asserting the walk terminates when a window
      yields no new address, so a site that links a chapter back to itself
      cannot loop forever
- [x] 4.6 Add the `///`-documented walk in `src/source/xtruyen/discovery.rs`

## 5. The adapter and its registration

- [x] 5.1 Add failing tests for URL acceptance: a novel page URL with and
      without a trailing slash both resolve; a genre listing, the site root
      and a chapter page are each rejected before any request
- [x] 5.2 Add a failing test asserting `fetch_metadata` returns metadata
      without requesting the chapter index, which is what `--epub-only`
      relies on
- [x] 5.3 Add a failing test asserting a `429` response surfaces as the
      rate-limited error variant rather than a generic failure
- [x] 5.4 Add a failing test asserting a chapter page with no chapter label
      still yields a chapter titled from its sequence number
- [x] 5.5 Add `src/source/xtruyen/mod.rs` with the `///`-documented
      `SiteAdapter` impl, deriving every request base from the URL it was
      handed rather than a compiled-in host
- [x] 5.6 Add the entry to `ADAPTERS` in `src/source/registry.rs`
- [x] 5.7 Add a failing test asserting the new host appears in the
      unsupported-host error message, since that list is generated and a
      missing entry means the adapter is not really wired in
- [x] 5.8 Write `rate_policy` with concurrency 2, a 500ms minimum delay, a 2s
      backoff base and 3 retries, and a doc comment recording the measured
      ceiling, the negative result for header rotation, and what would
      invalidate the numbers

## 6. Locking the pacing prompts

- [x] 6.1 Add a failing inline `#[cfg(test)]` test in `src/source/mod.rs` for
      the `RatePolicy` predicate: a policy with a concurrency ceiling or a
      non-zero minimum delay fixes the pacing, and the policy both existing
      sources declare does not
- [x] 6.2 Add the `///`-documented predicate that the test drives
- [x] 6.3 Add a failing inline test in `src/ui/wizard/state.rs` for the step
      that follows the end-chapter prompt: fixed pacing yields the
      existing-file prompt, free pacing yields the worker prompt
- [x] 6.4 Add the `///`-documented pure transition function alongside
      `step_after_mode`, keeping it and `WizardStep` crate-internal
- [x] 6.5 Cache the resolved pacing on `WizardState` once the URL is
      accepted, following the `recent_fonts` pattern so back-navigation does
      not re-resolve it
- [x] 6.6 Route `step_end_chapter` through the new function, and change the
      back target of `step_if_exists` to the end-chapter prompt when the
      pacing prompts were skipped
- [x] 6.7 State the enforced worker count and delay, and that the site requires
      them, on the confirmation summary rather than on a note screen of its own.
      Superseded during implementation: a note would reappear on every pass
      through the step, including back-navigation, and the summary is a surface
      the user already reads. See `design.md`
- [x] 6.8 Confirm the wizard writes the enforced values into the plan, so a
      locked run cannot carry a stale requested value into the runner

## 7. Reporting the pacing that is in force

- [x] 7.1 Add failing tests in `tests/ui.rs` for `build_summary`: a fixed
      pacing source reports the source's worker count and delay, an
      unconstrained one reports the user's
- [x] 7.2 Change `SummaryParams` and `build_summary` accordingly and let the
      compiler push the call sites, since the project keeps no compatibility
      shims
- [x] 7.3 Add a failing test in `tests/cli.rs` or `tests/runner.rs` for the
      delay override notice, matching the existing worker-clamp event
- [x] 7.4 Emit the delay override notice from the binary, and confirm no
      notice is emitted when the requested pacing is already within policy

## 8. Documentation

- [x] 8.1 Add `xtruyen.vn` to the supported-hosts list in `README.md`, with a
      note that downloads from it are paced by the site and that the wizard
      therefore does not ask for workers or delay
- [x] 8.2 Add the `source/xtruyen` bullet to the architecture section of
      `AGENTS.md`, matching the depth of the two existing entries and
      recording the payload scheme, the index walk and the positional
      sequence numbering
- [x] 8.3 Update the `AGENTS.md` note on chapter URL conventions, which
      currently states that output files are numbered from the site's own
      chapter number

## 9. Verification

- [x] 9.1 `git grep` for the new host and confirm no site-specific string
      lives outside `src/source/xtruyen/` and the registry entry
- [x] 9.2 `cargo test`
- [x] 9.3 `cargo clippy --all-targets`
- [x] 9.4 `cargo fmt --check`
- [x] 9.5 `cargo build --release`
- [ ] 9.6 One real end-to-end download of a short novel against the live
      site, since fixtures prove the parser and not the site's behavior,
      followed by opening the EPUB to confirm the prose and the table of
      contents
- [ ] 9.7 Eyes-on-terminal check of the wizard on a locked source and on an
      unconstrained one, including back-navigation past the skipped prompts,
      since the run loop is not unit-tested

## 10. Index fix, after the window walk failed on a live novel

Added after section 9 shipped. The walk truncated `vo-tan-dan-dien` to 101
chapters of 3611, because the chapter-select window is not an enumerator. See
the superseding decision in `design.md`.

- [x] 10.1 Record fixtures for the two new hops: the group listing returned by
      `POST <novel>/ajax/chapters/`, and two group payloads from
      `POST /api/api-chapters.php`, with invented chapter titles
- [x] 10.2 Add `api.rs` with the serde type for the group payload, the form body
      builder, and the endpoint's auth constants, documenting where the token
      came from and how it fails
- [x] 10.3 Add `manga_id` to the novel-page extractors, dropping the first and
      latest chapter links the walk needed
- [x] 10.4 Add failing inline tests for `parse_group_bounds`, then implement it
- [x] 10.5 Replace `walk_index` with `fetch_index`, deleting `parse_window` and
      `parse_next_href` rather than leaving them unused
- [x] 10.6 Add `post_form` to the adapter, mapping `401`/`403` to a rejected
      client so a rotated token cannot look like a short novel
- [x] 10.7 Rewrite the integration tests onto the two hops: ordering, positional
      numbering, index-supplied titles, a failed group, a refused request, an
      empty group list, and that the auth header is sent
- [x] 10.8 Correct `design.md`, the capability spec and `AGENTS.md`, which all
      asserted the window walk worked
- [ ] 10.9 Re-run the live check against `vo-tan-dan-dien` and confirm the index
      reports 3610 chapters rather than 101

## 11. Pacing the index, after discovery was refused on the spot

The group-based index from section 10 fired 36 requests in a tight loop and the
live host answered `429` on the first run. Pacing lives in `runner::Pacer`,
which wraps chapter downloads only, so nothing spaced or retried an index read.

- [x] 11.1 Page `POST /api/api-chapters.php` by chapter position, 400 at a time,
      stopping at the first short page. Positions are contiguous, so the group
      listing and `parse_group_bounds` are deleted along with their fixture
- [x] 11.2 Space index pages by the policy's `min_delay` and retry a `429`
      through a `post_form_retrying` helper
- [x] 11.3 Widen `SourceError::RateLimited` with `retry_after: Option<Duration>`
      and add `utils::retry_after` to read the header, so a site that states its
      own wait is obeyed rather than guessed at
- [x] 11.4 Have `runner::crawl_chapter_paced` prefer a stated wait over its
      computed backoff, and khodocsach populate the field from the same header,
      so the download path benefits too
- [x] 11.5 Decode HTML entities in index-supplied titles, which the live
      endpoint sends inside its JSON strings
- [x] 11.6 Set `min_delay` to 500ms. 250ms shipped first, on the index
      measurement alone, and was reverted once the download phase drew refusals:
      a run that trips the limiter and waits out each refusal finishes later
      than one paced at 500ms throughout
- [x] 11.7 Rewrite the index tests onto paging, and add coverage for a short
      page, entity decoding, and a retried `429`
- [x] 11.8 Update `design.md`, both specs, `AGENTS.md` and the README
