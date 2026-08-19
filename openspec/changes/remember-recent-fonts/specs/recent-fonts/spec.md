## Purpose

Remembers the custom EPUB fonts a user has confirmed in the interactive wizard,
so later runs can offer them as ready-made choices instead of asking for a full
path again, and keeps that remembered list honest by discarding entries whose
files are no longer usable.

## ADDED Requirements

### Requirement: Remembered fonts persist across runs

The system SHALL persist remembered fonts in a JSON file named
`recent-fonts.json` inside a `novel-downloader` directory under the user's
config root, resolved as `$XDG_CONFIG_HOME` when that variable is set and
non-empty, otherwise `$HOME/.config`. Each remembered entry SHALL record the
font's canonical absolute path, its family name, its lowercased dot-prefixed
file extension, and its size in bytes. The list SHALL be stored newest-used
first.

When neither `$XDG_CONFIG_HOME` nor `$HOME` is set, the system SHALL disable
persistence entirely: loading yields an empty list, recording is a no-op, and no
file or directory is created.

#### Scenario: Store round-trips through a config directory

- **WHEN** a font is recorded into an empty config directory and the list is
  loaded again from that same directory
- **THEN** the loaded list contains exactly that font, with its canonical path,
  family name, extension, and size

#### Scenario: Store file is created on first record

- **WHEN** a font is recorded and the `novel-downloader` directory does not exist
- **THEN** the directory and `recent-fonts.json` are created

#### Scenario: No config root available

- **WHEN** the config root cannot be resolved
- **THEN** loading returns an empty list, recording succeeds without writing, and
  no file is created

### Requirement: Recording orders entries by most recent use

Recording a font SHALL place it at the front of the list. When the font is
already remembered, recording SHALL move that existing entry to the front rather
than adding a duplicate, leaving the list length unchanged. When the font is not
already remembered and the list already holds 10 entries, recording SHALL insert
the font at the front and drop the last entry, keeping the list at 10. Entries
SHALL be compared by canonical path.

#### Scenario: Reusing a remembered font moves it to the front

- **WHEN** a list holds fonts A, B, C in that order and B is recorded
- **THEN** the list holds B, A, C and its length is unchanged

#### Scenario: A new font evicts the oldest entry at capacity

- **WHEN** a list holds 10 entries and an eleventh, previously unseen font is
  recorded
- **THEN** the new font is first, the previously last entry is gone, and the list
  holds 10 entries

#### Scenario: Two paths that resolve to the same file are one entry

- **WHEN** the same font is recorded twice through different but equivalent paths
- **THEN** the list holds a single entry for it

### Requirement: Only usable custom fonts are remembered

Recording SHALL store a font only when its file can be read and its metadata
extracted; an unreadable or non-font path SHALL leave the list unchanged. The
bundled `Bokerlam.ttf` that ships with the application SHALL never be recorded,
identified by comparing the canonical path of the font being recorded against
the canonical path of the bundled font.

#### Scenario: An unreadable path is not remembered

- **WHEN** recording is asked to remember a path that does not exist
- **THEN** the stored list is unchanged

#### Scenario: The bundled font is not remembered

- **WHEN** recording is asked to remember the bundled font's own path
- **THEN** the stored list is unchanged

### Requirement: Invalid entries are pruned silently on load

Loading SHALL check every remembered entry against the filesystem and drop each
entry whose file cannot be inspected, for any reason — deleted, renamed,
permission denied, or on an unmounted volume. Pruning SHALL be silent: no
prompt, no warning, and no error surfaced to the caller. When pruning changed
the list, the system SHALL rewrite the store so the next load is already clean;
when nothing was pruned, the store SHALL be left untouched. Surviving entries
SHALL keep their relative order.

#### Scenario: A deleted font disappears from the list

- **WHEN** a list holds fonts A, B, C and B's file is deleted before loading
- **THEN** loading returns A and C in that order

#### Scenario: Pruning rewrites the store

- **WHEN** loading pruned at least one entry
- **THEN** a second load reads a store that no longer contains the pruned entry

