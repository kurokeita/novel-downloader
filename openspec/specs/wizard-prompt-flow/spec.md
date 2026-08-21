## Purpose

Governs which prompts the interactive wizard asks for each operating mode,
where back-navigation lands, and what the confirmation summary reports about
where the run's files will be written, so that a user is never asked for a
value the chosen mode ignores and never told a destination the run will not
use.

## Requirements

### Requirement: Build-only mode does not ask for an output root

When the user selects the mode that builds an EPUB from chapter files already
on disk, the wizard SHALL NOT prompt for an output root, because that mode
derives every path from the chapter directory the user names next.

The wizard SHALL NOT create any directory on behalf of a prompt it does not
show, so choosing build-only mode leaves the filesystem untouched until the
run itself writes output.

#### Scenario: Mode select leads straight to the chapter directory

- **WHEN** the user selects the build-only mode
- **THEN** the next prompt asks for the directory holding the existing chapter
  files
- **AND** no output-root prompt is shown

#### Scenario: No directory is created for the skipped prompt

- **WHEN** the user selects the build-only mode and reaches the chapter
  directory prompt
- **THEN** no output-root directory has been created

### Requirement: Crawl modes still ask for an output root

When the user selects a mode that downloads chapters, the wizard SHALL prompt
for an output root, because those modes write chapters, and any EPUB, beneath
it.

#### Scenario: Download modes keep the prompt

- **WHEN** the user selects the crawl mode or the crawl-and-build mode
- **THEN** the wizard asks for an output root before it discovers the novel

### Requirement: Back-navigation returns to the previous prompt shown

Going back from a prompt SHALL return to the prompt the user last answered,
never to one the current mode skipped, so that back-navigation is reversible
in every mode.

#### Scenario: Back from the chapter directory reaches the mode select

- **WHEN** the user is at the chapter directory prompt in build-only mode
- **AND** the user goes back
- **THEN** the mode select is shown

### Requirement: The confirmation summary names where the EPUB will be written

When the plan builds an EPUB, the confirmation summary SHALL name the
directory that will receive the file, and that directory SHALL be the one the
run actually writes to, so the user can confirm the destination before the
run starts.

When the plan builds no EPUB, the summary SHALL NOT report an EPUB
destination.

#### Scenario: Build-only mode names the chapter directory

- **WHEN** the summary is rendered for build-only mode with a chapter
  directory of `/books/my-novel`
- **THEN** the summary reports `/books/my-novel` as the EPUB destination

#### Scenario: Crawl-and-build mode names the directory holding the chapters

- **WHEN** the summary is rendered for crawl-and-build mode with an output
  root and a known book title
- **THEN** the summary reports the per-novel directory beneath that output
  root as the EPUB destination, matching where the chapter files will be
  written

#### Scenario: Download-only mode reports no EPUB destination

- **WHEN** the summary is rendered for the crawl mode, which builds no EPUB
- **THEN** the summary reports no EPUB destination


### Requirement: Pacing prompts are not shown when the source fixes the pacing

Some sources enforce a request rate and a concurrency ceiling that the
pipeline applies regardless of what the user asks for. When the source
resolved from the novel URL fixes either the worker count or the delay
between requests, the wizard SHALL NOT prompt for either value.

The wizard SHALL instead tell the user the worker count and delay the run
will use, and that the site requires them, so that a prompt the user has
seen on other novels does not appear to have gone missing.

Sources that constrain neither value SHALL keep both prompts exactly as they
are today.

#### Scenario: A rate-limited source skips both pacing prompts

- **WHEN** the user supplies a novel URL for a source that declares a
  concurrency ceiling or a minimum request delay
- **THEN** the wizard does not prompt for a worker count
- **AND** the wizard does not prompt for a request delay
- **AND** the user is shown the worker count and delay the run will use, and
  that the site requires them

#### Scenario: An unconstrained source keeps both pacing prompts

- **WHEN** the user supplies a novel URL for a source that declares no
  concurrency ceiling and no minimum request delay
- **THEN** the wizard prompts for a worker count and a request delay as
  before

#### Scenario: Back-navigation skips the prompts that were never shown

- **WHEN** the pacing prompts were skipped and the user navigates back from
  the prompt that follows them
- **THEN** the wizard returns to the last prompt it actually showed
- **AND** neither pacing prompt is presented on the way back

### Requirement: The confirmation summary reports the pacing the run will use

The summary shown before a run starts SHALL report the worker count and the
request delay the run will actually use, which for a source that fixes them
is the source's values rather than any value held over from a default or an
earlier novel.

#### Scenario: Summary matches the run for a rate-limited source

- **WHEN** the confirmation summary is shown for a source that fixes the
  pacing
- **THEN** the worker count and delay it reports are the ones the run
  enforces

#### Scenario: Summary matches the run for an unconstrained source

- **WHEN** the confirmation summary is shown for a source that fixes neither
  value
- **THEN** the worker count and delay it reports are the ones the user
  entered
