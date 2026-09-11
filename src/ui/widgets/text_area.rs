use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use unicode_normalization::UnicodeNormalization;

/// Result of processing a single key event in [`TextArea`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAreaAction {
    /// No transition: repaint and keep waiting for input.
    Continue,
    /// User pressed Enter; the value is the answer.
    Submit,
    /// User pressed Esc, meaning "go back to the previous step".
    Cancel,
    /// User pressed Ctrl+C, meaning "exit the wizard".
    Quit,
}

/// The wrapped layout the prompt last rendered with, handed to
/// [`TextArea::handle_key`] so row movement can be interpreted.
///
/// Passed per keystroke rather than stored: "one row up" has no meaning
/// without a wrap width, but keeping it on the widget would be state to
/// re-sync on every resize. The draw path reports what it actually rendered
/// with, so there is one producer and nothing to invalidate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextAreaLayout {
    /// Columns available for text inside the block's borders.
    pub width: usize,
}

/// Multi-line text editor for values too long to read on one line, such as a
/// book description. Unlike [`super::TextInput`] it carries an insertion
/// point, so a word in the middle of a long paragraph can be corrected
/// without deleting everything after it.
///
/// The cursor is a `char` index rather than a byte offset, and the value is
/// held in NFC. Every Vietnamese letter has a precomposed form, so one `char`
/// is one letter for this crate's corpus and a tone mark can never be
/// stranded by a single delete.
///
/// Ceiling: emoji and ZWJ sequences are several `char`s each and still split
/// under one delete. No novel blurb carries them, which is why this indexes
/// by `char` instead of pulling in a grapheme-segmentation dependency.
///
/// The widget stores no layout. Row movement needs one to interpret a
/// keystroke, so it arrives as a [`TextAreaLayout`] argument to
/// [`handle_key`](TextArea::handle_key); rendering calls [`wrap_text`] and
/// [`wrapped_cursor_position`] with the same values. Nothing is kept between
/// keystrokes, so a resize cannot strand stale layout state.
pub struct TextArea {
    value: String,
    cursor: usize,
}

impl TextArea {
    /// Create an empty editor with the cursor at the start.
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
        }
    }

    /// Replace the value and place the cursor after its last character. The
    /// text is composed on the way in, so a source that spells a toned vowel
    /// as a base letter plus combining marks does not cost the user several
    /// keystrokes to delete one letter.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into().nfc().collect();
        self.cursor = self.len();
    }

    /// Current value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Insertion point as a `char` index into the value.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Length of the value in `char`s, which is what the cursor indexes.
    fn len(&self) -> usize {
        self.value.chars().count()
    }

    /// Byte offset of `char` index `index`, for slicing the value.
    fn byte_offset(&self, index: usize) -> usize {
        self.value
            .char_indices()
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or(self.value.len())
    }

    /// Insert one character at the cursor and step past it.
    ///
    /// Only the text up to and including the new character is recomposed, so
    /// a combining mark typed after its base letter merges into that letter
    /// and the cursor lands after one letter rather than after two `char`s.
    /// The tail is already composed and is left untouched.
    fn insert(&mut self, c: char) {
        let at = self.byte_offset(self.cursor);
        let tail = self.value.split_off(at);
        self.value.push(c);
        self.value = self.value.nfc().collect();
        self.cursor = self.value.chars().count();
        self.value.push_str(&tail);
    }

    /// Remove the character before the cursor, or do nothing at the start.
    fn delete_backwards(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_offset(self.cursor - 1);
        let end = self.byte_offset(self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Remove the character at the cursor, or do nothing at the end. The
    /// cursor stays put: the tail slides in under it.
    fn delete_forwards(&mut self) {
        if self.cursor >= self.len() {
            return;
        }
        let start = self.byte_offset(self.cursor);
        let end = self.byte_offset(self.cursor + 1);
        self.value.replace_range(start..end, "");
    }

    /// Move the cursor one row up (`-1`) or down (`1`) through the wrapped
    /// layout, holding its column where the target row is long enough and
    /// stopping at that row's end where it is not.
    ///
    /// Past either edge the cursor clamps to the end of the value instead:
    /// `Up` on the first row lands on the start and `Down` on the last row
    /// on the end. That is how a Mac text view behaves, and it is the
    /// advertised way to reach either end, because Mac keyboards need fn or
    /// a remap to produce `Home` and `End` at all.
    fn move_rows(&mut self, delta: isize, layout: TextAreaLayout) {
        let lines = wrap_text(&self.value, layout.width);
        let (row, column) = wrapped_cursor_position(&lines, self.cursor, layout.width);
        // `row` can sit one past the last line when the cursor is at the end
        // of a full row, so the bounds are checked before any arithmetic.
        if delta < 0 && row == 0 {
            self.cursor = 0;
            return;
        }
        if delta > 0 && row + 1 >= lines.len() {
            self.cursor = self.len();
            return;
        }
        let target = row.saturating_add_signed(delta);
        let start: usize = lines
            .iter()
            .take(target)
            .map(|line| line.chars().count())
            .sum();
        let visible = lines
            .get(target)
            .map(|line| line.trim_end_matches('\n').chars().count())
            .unwrap_or(0);
        self.cursor = (start + column.min(visible)).min(self.len());
    }

    /// Process a key event and return the resulting action.
    ///
    /// Enter submits, because the prompt shares the wizard's key contract, so
    /// a line break needs its own key: `Ctrl+J`, the literal line feed, which
    /// the terminal delivers as `Char('j')` with Control and so cannot be
    /// mistaken for Enter.
    ///
    /// `Home` and `End` act on the whole value. They are silent aliases for
    /// the arrow clamping in [`move_rows`](Self::move_rows), kept for users
    /// whose keyboards produce them. `Delete` is likewise silent: it removes
    /// the character after the cursor, and a Mac keyboard needs fn to send it.
    pub fn handle_key(&mut self, event: KeyEvent, layout: TextAreaLayout) -> TextAreaAction {
        if event.kind == KeyEventKind::Release {
            return TextAreaAction::Continue;
        }
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && event.code == KeyCode::Char('c') {
            return TextAreaAction::Quit;
        }
        if ctrl && event.code == KeyCode::Char('j') {
            self.insert('\n');
            return TextAreaAction::Continue;
        }
        // Any other chord is not one of ours. Falling through would insert a
        // stray `a` on Ctrl+A, or submit on Alt+Enter from a user reaching
        // for a line break.
        if ctrl || event.modifiers.contains(KeyModifiers::ALT) {
            return TextAreaAction::Continue;
        }
        match event.code {
            KeyCode::Char(c) => {
                self.insert(c);
                TextAreaAction::Continue
            }
            KeyCode::Backspace => {
                self.delete_backwards();
                TextAreaAction::Continue
            }
            KeyCode::Delete => {
                self.delete_forwards();
                TextAreaAction::Continue
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                TextAreaAction::Continue
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.len());
                TextAreaAction::Continue
            }
            KeyCode::Up => {
                self.move_rows(-1, layout);
                TextAreaAction::Continue
            }
            KeyCode::Down => {
                self.move_rows(1, layout);
                TextAreaAction::Continue
            }
            KeyCode::Home => {
                self.cursor = 0;
                TextAreaAction::Continue
            }
            KeyCode::End => {
                self.cursor = self.len();
                TextAreaAction::Continue
            }
            KeyCode::Enter => TextAreaAction::Submit,
            KeyCode::Esc => TextAreaAction::Cancel,
            _ => TextAreaAction::Continue,
        }
    }
}

