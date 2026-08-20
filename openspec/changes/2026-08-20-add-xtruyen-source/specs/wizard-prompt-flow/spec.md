## Purpose

Governs which prompts the interactive wizard asks for each operating mode,
where back-navigation lands, and what the confirmation summary reports about
the run, so that a user is never asked for a value the run will ignore and
never told a value the run will not use.

## ADDED Requirements

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
