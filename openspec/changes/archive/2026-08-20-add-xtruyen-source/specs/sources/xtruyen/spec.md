## Purpose

Defines how the application obtains novels from `xtruyen.vn`, a site that
serves scrapeable pages for everything except the chapter text itself, which
travels in the page as an encoded payload, that addresses some chapters with
no distinct number of their own, and that enforces a request limit per
client address which no request header can widen.

## ADDED Requirements

### Requirement: Novel URLs on the xtruyen host are accepted

The source SHALL register the `xtruyen.vn` host and SHALL accept a novel URL
of the form the site publishes for a novel page. The source SHALL derive the
novel's slug from the URL path, and SHALL accept the URL whether or not it
carries a trailing slash, because the site answers the form without one with
a redirect.

#### Scenario: Novel page URL is accepted

- **WHEN** the user supplies an xtruyen novel page URL
- **THEN** the source accepts it and derives the novel slug from the path

#### Scenario: Trailing slash is not required

- **WHEN** the user supplies the same novel URL with and without a trailing
  slash
- **THEN** both resolve to the same novel and the same chapter index

#### Scenario: An xtruyen URL that is not a novel page is rejected

- **WHEN** the user supplies an xtruyen URL that does not identify a novel
  (for example a genre listing, the site root, or a chapter page)
- **THEN** the source rejects it with an error stating that a novel page URL
  is required
- **AND** no network request is made

### Requirement: The chapter index is read from the site, never synthesized

The source SHALL obtain the chapter index by reading the addresses the site
publishes. It SHALL NOT construct chapter addresses by combining the novel
URL with a range of numbers, because a chapter may be published as an
extension of an earlier one, addressed with a suffix beyond that chapter's
number, and would be omitted entirely.

The index SHALL be complete and in the site's own reading order before the
first chapter is downloaded. Order SHALL be taken from the sequence the site
publishes, never from sorting numbers parsed out of chapter addresses.

#### Scenario: Extension chapters are included as chapters of their own

- **WHEN** a novel contains a chapter published as an extension of an earlier
  one, sharing that chapter's leading number and differing by a suffix
- **THEN** it appears in the index as its own entry
- **AND** it is downloaded and stored as its own chapter, neither skipped nor
  merged into the chapter it extends

#### Scenario: An extension chapter keeps its place in reading order

- **WHEN** the index contains a chapter and the extensions of that chapter
- **THEN** each extension appears immediately after the chapter it extends,
  before the chapter the site places next

#### Scenario: Index covers the whole novel

- **WHEN** a novel is indexed
- **THEN** the index runs from the novel's first chapter through the latest
  chapter the novel page advertises

#### Scenario: An unreachable page aborts indexing

- **WHEN** a page needed to continue reading the index cannot be retrieved
- **THEN** indexing fails with an error
- **AND** the run does not proceed with a partial index

#### Scenario: Indexing terminates on a novel that advertises no more chapters

- **WHEN** the index reaches the novel's final chapter
- **THEN** indexing stops
- **AND** no chapter appears in the index twice

### Requirement: Chapter sequence numbers are unique within a novel

Each index entry SHALL carry a sequence number that is unique within the
novel, because the sequence number names the file the chapter is written to
and two chapters sharing one would overwrite each other.

The number SHALL be the entry's position in the index rather than a number
parsed out of the chapter's address, since a chapter and its extensions all
parse to the same number while being separate chapters.

A consequence the user can observe: on a novel containing extension chapters
the position and the number the site prints on a chapter drift apart, so a
requested chapter range selects by position.

#### Scenario: Chapters sharing a parsed number get distinct numbers

- **WHEN** a novel contains several chapters whose addresses parse to the
  same number
- **THEN** each receives a different sequence number
- **AND** each is written to its own file

#### Scenario: Numbering is contiguous from one

- **WHEN** a novel is indexed
- **THEN** the sequence numbers run from 1 upward with no gaps

### Requirement: Chapter prose is recovered from the encoded page payload

The chapter page does not serve the prose as readable markup. The source
SHALL recover the text from the encoded payload the page carries, and SHALL
normalize it into an ordered list of paragraphs like every other source.