impl Default for TextArea {
    /// Same as [`TextArea::new`].
    fn default() -> Self {
        Self::new()
    }
}

/// Wrap one break-free run of text into `out`, always pushing at least one
/// line so an empty run still occupies a row.
fn wrap_segment(text: &str, width: usize, out: &mut Vec<String>) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        out.push(String::new());
        return;
    }
    let mut start = 0;
    while start < chars.len() {
        if chars.len() - start <= width {
            out.push(chars[start..].iter().collect());
            break;
        }
        let end = match chars[start..start + width].iter().rposition(|c| *c == ' ') {
            Some(space) => start + space + 1,
            None => start + width,
        };
        out.push(chars[start..end].iter().collect());
        start = end;
    }
}

/// Wrap `value` to `width` columns, returning one string per visual row.
///
/// The rows concatenate back to exactly `value`: a soft break keeps the
/// space it broke at on the row above, and an explicit line break stays
/// attached to the row it ends, so a cursor index into the value maps into
/// the wrapped layout with no bookkeeping. The draw path strips the trailing
/// break before rendering, since it is not a visible character.
///
/// Rows break at the last space that fits; a word longer than the width is
/// broken at the width, since the alternative is a row that overflows. An
/// empty value, and a value ending in a break, both yield a trailing empty
/// row, so the cursor always has one to sit on.
pub fn wrap_text(value: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    for segment in value.split_inclusive('\n') {
        let (text, has_break) = match segment.strip_suffix('\n') {
            Some(text) => (text, true),
            None => (segment, false),
        };
        wrap_segment(text, width, &mut lines);
        if has_break && let Some(last) = lines.last_mut() {
            last.push('\n');
        }
    }
    if lines.is_empty() || value.ends_with('\n') {
        lines.push(String::new());
    }
    lines
}

/// Map a cursor `char` index into the `(row, column)` it occupies in `lines`,
/// as produced by [`wrap_text`] at the same `width`.
///
/// A cursor at the very end of a value whose last line is full has no column
/// left on that line, so it is reported at the start of the row below, which
/// is where the next character typed would actually appear.
pub fn wrapped_cursor_position(lines: &[String], cursor: usize, width: usize) -> (usize, usize) {
    let mut consumed = 0;
    for (row, line) in lines.iter().enumerate() {
        let length = line.chars().count();
        if cursor < consumed + length {
            return (row, cursor - consumed);
        }
        consumed += length;
    }
    let row = lines.len().saturating_sub(1);
    let column = lines.last().map(|l| l.chars().count()).unwrap_or(0);
    if column >= width.max(1) {
        (row + 1, 0)
    } else {
        (row, column)
    }
}
