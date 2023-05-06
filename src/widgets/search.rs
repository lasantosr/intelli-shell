use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::{
    common::{OverflowText, StatefulList, StrExt, StringExt, Widget},
    model::{Command, MaybeCommand},
    storage::SqliteStorage,
    theme::Theme,
    WidgetOutput,
};

/// Widget to search for [Command]
pub struct SearchWidget<'s> {
    /// Storage
    storage: &'s mut SqliteStorage,
    /// Current value of the filter box
    filter: String,
    /// Current cursor offset
    cursor_offset: usize,
    /// Command list of results
    commands: StatefulList<Command>,
}

impl<'s> SearchWidget<'s> {
    pub fn new(storage: &'s mut SqliteStorage, filter: String) -> Result<Self> {
        let commands = storage.find_commands(&filter)?;
        Ok(Self {
            commands: StatefulList::with_items(commands),
            cursor_offset: filter.len_chars(),
            filter,
            storage,
        })
    }
}

impl<'s> Widget for SearchWidget<'s> {
    type Output = MaybeCommand;

    fn min_height(&self) -> usize {
        (self.commands.len() + 1).clamp(4, 15)
    }

    fn peek(&mut self) -> Result<Option<WidgetOutput<Self::Output>>> {
        if self.storage.is_empty()? {
            let message = indoc::indoc! { r#"
                -> There are no stored commands yet!
                    - Try to bookmark some command with 'Ctrl + B'
                    - Or execute 'intelli-shell fetch' to download a bunch of tldr's useful commands"# 
            };
            Ok(Some(WidgetOutput::message(message)))
        } else if !self.filter.is_empty() && self.commands.len() == 1 {
            Ok(self.commands.current().map(|c| c.cmd.clone()).map(WidgetOutput::output))
        } else {
            Ok(None)
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect, inline: bool, theme: Theme) {
        // Prepare main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(!inline as u16)
            .constraints([
                if inline {
                    Constraint::Length(1)
                } else {
                    Constraint::Length(3)
                },
                Constraint::Min(1),
            ])
            .split(area);

        let header = chunks[0];
        let body = chunks[1];

        // Display filter
        let mut filter_offset = self.cursor_offset;
        let max_width = header.width as usize - 1 - (2 * (!inline as usize));
        let text_inline = format!("(filter): {}", self.filter);
        let filter_text = if inline {
            filter_offset += 10;
            OverflowText::new(max_width, &text_inline)
        } else {
            OverflowText::new(max_width, &self.filter)
        };
        let filter_text_width = filter_text.width();
        if text_inline.len_chars() > filter_text_width {
            let overflow = text_inline.len_chars() as i32 - filter_text_width as i32;
            if overflow < filter_offset as i32 {
                filter_offset -= overflow as usize;
            } else {
                filter_offset = 0;
            }
        }
        let mut filter_input = Paragraph::new(filter_text).style(Style::default().fg(theme.main));
        if !inline {
            filter_input = filter_input.block(Block::default().borders(Borders::ALL).title(" Filter "));
        }
        frame.render_widget(filter_input, header);

        // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
        frame.set_cursor(
            // Put cursor past the end of the input text
            header.x + filter_offset as u16 + (!inline as u16),
            // Move one line down, from the border to the input line
            header.y + (!inline as u16),
        );

        // Display command suggestions
        let (commands, state) = self.commands.borrow();
        let commands: Vec<ListItem> = commands
            .iter()
            .map(|c| {
                let content = Spans::from(vec![
                    Span::raw(&c.cmd),
                    Span::styled(" # ", Style::default().fg(theme.secondary)),
                    Span::styled(&c.description, Style::default().fg(theme.secondary)),
                ]);
                ListItem::new(content)
            })
            .collect();

        let mut commands = List::new(commands)
            .style(Style::default().fg(theme.main))
            .highlight_style(
                Style::default()
                    .bg(theme.selected_background)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        if !inline {
            commands = commands.block(
                Block::default()
                    .border_style(Style::default().fg(theme.main))
                    .borders(Borders::ALL)
                    .title(" Commands "),
            );
        }
        frame.render_stateful_widget(commands, body, state);
    }

    fn process_event(&mut self, event: Event) -> Result<Option<WidgetOutput<Self::Output>>> {
        if let Event::Key(key) = event {
            let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Char(c) if has_ctrl && c == 'd' => {
                    // Delete
                    if let Some(cmd) = self.commands.delete_current() {
                        self.storage.delete_command(cmd.id)?;
                    }
                }
                KeyCode::Char(c) if has_ctrl && c == 'j' => {
                    self.commands.next();
                }
                KeyCode::Down => {
                    self.commands.next();
                }
                KeyCode::Char(c) if has_ctrl && c == 'k' => {
                    self.commands.previous();
                }
                KeyCode::Up => {
                    self.commands.previous();
                }
                KeyCode::Enter | KeyCode::Tab => {
                    if let Some(cmd) = self.commands.current_mut() {
                        cmd.increment_usage();
                        self.storage.update_command(cmd)?;
                        return Ok(Some(WidgetOutput::output(cmd.clone())));
                    } else if self.filter.is_empty() {
                        return Ok(Some(WidgetOutput::empty()));
                    } else {
                        return Ok(Some(WidgetOutput::output(self.filter.clone())));
                    }
                }
                KeyCode::Char(c) => {
                    self.filter.insert_safe(self.cursor_offset, c);
                    self.cursor_offset += 1;
                    self.commands.update_items(self.storage.find_commands(&self.filter)?);
                }
                KeyCode::Backspace => {
                    if !self.filter.is_empty() && self.cursor_offset > 0 {
                        self.filter.remove_safe(self.cursor_offset - 1);
                        self.cursor_offset -= 1;
                        self.commands.update_items(self.storage.find_commands(&self.filter)?);
                    }
                }
                KeyCode::Delete => {
                    if !self.filter.is_empty() && self.cursor_offset < self.filter.len_chars() {
                        self.filter.remove_safe(self.cursor_offset);
                        self.commands.update_items(self.storage.find_commands(&self.filter)?);
                    }
                }
                KeyCode::Right => {
                    if self.cursor_offset < self.filter.len_chars() {
                        self.cursor_offset += 1;
                    }
                }
                KeyCode::Left => {
                    if self.cursor_offset > 0 {
                        self.cursor_offset -= 1;
                    }
                }
                KeyCode::Esc => {
                    if self.filter.is_empty() {
                        return Ok(Some(WidgetOutput::empty()));
                    } else {
                        return Ok(Some(WidgetOutput::output(self.filter.clone())));
                    }
                }
                _ => (),
            }
        }
        // Continue waiting for input
        Ok(None)
    }
}
