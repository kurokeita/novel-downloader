## Purpose

Gives each chapter in a generated EPUB a printed-novel opening: the first
letter of the chapter's first paragraph is set as an oversized initial that the
following lines wrap around, while chapter openings where that treatment would
look wrong are left plain.

## ADDED Requirements

### Requirement: Chapter openings carry a drop cap letter

When a chapter's first paragraph begins with a letter, the generated chapter
document SHALL wrap that single letter in a `<span class="dropcap">` element and
SHALL mark the paragraph that carries it with a class distinguishing it from
ordinary paragraphs. The remainder of the paragraph text SHALL follow the span
unchanged, and no other paragraph in the chapter SHALL be marked.

The drop cap SHALL be applied to the chapter's first paragraph only, regardless
of how many paragraphs the chapter holds.

#### Scenario: First paragraph begins with a letter

- **WHEN** a chapter whose first paragraph begins with `Sương mù phủ kín thung
  lũng` is written into the EPUB
- **THEN** that paragraph carries the drop cap class, its first letter `S` is
  wrapped in a `dropcap` span, and the text following the span reads
  `ương mù phủ kín thung lũng`

#### Scenario: Later paragraphs are untouched

- **WHEN** a chapter holds three paragraphs, all beginning with letters
- **THEN** exactly one `dropcap` span appears in the chapter document, and the
  second and third paragraphs carry neither the span nor the class

### Requirement: Openings that do not begin with a letter are left plain

When a chapter's first paragraph does not begin with a letter, the generated
chapter document SHALL contain no drop cap span and no drop cap paragraph
class, and the paragraph markup SHALL be identical to what the same chapter
produced before this capability existed.

An opening does not begin with a letter when its first character is punctuation
(including a quotation mark, an apostrophe, a dash of any width, or an opening
bracket), a digit, whitespace, or an HTML entity reference, or when the
paragraph holds no text at all. A chapter whose body holds no paragraph element
SHALL likewise be left unchanged.

#### Scenario: Dialogue opening is skipped

- **WHEN** a chapter whose first paragraph begins with a quotation mark followed
  by dialogue is written into the EPUB
- **THEN** the chapter document holds no `dropcap` span and the paragraph is
  emitted exactly as it was received

#### Scenario: Dash opening is skipped

- **WHEN** a chapter's first paragraph begins with a dash followed by dialogue
- **THEN** the chapter document holds no `dropcap` span

#### Scenario: Entity opening is skipped

- **WHEN** a chapter's first paragraph begins with the entity reference
  `&quot;` rather than a literal quotation mark
- **THEN** the chapter document holds no `dropcap` span, and the entity is
  emitted unchanged

#### Scenario: Empty first paragraph is skipped

- **WHEN** a chapter's first paragraph holds no text
- **THEN** the chapter document holds no `dropcap` span

### Requirement: Decomposed letters produce a single drop cap character

The system SHALL normalize the opening of a chapter's first paragraph to
Unicode NFC before selecting the drop cap letter, so that a Vietnamese letter
written as a base character followed by combining marks yields one precomposed
character inside the span. Combining marks belonging to the drop cap letter
SHALL NOT be left outside the span, and the text following the span SHALL begin
at the second letter of the paragraph.

#### Scenario: NFD input yields a precomposed drop cap

- **WHEN** a chapter's first paragraph begins with the letter `Ế` written in
  decomposed form, as `E` followed by its combining marks
- **THEN** the `dropcap` span holds the single precomposed character `Ế`, and no
  combining mark appears immediately after the closing span tag

### Requirement: The embedded stylesheet floats the drop cap

The stylesheet embedded in every generated EPUB SHALL style the drop cap span
as a left-floated initial sized to span several lines of body text, and SHALL
suppress the first-line indent on the paragraph that carries the drop cap,
overriding the indent applied to ordinary paragraphs.

#### Scenario: Stylesheet carries the drop cap rules

- **WHEN** an EPUB is built
- **THEN** its embedded stylesheet contains a rule floating the `dropcap` class
  to the left at a font size larger than the body text, and a rule setting the
  text indent of the drop cap paragraph to zero

### Requirement: Non-chapter documents carry no drop cap

The title page and the navigation document SHALL contain no drop cap span and
no drop cap paragraph class, so that the author name and the table of contents
render at ordinary size even though they share the embedded stylesheet.

#### Scenario: Title page author is not drop capped

- **WHEN** an EPUB is built with an author
- **THEN** the title page document holds no `dropcap` span and the author
  paragraph carries no drop cap class
