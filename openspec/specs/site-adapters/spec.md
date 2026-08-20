## Purpose

Defines the contract every source site must satisfy so that one download pipeline, one EPUB builder and one terminal UI can serve any number of novel sites, and defines how the application picks the right source for a given URL without asking the user.

## Requirements

### Requirement: Source resolution from the URL host

The application SHALL determine which source to use solely from the host of the URL the user supplies. The user SHALL NOT be asked to pick a source, and no command-line flag SHALL be required to select one.

Host matching SHALL be case-insensitive and SHALL ignore a leading `www.` label.

#### Scenario: Known host resolves to its source

- **WHEN** the user supplies a novel URL whose host is registered by a source
- **THEN** the application selects that source without prompting
- **AND** the resolved source name is shown to the user before the download starts

#### Scenario: Host casing and www prefix are ignored

- **WHEN** the user supplies a URL whose host differs from a registered host only by letter case or a leading `www.`
- **THEN** the application resolves the same source as it would for the canonical host

#### Scenario: Unknown host is rejected with guidance

- **WHEN** the user supplies a URL whose host no source registers
- **THEN** the application rejects the URL before any network request is made
- **AND** the error names the offending host and lists every supported host

#### Scenario: Malformed URL is rejected

- **WHEN** the user supplies a value that cannot be parsed as an absolute URL, or that has no host component
- **THEN** the application rejects it with an error identifying the input as an invalid URL

### Requirement: Sources publish a complete chapter index before downloading

A source SHALL resolve a novel URL into the novel's metadata and a complete, ordered index of its chapters before any chapter is downloaded. The index SHALL be the sole authority on which chapters exist and in what order.

Each index entry SHALL carry a sequence number that is stable across runs, and an opaque locator that only the owning source interprets.

#### Scenario: Index is retrieved before the first chapter download

- **WHEN** a download begins for a supported novel URL
- **THEN** the source returns the novel metadata and the full chapter index first
- **AND** the reported total chapter count comes from that index

#### Scenario: Chapters are addressed by locator, not by arithmetic

- **WHEN** the pipeline downloads a chapter
- **THEN** it passes the locator from the index entry back to the source
- **AND** the pipeline never constructs a chapter address by transforming the novel URL

#### Scenario: Sequence numbering is stable across runs

- **WHEN** the same novel is indexed on two separate runs and no chapters were published in between
- **THEN** each chapter receives the same sequence number in both runs

### Requirement: Novel metadata is supplied by the source

A source SHALL supply the novel's title, and SHALL supply author, description, publication status and cover image location when the site exposes them. The application SHALL NOT re-derive metadata from downloaded chapter files or from a saved copy of the novel page.

#### Scenario: Metadata flows from index to EPUB

- **WHEN** an EPUB is built for a downloaded novel
- **THEN** its title, author, description, status and cover come from the metadata the source returned with the chapter index

#### Scenario: Absent optional metadata degrades gracefully

- **WHEN** a source cannot supply author, description, status or cover for a novel
- **THEN** the download still completes and an EPUB is still produced
- **AND** the absent fields are omitted rather than filled with placeholder text

### Requirement: Each source declares its own rate policy

A source SHALL declare the request pacing it requires: a maximum number of concurrent in-flight requests, and a minimum delay between requests. The download pipeline SHALL NOT exceed either limit, regardless of what concurrency the user requests.

#### Scenario: User concurrency is clamped to the source limit

- **WHEN** the user requests a concurrency higher than the resolved source's declared maximum
- **THEN** the pipeline runs at the source's maximum
- **AND** the user is told the requested concurrency was reduced, and to what

#### Scenario: A lower user concurrency is respected

- **WHEN** the user requests a concurrency at or below the source's declared maximum
- **THEN** the pipeline runs at the user's requested concurrency

### Requirement: Rate-limit responses are retried with backoff

When a source reports that a request was refused for exceeding a rate limit, the pipeline SHALL wait and retry that chapter rather than recording it as a permanent failure. Successive refusals for the same chapter SHALL increase the wait before the next attempt.

After a bounded number of attempts the chapter SHALL be reported as failed, and the failure SHALL name rate limiting as the cause.

#### Scenario: Transient rate limiting recovers without user action

- **WHEN** a chapter request is refused for rate limiting and a later retry succeeds
- **THEN** the chapter is recorded as downloaded
- **AND** the run continues without user intervention

#### Scenario: Repeated refusal surfaces as a labeled failure

- **WHEN** every permitted attempt for a chapter is refused for rate limiting
- **THEN** the chapter is reported as failed with rate limiting named as the cause
- **AND** the run continues with the remaining chapters

#### Scenario: Backoff slows the whole run, not just one chapter

- **WHEN** rate-limit refusals are observed across multiple chapters
- **THEN** the pipeline reduces its overall request rate for the remainder of the run

### Requirement: Downloaded chapters are stored in a source-independent form

Every source SHALL normalize a fetched chapter into the same stored representation: a chapter title and an ordered list of text paragraphs. Downstream EPUB assembly SHALL depend only on that representation and SHALL NOT vary by source.

#### Scenario: EPUB assembly is identical across sources

- **WHEN** two novels of equal chapter count are downloaded from two different sources
- **THEN** EPUB assembly reads the stored chapters through the same path for both

#### Scenario: A chapter yielding no text is a failure

- **WHEN** a source fetches a chapter that normalizes to zero paragraphs
- **THEN** the chapter is reported as failed rather than written as an empty file

### Requirement: Existing metruyenhot behavior is preserved

Relocating the existing metruyenhot logic behind the source contract SHALL NOT change any behavior a user can observe: the same novels resolve, the same chapter counts are discovered, the same chapter text is extracted, and the same EPUB output is produced.

#### Scenario: Existing behavior stays green

- **WHEN** the source contract refactor is complete
- **THEN** the pre-existing metruyenhot test suite passes unchanged

#### Scenario: Both registered hosts keep working

- **WHEN** a novel URL on either previously supported metruyenhot host is supplied
- **THEN** it resolves to the metruyenhot source and downloads as it did before the refactor

### Requirement: Test and local hosts remain reachable

The application SHALL retain an opt-in escape hatch that additionally accepts `localhost` and `127.0.0.1`, so integration tests can run against a local fixture server. The escape hatch SHALL be off by default.

#### Scenario: Local host rejected by default

- **WHEN** a `localhost` URL is supplied without the escape hatch enabled
- **THEN** the URL is rejected as an unsupported host

#### Scenario: Local host accepted with the escape hatch

- **WHEN** a `localhost` URL is supplied with the escape hatch enabled
- **THEN** the URL is accepted and served by the source under test
