use anyhow::Result;
use crossterm::event::Event;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::LabelWidget;
use crate::{
    common::{EditableText, InputWidget, OverflowText, StatefulList, StrExt, Widget},
    model::{AsLabeledCommand, Command},
    storage::SqliteStorage,
    theme::Theme,
    WidgetOutput,
};

/// Widget to search for [Command]
pub struct SearchWidget<'s> {
    /// Storage
    storage: &'s SqliteStorage,
    /// Current value of the filter box
    filter: EditableText,
    /// Command list of results
    commands: StatefulList<Command>,
    /// Delegate label widget
    delegate_label: Option<LabelWidget<'s>>,
}

impl<'s> SearchWidget<'s> {
    pub fn new(storage: &'s SqliteStorage, filter: String) -> Result<Self> {
        let commands = storage.find_commands(&filter)?;
        Ok(Self {
            commands: StatefulList::with_items(commands),
            filter: EditableText::from_str(filter),
            storage,
            delegate_label: None,
        })
    }

    pub fn exit_or_label_replace(&mut self, output: WidgetOutput) -> Result<Option<WidgetOutput>> {
        if let Some(cmd) = &output.output {
            if let Some(labeled_cmd) = cmd.as_labeled_command() {
                let w = LabelWidget::new(self.storage, labeled_cmd)?;
                self.delegate_label = Some(w);
                return Ok(None);
            }
        }
        Ok(Some(output))
    }
}

impl<'s> Widget for SearchWidget<'s> {
    fn min_height(&self) -> usize {
        (self.commands.len() + 1).clamp(4, 15)
    }

    fn peek(&mut self) -> Result<Option<WidgetOutput>> {
        if self.storage.is_empty()? {
            let message = indoc::indoc! { r#"
                -> There are no stored commands yet!
                    - Try to bookmark some command with 'Ctrl + B'
                    - Or execute 'intelli-shell fetch' to download a bunch of tldr's useful commands"# 
            };
            Ok(Some(WidgetOutput::message(message)))
        } else if !self.filter.as_str().is_empty() && self.commands.len() == 1 {
            if let Some(command) = self.commands.current_mut() {
                command.increment_usage();
                self.storage.update_command(command)?;
                let cmd = command.cmd.clone();
                self.exit_or_label_replace(WidgetOutput::output(cmd))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect, inline: bool, theme: Theme) {
        // If there's a delegate active, forward to it
        if let Some(delegate) = &mut self.delegate_label {
            delegate.render(frame, area, inline, theme);
            return;
        }

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
        let mut filter_offset = self.filter.offset();
        let max_width = header.width as usize - 1 - (2 * (!inline as usize));
        let text_inline = format!("(filter): {}", self.filter);
        let filter_text = if inline {
            filter_offset += 10;
            OverflowText::new(max_width, &text_inline)
        } else {
            OverflowText::new(max_width, self.filter.as_str())
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

    fn process_raw_event(&mut self, event: Event) -> Result<Option<WidgetOutput>> {
        // If there's a delegate active, forward to it
        if let Some(delegate) = &mut self.delegate_label {
            delegate.process_event(event)
        } else {
            self.process_event(event)
        }
    }
}

impl<'s> InputWidget for SearchWidget<'s> {
    fn move_up(&mut self) {
        self.commands.previous()
    }

    fn move_down(&mut self) {
        self.commands.next()
    }

    fn move_left(&mut self) {
        self.filter.move_left()
    }

    fn move_right(&mut self) {
        self.filter.move_right()
    }

    fn prev(&mut self) {
        self.commands.previous()
    }

    fn next(&mut self) {
        self.commands.next()
    }

    fn insert_text(&mut self, text: String) -> Result<()> {
        self.filter.insert_text(text);
        self.commands
            .update_items(self.storage.find_commands(self.filter.as_str())?);
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        self.filter.insert_char(c);
        self.commands
            .update_items(self.storage.find_commands(self.filter.as_str())?);
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        if self.filter.delete_char(backspace) {
            self.commands
                .update_items(self.storage.find_commands(self.filter.as_str())?);
        }
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        if let Some(cmd) = self.commands.delete_current() {
            self.storage.delete_command(cmd.id)?;
        }
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<WidgetOutput>> {
        if let Some(command) = self.commands.current_mut() {
            command.increment_usage();
            self.storage.update_command(command)?;
            let cmd = command.cmd.clone();
            self.exit_or_label_replace(WidgetOutput::output(cmd))
        } else if !self.filter.as_str().is_empty() {
            self.exit_or_label_replace(WidgetOutput::output(self.filter.as_str()))
        } else {
            Ok(Some(WidgetOutput::empty()))
        }
    }

    fn exit(&mut self) -> Result<WidgetOutput> {
        if self.filter.as_str().is_empty() {
            Ok(WidgetOutput::empty())
        } else {
            Ok(WidgetOutput::output(self.filter.as_str()))
        }
    }
}
