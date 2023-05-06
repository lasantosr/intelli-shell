use anyhow::{bail, Result};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use itertools::Itertools;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::{
    common::{StatefulList, StrExt, StringExt},
    model::{CommandPart, LabelSuggestion, LabeledCommand},
    storage::SqliteStorage,
    theme::Theme,
    Widget, WidgetOutput,
};

/// Widget to complete [LabeledCommand]
pub struct LabelWidget<'s> {
    /// Storage
    storage: &'s mut SqliteStorage,
    /// Command
    command: LabeledCommand<'s>,
    /// Current label index
    current_label_ix: usize,
    /// Current label name
    current_label: String,
    /// Suggestions for the current label
    suggestions: StatefulList<Suggestion>,
}

enum Suggestion {
    New(String, usize),
    Label(String),
    Persisted(LabelSuggestion),
}

impl<'s> LabelWidget<'s> {
    pub fn new(storage: &'s mut SqliteStorage, command: LabeledCommand<'s>) -> Result<Self> {
        let (current_label_ix, current_label) = command
            .next_label()
            .ok_or_else(|| anyhow::anyhow!("Command doesn't have labels"))?;
        let current_label = current_label.to_owned();
        let suggestions = Self::suggestion_items_for(storage, command.root, &current_label, "", 0)?;
        Ok(Self {
            storage,
            command,
            current_label_ix,
            current_label,
            suggestions: StatefulList::with_items(suggestions),
        })
    }

    fn suggestion_items_for(
        storage: &mut SqliteStorage,
        root_cmd: &str,
        label: &str,
        filter: &str,
        filter_ix: usize,
    ) -> Result<Vec<Suggestion>> {
        let mut suggestions = storage
            .find_suggestions_for(root_cmd, label)?
            .into_iter()
            .map(Suggestion::Persisted)
            .collect_vec();
        suggestions.insert(0, Suggestion::New(filter.to_owned(), filter_ix));
        let mut from_label = label.split('|').map(|l| Suggestion::Label(l.to_owned())).collect_vec();
        suggestions.append(&mut from_label);
        if !filter.is_empty() {
            suggestions.retain(|s| match s {
                Suggestion::New(_, _) => true,
                Suggestion::Label(l) => l.starts_with(filter),
                Suggestion::Persisted(s) => s.suggestion.starts_with(filter),
            })
        }
        Ok(suggestions)
    }
}

impl<'s> Widget for LabelWidget<'s> {
    type Output = LabeledCommand<'s>;