#### Scenario: A clean load leaves the store alone

- **WHEN** loading finds every entry valid
- **THEN** the store file is not rewritten

### Requirement: Cached metadata avoids re-reading font files

Loading SHALL use each entry's recorded family name and extension rather than
re-parsing the font file, so that a load costs one filesystem metadata lookup
per entry. When an entry's actual size differs from its recorded size, the
system SHALL treat the cached metadata as stale, re-extract that one entry's
family name and extension, update its recorded size, and keep the entry at its
existing position.

#### Scenario: Unchanged fonts are served from cache

- **WHEN** a remembered font's file is unchanged since it was recorded
- **THEN** the loaded entry reports the recorded family name

#### Scenario: A replaced font of a different size is refreshed

- **WHEN** a remembered font's file is replaced by a different font whose size
  differs from the recorded size
- **THEN** the loaded entry reports the new font's family name, records the new
  size, and holds its former position in the list

### Requirement: An unreadable store never breaks the application

Loading SHALL treat a missing, empty, malformed, or otherwise unparseable store
file as an empty list and SHALL NOT return an error. The next successful record
SHALL overwrite such a file with a well-formed store. Unrecognized fields in a
stored entry SHALL be ignored, and absent optional fields SHALL fall back to
defaults, so a store written by a future version does not fail to load.

#### Scenario: Malformed JSON loads as empty

- **WHEN** the store file contains text that is not valid JSON for the expected
  shape
- **THEN** loading returns an empty list without an error

#### Scenario: Recording repairs a malformed store

- **WHEN** a font is recorded over a malformed store
- **THEN** the store afterwards is well-formed and holds exactly that font

### Requirement: The wizard offers remembered fonts at the font step

The interactive wizard's EPUB-font step SHALL list, after the bundled-font
option and before the custom-path option, one option per remembered font
showing its family name and its path. Choosing a remembered font SHALL set it as
the plan's font and go straight to the confirmation step without asking for a
path. When no fonts are remembered, the step SHALL present exactly the two
options it presents today. Validation and pruning SHALL run once per wizard run,
so returning to the font step by back-navigation SHALL NOT re-check the
filesystem.

#### Scenario: Remembered fonts appear as options

- **WHEN** the font step runs with two remembered fonts
- **THEN** the option list is the bundled font, those two fonts, then the
  custom-path option

#### Scenario: Choosing a remembered font skips the path prompt

- **WHEN** the user selects a remembered font
- **THEN** the wizard advances to the confirmation step and the plan carries that
  font's path

#### Scenario: Nothing remembered

- **WHEN** the font step runs with an empty remembered list
- **THEN** only the bundled-font and custom-path options are offered

### Requirement: A confirmed plan records its custom font

The system SHALL record the confirmed font exactly once per interactive run,
when the user confirms the plan, and SHALL record nothing when the user
abandons the wizard or chooses the bundled font. A font that was pre-filled from
the `--font-path` flag and then confirmed through the wizard SHALL be recorded
like any other; a non-interactive run SHALL NOT record anything.

#### Scenario: Confirming a custom font remembers it

- **WHEN** the user confirms a plan whose font is a custom path
- **THEN** that font is at the front of the remembered list on the next run

#### Scenario: Abandoning the wizard remembers nothing

- **WHEN** the user quits before confirming
- **THEN** the remembered list is unchanged

#### Scenario: A non-interactive run remembers nothing

- **WHEN** the application runs with a positional URL and `--font-path`
- **THEN** the remembered list is unchanged

### Requirement: A bad custom font path is rejected at the prompt

When the user submits a custom font path that cannot be read as a font, the
wizard SHALL tell the user at that prompt and re-ask for the path, instead of
accepting it and failing later during EPUB packaging.

#### Scenario: Non-existent path is rejected immediately

- **WHEN** the user submits a path with no file behind it
- **THEN** the wizard reports the problem and asks for the path again

#### Scenario: A valid path advances

- **WHEN** the user submits a path to a readable font file
- **THEN** the wizard advances to the confirmation step
