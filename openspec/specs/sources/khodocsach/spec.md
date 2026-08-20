## Purpose

Defines how the application obtains novels from khodocsach.com, a site that exposes a JSON API rather than scrapeable pages, and that guards chapter content with a short-lived signed ticket and a strict rate limiter.

## Requirements

### Requirement: Novel URLs on the khodocsach host are accepted

The source SHALL register the `khodocsach.com` host and SHALL accept a novel URL of the form the site publishes for a book page. The source SHALL derive the novel's slug from the URL path.

#### Scenario: Book page URL is accepted

- **WHEN** the user supplies a khodocsach book page URL
- **THEN** the source accepts it and derives the novel slug from the path

#### Scenario: A khodocsach URL that is not a book page is rejected

- **WHEN** the user supplies a khodocsach URL that does not identify a book (for example a genre listing, an author page, or the site root)
- **THEN** the source rejects it with an error stating that a book page URL is required

### Requirement: Every request identifies a client

The source SHALL send a non-empty browser-style client identifier on every request. Requests without one are refused by the site's edge with a forbidden response.

#### Scenario: Requests carry a client identifier

- **WHEN** the source issues any request to khodocsach
- **THEN** that request carries a non-empty client identifier header

#### Scenario: A forbidden response is reported distinctly

- **WHEN** the site refuses a request with a forbidden response
- **THEN** the failure is reported as a rejected-client error, distinct from a rate-limit or a not-found error

### Requirement: Novel metadata and chapter index come from the site API

The source SHALL resolve the novel by its slug and obtain the title, author, description, cover image location, publication status and total chapter count from the site's book endpoint. It SHALL obtain the chapter index from the site's chapter-listing endpoint, in ascending chapter order.

The source SHALL NOT parse the site's HTML pages for any of this data.

#### Scenario: Metadata is read from the book endpoint

- **WHEN** the source resolves a novel by slug
- **THEN** the title, author, description, cover location and status are taken from the book endpoint response

#### Scenario: Unknown slug is a clear failure

- **WHEN** the supplied slug does not correspond to a book on the site
- **THEN** the source fails with an error naming the slug as not found
- **AND** no chapter download is attempted

### Requirement: The chapter index is paginated to completion

The site's chapter listing returns at most a fixed maximum of entries per request and reports the total number of pages. The source SHALL request successive pages until the whole index is retrieved, and SHALL request no more than the site's per-page maximum.

The assembled index SHALL contain one entry per chapter, in ascending order, each carrying the chapter's identifier, its display title and its position.

#### Scenario: A multi-page index is fully assembled

- **WHEN** a novel's chapter count exceeds the site's per-page maximum
- **THEN** the source retrieves every page
- **AND** the assembled index length equals the total the site reported

#### Scenario: An over-large page size is capped

- **WHEN** the source requests more entries per page than the site permits
- **THEN** it treats the site's returned page size as authoritative and paginates accordingly

#### Scenario: A short or failed page aborts indexing

- **WHEN** a page of the chapter index fails to load after retries
- **THEN** indexing fails with an error rather than proceeding with a partial index

### Requirement: Chapter content requires a freshly issued ticket

Chapter content is served only against a signed ticket. The source SHALL request a ticket for a chapter and then use it to retrieve that chapter's content. A ticket SHALL be requested immediately before the content request it authorizes, because tickets expire approximately one minute after issue.

A ticket SHALL be used only for the chapter it was issued for; it is not valid for any other chapter.

#### Scenario: Ticket then content

- **WHEN** the source downloads a chapter
- **THEN** it first obtains a ticket for that chapter
- **AND** it then retrieves the content using that ticket

#### Scenario: Tickets are not shared between chapters

- **WHEN** the source downloads more than one chapter
- **THEN** it obtains a separate ticket for each chapter

#### Scenario: An expired ticket is re-obtained

- **WHEN** a content request is refused because the ticket is no longer valid
- **THEN** the source obtains a fresh ticket and retries the content request

#### Scenario: No authentication is required for freely readable chapters

- **WHEN** the source downloads a chapter of a freely readable novel
- **THEN** it succeeds without any user account, credential or session

### Requirement: The source paces itself against the site rate limiter

The site enforces a rate limit that permits a short burst and then refuses further requests for a sustained period, with the ticket step refused first. The source SHALL declare a conservative concurrency and inter-request delay, and SHALL treat a refusal as retryable rather than fatal.

The site does not advertise a retry delay, so the source SHALL apply its own increasing backoff.

#### Scenario: Sustained downloading stays within the limit

- **WHEN** a novel of many chapters is downloaded end to end
- **THEN** the run completes with every chapter either downloaded or reported failed
- **AND** the run does not abort because of rate limiting

#### Scenario: A refusal at the ticket step is retried

- **WHEN** a ticket request is refused for rate limiting
- **THEN** the source waits and retries rather than failing the chapter immediately

#### Scenario: The user is told when throttling is slowing the run

- **WHEN** rate-limit refusals occur repeatedly during a run
- **THEN** the user is shown that the run is being throttled by the site

### Requirement: Chapter content is normalized into paragraphs

Chapter content arrives as plain text with no markup. The source SHALL split it into an ordered list of paragraphs and pair it with the chapter title from the index, matching the stored representation every source produces.

#### Scenario: Plain text becomes ordered paragraphs

- **WHEN** the source retrieves a chapter's content
- **THEN** it produces the chapter title and an ordered list of text paragraphs
- **AND** the reading order of the original text is preserved

#### Scenario: Empty content is a failure

- **WHEN** a chapter's content is empty or normalizes to no paragraphs
- **THEN** the chapter is reported as failed rather than stored

### Requirement: Chapters the user may not read are reported, not silently skipped

Some novels or chapters on the site require purchase or a subscription. When the site declines to serve a chapter's content because the user lacks entitlement, the source SHALL report that chapter as unavailable with entitlement named as the reason.

#### Scenario: An unentitled chapter is labeled

- **WHEN** the site declines a chapter because the user does not own or subscribe to it
- **THEN** that chapter is reported as unavailable for entitlement reasons
- **AND** it is distinguished from a rate-limit failure and from a network failure

#### Scenario: A partly unavailable novel still produces output

- **WHEN** some chapters of a novel are unavailable for entitlement reasons and others download successfully
- **THEN** the run completes and an EPUB is produced from the chapters that were obtained
- **AND** the user is told how many chapters were omitted and why
