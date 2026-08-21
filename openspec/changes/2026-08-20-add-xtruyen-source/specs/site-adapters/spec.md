## Purpose

Defines the contract every source site must satisfy so that one download
pipeline, one EPUB builder and one terminal UI can serve any number of novel
sites, and defines how the application picks the right source for a given URL
without asking the user.

## ADDED Requirements

### Requirement: Pacing enforced on the user's behalf is reported, not silent

When the resolved source's rate policy overrides pacing the user asked for,
the application SHALL say so, naming the value in force. This holds for both
surfaces and for both values: the concurrency ceiling and the minimum delay
between requests.

The application SHALL NOT reject an invocation for asking to go faster than
the source allows, since the pipeline can satisfy the request safely by
slowing it down, and a caller should not need to know each site's limits in
advance.

#### Scenario: A reduced worker count is reported

- **WHEN** the user asks for more concurrent workers than the resolved source
  allows
- **THEN** the run proceeds at the source's maximum
- **AND** the user is told the worker count was reduced, and to what

#### Scenario: A raised delay is reported

- **WHEN** the user asks for a shorter delay between requests than the
  resolved source allows
- **THEN** the run proceeds at the source's minimum delay
- **AND** the user is told the delay was raised, and to what

#### Scenario: Pacing within the policy is left alone

- **WHEN** the user asks for a worker count and delay that the resolved
  source's policy permits
- **THEN** the run uses exactly those values
- **AND** no override message is shown

### Requirement: A wait the site states is honored over a computed backoff

When a site refuses a request for exceeding its rate limit and states how long
the client should wait, the pipeline SHALL wait at least that long before
retrying, in preference to any interval it would otherwise compute.

Retrying sooner than the site asked earns a second refusal and consumes a retry
for nothing, so a stated wait SHALL be carried from the source that observed it
to whatever code performs the wait. Where a site states nothing, the source's
own backoff applies unchanged.

#### Scenario: A stated wait is obeyed

- **WHEN** a source reports a rate-limit refusal that carries the wait the site
  asked for
- **THEN** the retry happens no earlier than that wait allows

#### Scenario: No stated wait leaves existing backoff untouched

- **WHEN** a source reports a rate-limit refusal without a stated wait
- **THEN** the pipeline backs off by its own growing interval as before

### Requirement: A source may not be given a rate policy it cannot justify

A source's declared rate policy SHALL be accompanied, in the source itself, by
a record of how its numbers were measured and what observation would
invalidate them. A policy that names limits without recording their basis
cannot be revisited safely by the next person to touch it.

#### Scenario: Every registered source records the basis for its policy

- **WHEN** a source declares its rate policy
- **THEN** the declaration is accompanied by the measurements behind it,
  including any technique that was tried and did not widen the limit
