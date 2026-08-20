## Why

In build-only mode the wizard asks for an output root, then asks for the
directory holding the chapter HTML files, and then writes the EPUB into the
second one. The first prompt collects a value that mode never reads, creates a
directory that mode never uses, and its own copy promises the EPUB will be
saved there. The confirmation screen then lists both directories without
saying which one receives the file.

## What Changes

- The wizard SHALL skip the output-root prompt in build-only mode, going
  straight from the mode select to the chapter-directory prompt. Crawl and
  crawl-and-build modes keep the prompt unchanged.
- Back-navigation from the chapter-directory prompt SHALL return to the mode
  select rather than to a prompt that was never shown.
- The confirmation summary SHALL name the directory the EPUB will be written
  to, so the reported destination matches the file's actual location. The
  exact rendering for the crawl-and-build mode is a design question: the
  summary receives the output root but not the novel slug that the EPUB path
  is built from, so it cannot name that directory today.
- The output-root prompt keeps its current copy, which stays accurate for the
  two modes that still show it: chapters land in `<output root>/<slug>/` and
  the EPUB is written beside them.

Deliberately unchanged, both surfaced while exploring and both out of scope
here:

- `step_chapter_dir` accepts an empty submit as an empty path, which surfaces
  later as a `Chapter directory not found:` error naming no directory.
- The non-interactive `--epub-only --output-root X --chapter-dir D` case,
  where `--output-root` is silently unused. It remains the inference source
  when `--chapter-dir` is omitted.

Not breaking for the CLI: no flag is renamed or removed. Possibly breaking for
the library API if the summary needs a new input, since `SummaryParams` is
public and the project carries no compat shims. Design phase decides.

## Capabilities

### New Capabilities

- `wizard-prompt-flow`: which prompts the interactive wizard asks for each
  operating mode, where back-navigation lands, and what the confirmation
  summary reports about the run's destinations.

### Modified Capabilities

None. The EPUB writer, the runner, and the CLI keep their current behavior.
The invariant that the EPUB is written into the directory holding its chapter
files is preserved in every mode, which is why the prompt is being removed
rather than made to relocate the file.

## Impact

Surfaces affected: **TUI wizard steps** and, depending on the design decision
above, one **library API** struct.

- `src/ui/wizard/steps.rs`: the mode select gains the branch that
  `step_output_root` holds today; `step_output_root` loses its now-dead branch;
  `step_chapter_dir` gets a new back target.
- `src/ui/plan.rs`: `build_summary` reports the EPUB destination.
- `src/ui/wizard/state.rs`: `WizardState.output_root` stays. Build-only runs
  leave it at its default, and the CLI still needs the field.
- No change to `src/epub/`, `src/runner.rs`, `src/cli.rs`, or the binary.

Test-coverage note that the specs and tasks phases both depend on: no test in
`tests/` references `WizardStep` or any step function, because those functions
drive a real terminal. `build_summary` is a pure function and is covered in
`tests/ui.rs`. Making the prompt-order requirements verifiable therefore means
extracting the per-mode transition decision into a pure function that
`tests/ui.rs` can drive, in the same spirit as the existing `widgets/` state
machines. Without that seam the central requirement of this change can only be
checked by eye on a terminal.
