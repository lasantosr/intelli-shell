use anyhow::{bail, Result};
use crossterm::event::Event;
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
    common::{EditableText, InputWidget, StatefulList},
    model::{CommandPart, LabelSuggestion, LabeledCommand},
    storage::SqliteStorage,
    theme::Theme,
    Widget, WidgetOutput,
};

/// Widget to complete [LabeledCommand]
pub struct LabelWidget<'s> {
    /// Storage
    storage: &'s SqliteStorage,
    /// Command
    command: LabeledCommand,
    /// Current label index
    current_label_ix: usize,
    /// Current label name
    current_label: String,
    /// Suggestions for the current label
    suggestions: StatefulList<Suggestion>,
}

enum Suggestion {
    New(EditableText),
    Label(String),
    Persisted(LabelSuggestion),
}

impl<'s> LabelWidget<'s> {
    pub fn new(storage: &'s SqliteStorage, command: LabeledCommand) -> Result<Self> {
        let (current_label_ix, current_label) = command
            .next_label()
            .ok_or_else(|| anyhow::anyhow!("Command doesn't have labels"))?;
        let current_label = current_label.to_owned();
        let suggestions = Self::suggestion_items_for(storage, &command.root, &current_label, EditableText::default())?;
        Ok(Self {
            storage,
            command,
            current_label_ix,
            current_label,
            suggestions: StatefulList::with_items(suggestions),
        })
    }

    fn suggestion_items_for(
        storage: &SqliteStorage,
        root_cmd: &str,
        label: &str,
        new_suggestion: EditableText,
    ) -> Result<Vec<Suggestion>> {
        let mut suggestions = storage
            .find_suggestions_for(root_cmd, label)?
            .into_iter()
            .map(Suggestion::Persisted)
            .collect_vec();
        let mut from_label = label.split('|').map(|l| Suggestion::Label(l.to_owned())).collect_vec();
        suggestions.append(&mut from_label);
        if !new_suggestion.as_str().is_empty() {
            suggestions.retain(|s| match s {
                Suggestion::New(_) => true,
                Suggestion::Label(l) => l.contains(new_suggestion.as_str()),
                Suggestion::Persisted(s) => s.suggestion.contains(new_suggestion.as_str()),
            })
        }
        suggestions.insert(0, Suggestion::New(new_suggestion));
        Ok(suggestions)
    }
}

impl<'s> Widget for LabelWidget<'s> {
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
                        CommandPart::Text(t) => Span::raw(t),
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
                Suggestion::New(value) => ListItem::new(Spans::from(vec![
                    Span::styled(NEW_LABEL_PREFIX, Style::default().add_modifier(Modifier::ITALIC)),
                    Span::raw(value.as_str()),
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

        if let Some(Suggestion::New(t)) = self.suggestions.current() {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            frame.set_cursor(
                // Put cursor at the input text offset
                body.x
                    + HIGHLIGHT_SYMBOL_PREFIX.len() as u16
                    + NEW_LABEL_PREFIX.len() as u16
                    + t.offset() as u16
                    + (!inline as u16),
                // Move one line down, from the border to the input line
                body.y + (!inline as u16),
            );
        }
    }

    fn process_raw_event(&mut self, event: Event) -> Result<Option<WidgetOutput>> {
        self.process_event(event)
    }
}

impl<'s> InputWidget for LabelWidget<'s> {
    fn move_up(&mut self) {
        self.suggestions.previous()
    }

    fn move_down(&mut self) {
        self.suggestions.next()
    }

    fn move_left(&mut self) {
        if let Some(Suggestion::New(suggestion)) = self.suggestions.current_mut() {
            suggestion.move_left()
        }
    }

    fn move_right(&mut self) {
        if let Some(Suggestion::New(suggestion)) = self.suggestions.current_mut() {
            suggestion.move_right()
        }
    }

    fn prev(&mut self) {
        self.suggestions.previous()
    }

    fn next(&mut self) {
        self.suggestions.next()
    }

    fn insert_text(&mut self, text: String) -> Result<()> {
        if let Some(Suggestion::New(suggestion)) = self.suggestions.current_mut() {
            suggestion.insert_text(text);
            let suggestion = suggestion.clone();
            self.suggestions.update_items(Self::suggestion_items_for(
                self.storage,
                &self.command.root,
                &self.current_label,
                suggestion,
            )?);
        }
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        if let Some(Suggestion::New(suggestion)) = self.suggestions.current_mut() {
            suggestion.insert_char(c);
            let suggestion = suggestion.clone();
            self.suggestions.update_items(Self::suggestion_items_for(
                self.storage,
                &self.command.root,
                &self.current_label,
                suggestion,
            )?);
        }
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        if let Some(Suggestion::New(suggestion)) = self.suggestions.current_mut() {
            if suggestion.delete_char(backspace) {
                let suggestion = suggestion.clone();
                self.suggestions.update_items(Self::suggestion_items_for(
                    self.storage,
                    &self.command.root,
                    &self.current_label,
                    suggestion,
                )?);
            }
        }
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        if let Some(Suggestion::Persisted(_)) = self.suggestions.current() {
            if let Some(Suggestion::Persisted(suggestion)) = self.suggestions.delete_current() {
                self.storage.delete_label_suggestion(&suggestion)?;
            }
        }
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<WidgetOutput>> {
        if let Some(suggestion) = self.suggestions.current_mut() {
            match suggestion {
                Suggestion::New(value) => {
                    if !value.as_str().is_empty() {
                        let suggestion = self.command.new_suggestion_for(&self.current_label, value.as_str());
                        self.storage.insert_label_suggestion(&suggestion)?;
                    }
                    self.command.set_next_label(value.as_str());
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
                        Self::suggestion_items_for(self.storage, &self.command.root, label, EditableText::default())?;
                    self.suggestions = StatefulList::with_items(suggestions);

                    Ok(None)
                }
                None => Ok(Some(WidgetOutput::output(self.command.to_string()))),
            }
        } else {
            bail!("Expected at least one suggestion")
        }
    }

    fn exit(&mut self) -> Result<WidgetOutput> {
        Ok(WidgetOutput::output(self.command.to_string()))
    }
}
