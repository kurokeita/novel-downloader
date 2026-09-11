use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::Terminal;
use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use std::io::Stdout;

use crate::ui::widgets::{
    PathInput, PathInputAction, Select, SelectAction, SelectOption, TextArea, TextAreaAction,
    TextAreaLayout, TextInput, TextInputAction, Validator, wrap_text, wrapped_cursor_position,
};
use crate::ui::{
    PromptOutcome, TerminalGuard, footer_hint, header_paragraph, is_ctrl_c, next_key_event,
    palette, styled_block,
};

/// Render one frame of the text-prompt screen with the given title, message
/// body, and current `TextInput` state.
fn draw_text_prompt(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
    title: &str,
    message: &str,
    input: &TextInput,
    placeholder: Option<&str>,
) -> Result<()> {
    terminal.draw(|frame| {
        let area = frame.area().inner(Margin::new(2, 1));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(prompt_block_height(message)),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        frame.render_widget(header_paragraph(), chunks[0]);

        let prompt = Paragraph::new(message)
            .block(styled_block(title))
            .wrap(Wrap { trim: false });
        frame.render_widget(prompt, chunks[2]);

        let value_style = Style::default()
            .fg(palette::ACCENT)
            .add_modifier(Modifier::BOLD);
        let display: Line = if input.value().is_empty() {
            match placeholder {
                Some(p) => Line::from(Span::styled(
                    p.to_string(),
                    Style::default()
                        .fg(palette::MUTED)
                        .add_modifier(Modifier::ITALIC),
                )),
                None => Line::from(""),
            }
        } else {
            Line::from(vec![Span::styled(input.value().to_string(), value_style)])
        };
        let mut input_block = styled_block("Input");
        if input.error().is_some() {
            input_block = input_block.border_style(Style::default().fg(palette::DANGER));
        }
        let value = Paragraph::new(display).block(input_block);
        frame.render_widget(value, chunks[3]);

        let status_text = match input.error() {
            Some(err) => Line::from(vec![
                Span::styled(
                    "✗ ",
                    Style::default()
                        .fg(palette::DANGER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.to_string(), Style::default().fg(palette::DANGER)),
            ]),
            None => Line::from(vec![
                Span::styled(
                    "↵",
                    Style::default()
                        .fg(palette::SUCCESS)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" confirm   ", Style::default().fg(palette::MUTED)),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(palette::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" back   ", Style::default().fg(palette::MUTED)),
                Span::styled(
                    "Ctrl+C",
                    Style::default()
                        .fg(palette::DANGER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" quit", Style::default().fg(palette::MUTED)),
            ]),
        };
        let status = Paragraph::new(status_text).block(styled_block("Status"));
        frame.render_widget(status, chunks[4]);

        frame.render_widget(
            footer_hint("type to edit  •  Backspace to delete"),
            chunks[6],
        );
    })?;
    Ok(())
}

/// Run the text prompt synchronously and return the resulting
/// [`PromptOutcome`]: `Submitted(value)` on Enter, `Back` on Esc,
/// `Quit` on Ctrl+C.
pub fn run_text_prompt(
    title: &str,
    message: &str,
    initial: Option<String>,
    placeholder: Option<&str>,
    validator: Option<Validator>,
) -> Result<PromptOutcome<String>> {
    let mut guard = TerminalGuard::enter()?;
    let mut input = match validator {
        Some(v) => TextInput::with_boxed_validator(v),
        None => TextInput::new(),
    };
    if let Some(value) = initial {
        input.set_value(value);
    }
    loop {
        draw_text_prompt(&mut guard.terminal, title, message, &input, placeholder)?;
        let event = next_key_event()?;
        match input.handle_key(event) {
            TextInputAction::Submit => {
                return Ok(PromptOutcome::Submitted(input.value().to_string()));
            }
            TextInputAction::Cancel => return Ok(PromptOutcome::Back),
            TextInputAction::Quit => return Ok(PromptOutcome::Quit),
            TextInputAction::Invalid(_) | TextInputAction::Continue => continue,
        }
    }
}

/// Render the path-picker prompt: header, title block, input row, scrollable
/// suggestion list with the highlighted entry reversed, status footer.
fn draw_path_prompt(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
    title: &str,
    message: &str,
    input: &PathInput,
) -> Result<()> {
    terminal.draw(|frame| {
        let area = frame.area().inner(Margin::new(2, 1));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(prompt_block_height(message)),
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);

        frame.render_widget(header_paragraph(), chunks[0]);

        let prompt = Paragraph::new(message)
            .block(styled_block(title))
            .wrap(Wrap { trim: false });
        frame.render_widget(prompt, chunks[2]);

        let value_style = Style::default()
            .fg(palette::ACCENT)
            .add_modifier(Modifier::BOLD);
        let value_line: Line = if input.value().is_empty() {
            Line::from(Span::styled(
                "Type to start; Tab completes; ↑/↓ pick a match.",
                Style::default()
                    .fg(palette::MUTED)
                    .add_modifier(Modifier::ITALIC),
            ))
        } else {
            Line::from(Span::styled(input.value().to_string(), value_style))
        };
        let input_block = Paragraph::new(value_line).block(styled_block("Path"));
        frame.render_widget(input_block, chunks[3]);

        let suggestions = input.suggestions();
        let mut list_state = ListState::default();
        if let Some(i) = input.highlighted() {
            list_state.select(Some(i));
        }
        let items: Vec<ListItem> = suggestions
            .iter()
            .map(|p| ListItem::new(Line::from(p.clone())))
            .collect();
        let list_title = if suggestions.is_empty() {
            "Suggestions  (none)"
        } else {
            "Suggestions"
        };
        let list = List::new(items)
            .block(styled_block(list_title))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(palette::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        frame.render_stateful_widget(list, chunks[4], &mut list_state);

        frame.render_widget(
            footer_hint(
                "Tab complete  •  ↑/↓ navigate  •  ↵ pick / submit  •  Esc back  •  Ctrl+C quit",
            ),
            chunks[5],
        );
    })?;
    Ok(())
}

/// Run the path-picker prompt synchronously and return the resulting
/// [`PromptOutcome`]: `Submitted(path)`, `Back` on Esc, `Quit` on Ctrl+C.
pub fn run_path_prompt(
    title: &str,
    message: &str,
    initial: Option<String>,
) -> Result<PromptOutcome<String>> {
    let mut guard = TerminalGuard::enter()?;
    let mut input = PathInput::new();
    if let Some(value) = initial {
        input.set_value(value);
    } else {
        input.refresh_completions();
    }
    loop {
        draw_path_prompt(&mut guard.terminal, title, message, &input)?;
        let event = next_key_event()?;
        match input.handle_key(event) {
            PathInputAction::Submit => {
                return Ok(PromptOutcome::Submitted(input.value().to_string()));
            }
            PathInputAction::Cancel => return Ok(PromptOutcome::Back),
            PathInputAction::Quit => return Ok(PromptOutcome::Quit),
            PathInputAction::Continue => continue,
        }
    }
}

/// Render a select prompt: title + message, scrollable list, status line.
fn draw_select<T>(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
    title: &str,
    message: &str,
    select: &Select<T>,
    list_state: &mut ListState,
) -> Result<()>
where
    T: Clone,
{
    terminal.draw(|frame| {
        let area = frame.area().inner(Margin::new(2, 1));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(prompt_block_height(message)),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(area);
        frame.render_widget(header_paragraph(), chunks[0]);

        let prompt = Paragraph::new(message)
            .block(styled_block(title))
            .wrap(Wrap { trim: false });
        frame.render_widget(prompt, chunks[2]);

        let items: Vec<ListItem> = select
            .options()
            .iter()
            .map(|o| {
                let mut spans = vec![Span::raw(o.label.clone())];
                if let Some(hint) = &o.hint {
                    spans.push(Span::styled(
                        format!("  ({})", hint),
                        Style::default()
                            .fg(palette::MUTED)
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let list = List::new(items)
            .block(styled_block(select.list_title()))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(palette::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        frame.render_stateful_widget(list, chunks[3], list_state);

        frame.render_widget(
            footer_hint("↑/↓ move  •  ↵ confirm  •  Esc back  •  Ctrl+C quit"),
            chunks[4],
        );
    })?;
    Ok(())
}

/// Run a select prompt synchronously and return the resulting
/// [`PromptOutcome`]: `Submitted(value)`, `Back` on Esc, `Quit` on Ctrl+C.
pub fn run_select<T>(title: &str, message: &str, mut select: Select<T>) -> Result<PromptOutcome<T>>
where
    T: Clone,
{
    let mut guard = TerminalGuard::enter()?;
    let mut list_state = ListState::default();
    list_state.select(Some(select.cursor()));
    loop {
        draw_select(
            &mut guard.terminal,
            title,
            message,
            &select,
            &mut list_state,
        )?;
        let event = next_key_event()?;
        let action = select.handle_key(event);
        list_state.select(Some(select.cursor()));
        match action {
            SelectAction::Submit(value) => return Ok(PromptOutcome::Submitted(value)),
            SelectAction::Cancel => return Ok(PromptOutcome::Back),
            SelectAction::Quit => return Ok(PromptOutcome::Quit),
            SelectAction::Continue => continue,
        }
    }
}

/// Display a static text screen and wait for a keypress: Enter advances
/// (Submitted), Esc goes back, Ctrl+C quits.
pub fn show_note(title: &str, body: &str) -> Result<PromptOutcome<()>> {
    let mut guard = TerminalGuard::enter()?;
    loop {
        guard.terminal.draw(|frame| {
            let area = frame.area().inner(Margin::new(2, 1));
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(5),
                    Constraint::Length(1),
                ])
                .split(area);
            frame.render_widget(header_paragraph(), chunks[0]);
            let para = Paragraph::new(body)
                .block(styled_block(title))
                .wrap(Wrap { trim: false });
            frame.render_widget(para, chunks[2]);
            frame.render_widget(
                footer_hint("↵ continue  •  Esc back  •  Ctrl+C quit"),
                chunks[3],
            );
        })?;
        let event = next_key_event()?;
        if is_ctrl_c(&event) {
            return Ok(PromptOutcome::Quit);
        }
        match event.code {
            KeyCode::Enter => return Ok(PromptOutcome::Submitted(())),
            KeyCode::Esc => return Ok(PromptOutcome::Back),
            _ => continue,
        }
    }
}

/// Compute the height of a prompt's title block so that every line in the
/// `message` is visible. The result includes the two border rows and is
/// floor-clamped to 5 so a one-line message still has a comfortable inner
/// height of 3 rows.
pub fn prompt_block_height(message: &str) -> u16 {
    let lines = message.lines().count().max(1);
    let total = lines as u16 + 2;
    total.max(5)
}

/// Show a yes/no confirmation prompt and return the user's choice. When
/// `default_yes` is true the cursor starts on "Yes", otherwise on "No".
pub fn run_confirm(title: &str, message: &str, default_yes: bool) -> Result<PromptOutcome<bool>> {
    let options = vec![
        SelectOption {
            label: "Yes".to_string(),
            value: true,
            hint: None,
        },
        SelectOption {
            label: "No".to_string(),
            value: false,
            hint: None,
        },
    ];
    let initial = default_yes;
    run_select(
        title,
        message,
        Select::with_initial(options, &initial).with_list_title("Proceed"),
    )
}

/// Height of the editor block for `line_count` wrapped lines: the lines
/// themselves plus the block's two borders, never smaller than a single-line
/// input so a short value does not render a cramped box.
fn text_area_block_height(line_count: usize) -> u16 {
    let lines = u16::try_from(line_count).unwrap_or(u16::MAX);
    lines.saturating_add(2).max(3)
}

/// First wrapped row to draw, so the cursor's row stays on screen when the
/// value is taller than the block. Scrolls the minimum needed rather than
/// centering, which keeps the view still while the user types.
fn text_area_scroll(cursor_row: usize, visible_rows: usize) -> u16 {
    let last_visible = visible_rows.saturating_sub(1);
    u16::try_from(cursor_row.saturating_sub(last_visible)).unwrap_or(u16::MAX)
}

/// Render one frame of the multi-line editor prompt: the value wrapped to the
/// block's width, the terminal cursor placed at the insertion point, and the
/// placeholder in place of an empty value.
///
/// Returns the layout it actually rendered with, so the runner can hand the
/// same values to the next keystroke rather than recomputing the arithmetic
/// and risking a second copy that drifts.
fn draw_text_area_prompt(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
    title: &str,
    message: &str,
    area_widget: &TextArea,
    placeholder: Option<&str>,
) -> Result<TextAreaLayout> {
    let mut rendered = TextAreaLayout::default();
    terminal.draw(|frame| {
        let area = frame.area().inner(Margin::new(2, 1));
        // The editor spans the full width, so the wrap width is known before
        // the vertical split and the block can be sized from the line count.
        let wrap_width = area.width.saturating_sub(2) as usize;
        let lines = wrap_text(area_widget.value(), wrap_width);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(prompt_block_height(message)),
                Constraint::Length(text_area_block_height(lines.len())),
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        frame.render_widget(header_paragraph(), chunks[0]);

        let prompt = Paragraph::new(message)
            .block(styled_block(title))
            .wrap(Wrap { trim: false });
        frame.render_widget(prompt, chunks[2]);

        let editor_area = chunks[3];
        // Read the height back from the split rather than trusting the
        // requested one: a short terminal gets less than was asked for, and
        // the cursor has to stay inside what was actually granted.
        let visible_rows = editor_area.height.saturating_sub(2) as usize;
        rendered = TextAreaLayout { width: wrap_width };
        let (cursor_row, cursor_column) =
            wrapped_cursor_position(&lines, area_widget.cursor(), wrap_width);
        let scroll = text_area_scroll(cursor_row, visible_rows);

        let body = if area_widget.value().is_empty() {
            match placeholder {
                Some(p) => Paragraph::new(Line::from(Span::styled(
                    p.to_string(),
                    Style::default()
                        .fg(palette::MUTED)
                        .add_modifier(Modifier::ITALIC),
                ))),
                None => Paragraph::new(""),
            }
        } else {
            let value_style = Style::default()
                .fg(palette::ACCENT)
                .add_modifier(Modifier::BOLD);
            Paragraph::new(
                lines
                    .iter()
                    .map(|line| {
                        // The break is what put the next row on screen; drawing
                        // it would show a stray glyph at the end of this one.
                        let text = line.trim_end_matches('\n').to_string();
                        Line::from(Span::styled(text, value_style))
                    })
                    .collect::<Vec<_>>(),
            )
            .scroll((scroll, 0))
        };
        frame.render_widget(body.block(styled_block("Input")), editor_area);

        if visible_rows > 0 {
            let row = u16::try_from(cursor_row)
                .unwrap_or(u16::MAX)
                .saturating_sub(scroll);
            let column = u16::try_from(cursor_column).unwrap_or(u16::MAX);
            frame.set_cursor_position((
                editor_area.x + 1 + column.min(editor_area.width.saturating_sub(2)),
                editor_area.y + 1 + row,
            ));
        }

        let status = Paragraph::new(Line::from(vec![
            Span::styled(
                "↵",
                Style::default()
                    .fg(palette::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm   ", Style::default().fg(palette::MUTED)),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(palette::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" back   ", Style::default().fg(palette::MUTED)),
            Span::styled(
                "Ctrl+C",
                Style::default()
                    .fg(palette::DANGER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" quit", Style::default().fg(palette::MUTED)),
        ]))
        .block(styled_block("Status"));
        frame.render_widget(status, chunks[4]);

        frame.render_widget(
            footer_hint("←/→/↑/↓ move  •  Backspace delete  •  Ctrl+J new line"),
            chunks[6],
        );
    })?;
    Ok(rendered)
}

/// Run the multi-line editor prompt synchronously and return the resulting
/// [`PromptOutcome`]: `Submitted(value)` on Enter, `Back` on Esc, `Quit` on
/// Ctrl+C. Takes no validator: the values it collects are free prose, and a
/// blank one is a meaningful answer.
pub fn run_text_area_prompt(
    title: &str,
    message: &str,
    initial: Option<String>,
    placeholder: Option<&str>,
) -> Result<PromptOutcome<String>> {
    let mut guard = TerminalGuard::enter()?;
    let mut editor = TextArea::new();
    if let Some(value) = initial {
        editor.set_value(value);
    }
    loop {
        let layout =
            draw_text_area_prompt(&mut guard.terminal, title, message, &editor, placeholder)?;
        let event = next_key_event()?;
        match editor.handle_key(event, layout) {
            TextAreaAction::Submit => {
                return Ok(PromptOutcome::Submitted(editor.value().to_string()));
            }
            TextAreaAction::Cancel => return Ok(PromptOutcome::Back),
            TextAreaAction::Quit => return Ok(PromptOutcome::Quit),
            TextAreaAction::Continue => continue,
        }
    }
}

/// Tested inline because both helpers are private to this module: an
/// integration test under `tests/` could only reach them through a `pub`
/// re-export nothing else needs. The drawing around them stays under
/// eyes-on-terminal verification, as the project's definition of done
/// requires.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_area_block_height_covers_the_lines_plus_the_borders() {
        assert_eq!(text_area_block_height(4), 6);
    }

    #[test]
    fn text_area_block_height_never_goes_below_a_single_line_box() {
        assert_eq!(text_area_block_height(0), 3);
        assert_eq!(text_area_block_height(1), 3);
    }

    #[test]
    fn text_area_scroll_stays_at_the_top_while_the_cursor_fits() {
        assert_eq!(text_area_scroll(0, 5), 0);
        assert_eq!(text_area_scroll(4, 5), 0);
    }

    #[test]
    fn text_area_scroll_follows_a_cursor_past_the_last_visible_row() {
        assert_eq!(text_area_scroll(5, 5), 1);
        assert_eq!(text_area_scroll(9, 5), 5);
    }

    #[test]
    fn text_area_scroll_survives_a_block_with_no_room_for_a_row() {
        assert_eq!(text_area_scroll(3, 0), 3);
    }
}
