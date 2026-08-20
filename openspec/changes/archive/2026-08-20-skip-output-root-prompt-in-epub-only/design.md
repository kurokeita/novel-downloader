## Context

See proposal.md for motivation. Three facts about the current code shape the
approach:

- The per-mode branch already exists, one step too late. `step_mode` sends
  every mode to the output-root prompt, and `step_output_root` is where the
  mode is first consulted. Moving the branch earlier is the whole behavior
  change.
- Nothing in `tests/` drives any wizard step. Every step function calls a
  prompt that needs a real terminal, so three of the four requirements in
  `specs/wizard-prompt-flow/spec.md` describe behavior no current test can
  reach. `build_summary` is the exception: it is pure and already covered in
  `tests/ui.rs`.
- The EPUB destination rule is already written down twice over. The binary's
  `infer_chapter_dir` joins the output root with the slugified title, and
  `build_epub` defaults the output file into whichever chapter directory it is
  handed. A summary that names the destination becomes a third copy unless
  something is shared.

## Goals / Non-Goals

**Goals:**

- Make the prompt-order requirements verifiable by test rather than by eye.
- Leave exactly one place in the codebase that decides where an EPUB is
  written.
- Keep the change inside the modules that already own these concerns.

**Non-Goals:**

- A general transition table for the whole wizard. Only the mode decision needs
  extracting; every other back target is already a constant or already
  mode-aware.
- Widening the public library API for the sake of tests. `WizardStep` and the
  step functions stay crate-internal.
- Making the download screen or any other prompt testable. The terminal-driving
  layer stays untested, as it is today.

## Decisions

### The mode decision becomes a pure function in `ui::wizard::state`

`state.rs` already owns `WizardStep` and `WizardState`, so the decision "which
step follows the mode select" belongs there rather than in `steps.rs`, which
exists to drive prompts. `step_mode` then calls it instead of hard-coding the
next step, and `step_output_root` loses the branch it holds today, since by
then only crawl modes can reach it.

Tested with an inline `#[cfg(test)] mod tests`, following the precedent already
set in `source/khodocsach/api.rs`, `source/khodocsach/parser.rs`, and
`source/metruyenhot/metadata.rs`. That keeps the function private while still
covering it, which an integration test in `tests/` could not do without making
the wizard's internals public.

Alternative considered: make `WizardStep` and the transition function public so
`tests/ui.rs` can drive them. Rejected because it exports wizard internals as
library API purely to satisfy a test, and the project has an established inline
pattern for exactly this case.

Alternative considered: leave the branch inline in `step_mode` and verify by
eye. Rejected because it leaves the central requirement of this change with no
regression guard, and the branch is the one thing most likely to be undone by a
later edit to the step order.

### The "no directory is created" scenario is covered by construction

`create_dir_all` is called in exactly one place, inside `step_output_root`. Once
the transition test proves build-only mode routes to the chapter-directory step,
the absence of the directory follows: the only code that creates it is never
reached. The task list should assert that single call site rather than attempt
to observe the filesystem from a test that cannot run the wizard.

### One shared helper owns the EPUB destination, and `ui::plan` owns it

A new pure function in `ui::plan` derives the directory an EPUB will be written
to from the mode, the output root, the chapter directory, and the book title.
`build_summary` uses it to render the line, and the binary's `infer_chapter_dir`
delegates to it instead of repeating the join. The summary then cannot disagree
with the run, which is the requirement's actual point.

`ui::plan` owns it because the rule needs `CrawlMode`, which lives there, and
because the binary already imports from that module. `utils` was the other
candidate and is where `slugify` lives, but `utils` sits below the UI layer and
would have to import `CrawlMode` upward to host this.

Alternative considered: have `build_summary` compute the path itself and leave
`infer_chapter_dir` alone. Rejected: two copies of a path rule that must agree
is exactly the defect the summary requirement exists to prevent.

### The summary drops the output root in build-only mode

Under this change the wizard never collects an output root in build-only mode,
so the value on the plan is whatever the CLI default left there. Printing it
would report a directory the user never chose and the run never touches. The
line is therefore omitted for that mode, and the new EPUB-destination line
replaces it as the useful information.

`SummaryParams` needs no new field: it already carries `novel_title`, which is
the only input the destination rule was missing. The proposal's note about a
possibly breaking API change is resolved as not breaking.

## Risks / Trade-offs

- The prompt sequence is still only partly guarded: the transition function is
  tested, but that the step functions actually consult it is not → mitigated by
  keeping the branch out of `steps.rs` entirely, so a step function has no
  decision left to get wrong.
- `build_summary`'s existing assertions in `tests/ui.rs` will need updating for
  the new line → expected, and the updated assertions are what pin the new
  behavior.
- Sharing the destination helper couples `ui::plan` and the binary a little more
  tightly → accepted; they are already coupled through `InteractivePlan`, and
  the alternative is a duplicated path rule.
- In crawl-and-build mode the summary names a directory derived from a title the
  user can still edit at a later prompt → the title prompt precedes the
  confirmation screen, so the value shown is the final one.

## Migration Plan

None. No data, config, or on-disk layout changes. Existing EPUBs and chapter
directories are unaffected, and the non-interactive CLI keeps its current
behavior, including `--output-root` remaining the inference source when
`--chapter-dir` is omitted.

## Open Questions

- Exact label for the new summary line (for example `EPUB output:` versus
  spelling out the destination). Wording only, settled when the line is
  written; it changes no requirement, no approach, and no task.
