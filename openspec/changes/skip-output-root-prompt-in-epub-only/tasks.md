## 1. Prompt-order seam

- [ ] 1.1 Add a failing inline `#[cfg(test)] mod tests` in
      `src/ui/wizard/state.rs` asserting the step that follows the mode
      select: `CrawlMode::EpubOnly` yields `WizardStep::ChapterDir`, while
      `CrawlMode::Crawl` and `CrawlMode::CrawlEpub` both yield
      `WizardStep::OutputRoot`
- [ ] 1.2 Add the `///`-documented pure transition function in
      `src/ui/wizard/state.rs` that the test drives; keep it and
      `WizardStep` crate-internal
- [ ] 1.3 Have `step_mode` in `src/ui/wizard/steps.rs` return the step that
      function chooses instead of hard-coding `WizardStep::OutputRoot`
- [ ] 1.4 Collapse the now-dead mode branch in `step_output_root` so it
      always advances to `WizardStep::Discover`, leaving no mode decision in
      the prompt-driving layer
- [ ] 1.5 Change the back target in `step_chapter_dir` from
      `WizardStep::OutputRoot` to `WizardStep::Mode`
- [ ] 1.6 Confirm `create_dir_all` still has exactly one call site, inside
      `step_output_root`, since the "no directory is created" scenario holds
      by construction rather than by test

## 2. Shared EPUB destination rule

- [ ] 2.1 Add failing tests in `tests/ui.rs` for a public destination helper
      in `ui::plan`: build-only mode yields the chapter directory,
      crawl-and-build yields the per-novel directory beneath the output root,
      and the download-only mode yields no destination
- [ ] 2.2 Add the `///`-documented helper to `src/ui/plan.rs`, deriving the
      directory from the mode, output root, chapter directory, and book
      title, and reusing `utils::slugify` for the per-novel segment
- [ ] 2.3 Rewrite `infer_chapter_dir` in `src/bin/novel-downloader.rs` to
      delegate to the new helper so the path rule exists in one place only

## 3. Confirmation summary

- [ ] 3.1 Add failing tests in `tests/ui.rs` asserting the summary names the
      EPUB destination for build-only mode, names the per-novel directory for
      crawl-and-build, reports no destination for the download-only mode, and
      omits the `Output root` line in build-only mode
- [ ] 3.2 Emit the destination line from `build_summary` in `src/ui/plan.rs`
      via the helper from task 2.2, and drop the `Output root` line for
      build-only mode
- [ ] 3.3 Update the existing summary assertions in `tests/ui.rs`
      (`build_summary_includes_every_chosen_option_for_crawl_epub` and any
      other affected case) for the new line

## 4. Documentation

- [ ] 4.1 Update the `ui/` section of `AGENTS.md`: the wizard skips the
      output-root prompt in build-only mode, the mode-to-step decision is a
      tested pure function in `state.rs`, and `ui::plan` owns the single EPUB
      destination rule that both the summary and the binary consume

## 5. Verification

- [ ] 5.1 `cargo test`
- [ ] 5.2 `cargo clippy --all-targets` with zero warnings
- [ ] 5.3 `cargo fmt --check`
- [ ] 5.4 Eyes-on-terminal run of the TUI in build-only mode: no output-root
      prompt appears, back from the chapter directory reaches the mode
      select, and the summary names the directory the EPUB is written to