    fn min_height(&self) -> usize {
        (self.suggestions.len() + 1).clamp(4, 15)
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

        // Display command
        let max_width = header.width - 1 - (2 * (!inline as u16));
        let mut first_label = true;
        let mut current_width = 0;
        let mut command_parts = self
            .command
            .parts
            .iter()
            .filter_map(|p| {
                if first_label || current_width <= max_width {
                    let was_first_label = first_label;
                    let span = match p {
                        CommandPart::Text(t) => Span::raw(*t),
                        CommandPart::Label(l) => {
                            let style = if first_label {
                                first_label = false;
                                Style::default().fg(theme.main).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.disabled)
                            };
                            Span::styled(format!("{{{{{l}}}}}"), style)
                        }
                        CommandPart::LabelValue(v) => Span::raw(v),
                    };
                    current_width += span.width() as u16;
                    if was_first_label || current_width <= max_width {
                        Some(span)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect_vec();
        while command_parts.iter().map(|s| s.width() as u16).sum::<u16>() > max_width {
            command_parts.remove(0);
        }
        let command = Spans::from(command_parts);
        let mut command_widget = Paragraph::new(command).style(Style::default().fg(theme.disabled));
        if !inline {
            command_widget = command_widget.block(Block::default().borders(Borders::ALL).title(" Command "));
        }
        frame.render_widget(command_widget, header);

        // Display label suggestions
        const NEW_LABEL_PREFIX: &str = "(new) ";
        const HIGHLIGHT_SYMBOL_PREFIX: &str = ">> ";
        let (suggestions, state) = self.suggestions.borrow();
        let suggestions: Vec<ListItem> = suggestions
            .iter()
            .map(|c| match c {
                Suggestion::New(value, _) => ListItem::new(Spans::from(vec![
                    Span::styled(NEW_LABEL_PREFIX, Style::default().add_modifier(Modifier::ITALIC)),
                    Span::raw(value),
                ])),
                Suggestion::Label(value) => ListItem::new(value.clone()),
                Suggestion::Persisted(e) => ListItem::new(e.suggestion.clone()),
            })
            .collect();

        let mut suggestions = List::new(suggestions)
            .style(Style::default().fg(theme.main))
            .highlight_style(
                Style::default()
                    .bg(theme.selected_background)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(HIGHLIGHT_SYMBOL_PREFIX);
        if !inline {
            suggestions = suggestions.block(
                Block::default()
                    .border_style(Style::default().fg(theme.main))
                    .borders(Borders::ALL)
                    .title(" Labels "),
            );
        }
        frame.render_stateful_widget(suggestions, body, state);

        if let Some(Suggestion::New(_, offset)) = self.suggestions.current() {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            frame.set_cursor(
                // Put cursor at the input text offset
                body.x
                    + HIGHLIGHT_SYMBOL_PREFIX.len() as u16
                    + NEW_LABEL_PREFIX.len() as u16
                    + *offset as u16
                    + (!inline as u16),
                // Move one line down, from the border to the input line
                body.y + (!inline as u16),
            );
        }
    }

    fn process_event(&mut self, event: Event) -> Result<Option<WidgetOutput<Self::Output>>> {
        if let Event::Key(key) = event {
            let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                // `ctrl + d` - Delete
                KeyCode::Char(c) if has_ctrl && c == 'd' => {
                    if let Some(Suggestion::Persisted(_)) = self.suggestions.current() {
                        if let Some(Suggestion::Persisted(suggestion)) = self.suggestions.delete_current() {
                            self.storage.delete_label_suggestion(&suggestion)?;
                        }
                    }
                }
                KeyCode::Enter | KeyCode::Tab => {
                    if let Some(suggestion) = self.suggestions.current_mut() {
                        match suggestion {
                            Suggestion::New(value, _) => {
                                if !value.is_empty() {
                                    let suggestion =
                                        self.command.new_suggestion_for(&self.current_label, value.clone());
                                    self.storage.insert_label_suggestion(&suggestion)?;
                                }
                                self.command.set_next_label(value.clone());
                            }
                            Suggestion::Label(value) => {
                                self.command.set_next_label(value.clone());
                            }
                            Suggestion::Persisted(suggestion) => {
                                suggestion.increment_usage();
                                self.storage.update_label_suggestion(suggestion)?;
                                self.command.set_next_label(&suggestion.suggestion);
                            }
                        }
                        match self.command.next_label() {
                            Some((ix, label)) => {
                                self.current_label_ix = ix;
                                self.current_label = label.to_owned();

                                let suggestions =
                                    Self::suggestion_items_for(self.storage, self.command.root, label, "", 0)?;
                                self.suggestions = StatefulList::with_items(suggestions);
                            }
                            None => {
                                return Ok(Some(WidgetOutput::output(self.command.clone())));
                            }
                        }
                    } else {
                        bail!("Expected at least one suggestion")
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(Suggestion::New(suggestion, offset)) = self.suggestions.current_mut() {
                        suggestion.insert_safe(*offset, c);
                        *offset += 1;
                        let offset = *offset;
                        let suggestion = suggestion.clone();
                        self.suggestions.update_items(Self::suggestion_items_for(
                            self.storage,
                            self.command.root,
                            &self.current_label,
                            &suggestion,
                            offset,
                        )?);
                    }
                }
                KeyCode::Backspace => {
                    if let Some(Suggestion::New(suggestion, offset)) = self.suggestions.current_mut() {
                        if !suggestion.is_empty() && *offset > 0 {
                            suggestion.remove_safe(*offset - 1);
                            *offset -= 1;
                            let offset = *offset;
                            let suggestion = suggestion.clone();
                            self.suggestions.update_items(Self::suggestion_items_for(
                                self.storage,
                                self.command.root,
                                &self.current_label,
                                &suggestion,
                                offset,
                            )?);
                        }
                    }
                }
                KeyCode::Delete => {
                    if let Some(Suggestion::New(suggestion, offset)) = self.suggestions.current_mut() {
                        if !suggestion.is_empty() && *offset < suggestion.len_chars() {
                            suggestion.remove_safe(*offset);
                            let offset = *offset;
                            let suggestion = suggestion.clone();
                            self.suggestions.update_items(Self::suggestion_items_for(
                                self.storage,
                                self.command.root,
                                &self.current_label,
                                &suggestion,
                                offset,
                            )?);
                        }
                    }
                }
                KeyCode::Right => {
                    if let Some(Suggestion::New(suggestion, offset)) = self.suggestions.current_mut() {
                        if *offset < suggestion.len_chars() {
                            *offset += 1;
                        }
                    }
                }
                KeyCode::Left => {
                    if let Some(Suggestion::New(_, offset)) = self.suggestions.current_mut() {
                        if *offset > 0 {
                            *offset -= 1;
                        }
                    }
                }
                KeyCode::Down => {
                    self.suggestions.next();
                }
                KeyCode::Up => {
                    self.suggestions.previous();
                }
                KeyCode::Esc => {
                    return Ok(Some(WidgetOutput::output(self.command.clone())));
                }
                _ => (),
            }
        }
        // Continue waiting for input
        Ok(None)
    }
}
