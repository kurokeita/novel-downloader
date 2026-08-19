# download-progress Specification

## Purpose
Defines how a chapter download run reports its own progress to the interactive
screen: the chapter tally, how long the run has been going, and how much time
is left, including when a remaining-time estimate is honest enough to show and
when it must be withheld instead of guessed.
## Requirements
### Requirement: Elapsed run time is reported from the start of the run

The download progress state SHALL report the wall-clock time elapsed since the
run began, and SHALL do so from the moment the run starts rather than from the
first chapter event. Durations SHALL be rendered as `HH:MM:SS`, matching the
non-interactive progress bar, with no shorter form for runs under an hour.

#### Scenario: Elapsed time is available before any chapter finishes

- **WHEN** a run has started and no chapter has completed or failed yet
- **THEN** the reported elapsed time is the time since the run began

#### Scenario: Elapsed time advances with the clock

- **WHEN** elapsed time is read at two instants during an unfinished run
- **THEN** the later read reports the larger duration, and the difference equals
  the interval between the two instants

#### Scenario: Durations render with an hour field

- **WHEN** a duration of four minutes and twelve seconds is rendered
- **THEN** the result is `00:04:12`

- **WHEN** a duration of one hour, nine minutes and six seconds is rendered
- **THEN** the result is `01:09:06`

### Requirement: Elapsed run time freezes once the run ends

When a run reaches its end, the reported elapsed time SHALL stop advancing and
SHALL keep reporting the duration measured at the moment the run ended, so the
final screen shows how long the run actually took rather than how long the user
has been looking at it. This SHALL hold whether the run ended by finishing all
its chapters or by being aborted, since both paths mark the run finished.

#### Scenario: Elapsed time stops at the end of the run

- **WHEN** a run is marked finished at a given instant, and elapsed time is read
  at a later instant
- **THEN** the reported elapsed time is the duration from the run's start to the
  instant it was marked finished, not to the later read

#### Scenario: An aborted run freezes its elapsed time too

- **WHEN** a run is aborted before all chapters are accounted for and is
  therefore marked finished
- **THEN** the reported elapsed time is frozen at the abort instant

### Requirement: The remaining-time estimate reflects recent throughput

The remaining-time estimate SHALL be derived from a bounded window of the most
recent terminal chapter events (completions and failures) and SHALL NOT be
derived from a whole-run average. A run whose throughput changes SHALL converge
on the new throughput once the window no longer holds events from the old one.

This exists because two ordinary run shapes advance the chapter tally in an
instant burst that says nothing about the work left: resuming a run where most
chapters are already on disk and are skipped without a network request, and the
opening moments of a parallel run where the first batch of workers all report
together.

The estimate SHALL be independent of the order in which concurrent workers
report, so a parallel run and a sequential run at the same throughput produce
the same estimate.

#### Scenario: Steady throughput produces a proportional estimate

- **WHEN** chapters have been completing at a steady rate and chapters remain
- **THEN** the estimate is the remaining chapter count divided by that rate

#### Scenario: Estimate recovers after a burst of skipped chapters

- **WHEN** a large number of chapters complete near-instantly, and then further
  chapters complete at a slow steady rate until the burst's events have been
  pushed out of the window
- **THEN** the estimate reflects the slow rate rather than the burst rate

#### Scenario: A run with no chapters has no estimate

- **WHEN** the run's total chapter count is zero
- **THEN** no estimate is reported

### Requirement: The estimate is withheld when the samples cannot support it

Rather than report a figure the samples do not justify, the system SHALL withhold
the remaining-time estimate entirely when any of the following holds:

- Fewer than two terminal chapter events have been observed.
- The observed events span less than a minimum wall-clock interval of two
  seconds, measured as described in the stall requirement below.
- The run has ended.

A withheld estimate SHALL be shown on screen as a placeholder rather than by
removing the estimate from the display, so the label does not change width from
one redraw to the next while a run is in flight.

#### Scenario: A single completion is not enough to estimate from

- **WHEN** exactly one chapter has completed
- **THEN** no estimate is reported

#### Scenario: An instantaneous burst is not enough to estimate from

- **WHEN** many chapters complete within a few milliseconds of one another and
  less than two seconds of wall-clock time has passed since the oldest of them
- **THEN** no estimate is reported, rather than an estimate near zero

#### Scenario: A finished run reports no estimate

- **WHEN** the run has been marked finished
- **THEN** no estimate is reported, while elapsed time is still reported

### Requirement: A stalled run's estimate grows instead of going stale

While no chapter events are arriving, for example while the run is backing off
from a source that has rate-limited it, the estimate SHALL grow to account for
the time spent waiting, rather than continuing to report the figure that was
current when the last chapter arrived. The interval used to measure throughput
SHALL therefore extend to the present whenever the present is later than the
most recent event.

#### Scenario: Estimate increases while nothing completes

- **WHEN** the estimate is read, then read again later with no chapter events in
  between
- **THEN** the later read reports a larger remaining-time estimate

### Requirement: The download screen shows both times alongside the tally

The interactive download screen SHALL display elapsed time, and the
remaining-time estimate or its placeholder, together with the existing chapter
tally and percentage. Adding them SHALL NOT change the screen's row layout.

Once the run has ended, the screen SHALL show the frozen elapsed time and SHALL
drop the estimate rather than show its placeholder.

#### Scenario: An in-flight run shows the tally, elapsed time, and estimate

- **WHEN** a run is in flight with an estimate available
- **THEN** the progress display shows the completed and total chapter counts, the
  percentage, the elapsed time, and the estimate

#### Scenario: An in-flight run without an estimate shows the placeholder

- **WHEN** a run is in flight and the estimate is withheld
- **THEN** the progress display shows the elapsed time and a placeholder in place
  of the estimate

#### Scenario: A finished run shows only the frozen elapsed time

- **WHEN** a run has ended
- **THEN** the progress display shows the final tally and the frozen elapsed
  time, and shows no estimate and no placeholder