The source SHALL take the decoding parameters from the page it was served
when the page supplies them, falling back to compiled-in defaults only when
it does not, so that a change to those parameters on the site does not
require a new release to diagnose.

#### Scenario: Encoded payload becomes ordered paragraphs

- **WHEN** a chapter page is fetched
- **THEN** the chapter's paragraphs are recovered from its encoded payload in
  reading order

#### Scenario: Advertising markup does not reach the chapter text

- **WHEN** a chapter page carries advertising or promotional markup
- **THEN** none of it appears in the stored chapter

#### Scenario: An undecodable payload is a failure, not an empty chapter

- **WHEN** a chapter's payload is absent, or cannot be decoded
- **THEN** the chapter is reported as failed with a decoding error
- **AND** no chapter file is written for it

### Requirement: Chapter titles come from the index

The chapter index carries each chapter's title, so the source SHALL take titles
from it rather than fetching a page to learn one. Where a chapter's title is
absent from the index, the source SHALL fall back to the label on the chapter's
own page, and failing that to the chapter's sequence number, so that every
stored chapter has a title.

#### Scenario: The index supplies the title

- **WHEN** a novel is indexed
- **THEN** each entry carries the chapter's title as the site publishes it
- **AND** no chapter page is fetched in order to obtain it

#### Scenario: A chapter with no title in the index still gets one

- **WHEN** a chapter's title is absent from the index
- **THEN** the chapter is still stored, titled from its own page's label, or
  from its sequence number when the page carries no label either

### Requirement: A refused index request is reported, never treated as a short novel

Reading the index requires the source to identify itself to the site in the way
the site's own reader does. If that identification stops being accepted, the
source SHALL report the refusal.

It SHALL NOT fall back to any means of guessing the index, because a guess that
silently omits chapters is worse than a run that stops and says why.

#### Scenario: A rejected index request surfaces as a rejection

- **WHEN** the site refuses an index request because it does not accept how the
  source identified itself
- **THEN** the run fails with an error stating the request was rejected
- **AND** no partial index is returned

### Requirement: Novel metadata comes from the novel page

The source SHALL read the novel's title, author, publication status,
description and cover image location from the novel page, in the single fetch
that also begins the index walk.

#### Scenario: Metadata is read from the novel page

- **WHEN** a novel URL is resolved
- **THEN** the title, author, status, description and cover location come
  from that page

#### Scenario: Metadata is available without the chapter index

- **WHEN** metadata alone is requested, as when packaging chapter files
  already on disk
- **THEN** the source returns it without reading the chapter index

#### Scenario: An absent cover does not fail the run

- **WHEN** the novel page advertises no cover image, or advertises one that
  cannot be retrieved
- **THEN** the download completes and an EPUB is still produced without a
  cover

### Requirement: The source declares the pacing the site enforces

The site refuses requests beyond a sustained rate and beyond a small number
of concurrent requests, and it applies the limit per client address rather
than per request header, so the limit cannot be widened by varying headers.

The source SHALL declare a maximum concurrency and a minimum inter-request
delay that keep a run inside the observed limit. Its declaration SHALL be
accompanied by a record of how the limit was measured and what would
invalidate the numbers.

#### Scenario: A refusal is reported as rate limiting

- **WHEN** the site refuses a request for exceeding its rate limit
- **THEN** the source reports it as rate limiting rather than as a generic
  failure
- **AND** the pipeline backs off and retries rather than failing the chapter
  outright

#### Scenario: Sustained downloading stays within the limit

- **WHEN** a novel of many chapters is downloaded start to finish
- **THEN** the run completes without a chapter failing for rate limiting

#### Scenario: The declared concurrency is not exceeded

- **WHEN** the user requests more concurrent workers than the source declares
- **THEN** the run uses the source's maximum

#### Scenario: Building the index is paced like any other work

- **WHEN** a novel long enough to need many index requests is resolved
- **THEN** those requests are spaced by the source's declared minimum delay
- **AND** the run is not refused for reading its own index too quickly

#### Scenario: A refusal while indexing is waited out, not fatal

- **WHEN** the site refuses an index request for exceeding its rate limit and
  states how long the client should wait
- **THEN** the source waits at least that long and retries
- **AND** the index completes without the user restarting the run
