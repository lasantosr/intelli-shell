use anyhow::{bail, Result};
use crossterm::event::Event;
use itertools::Itertools;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    Frame,
};

use crate::{
    common::{
        widget::{
            CustomParagraph, CustomStatefulList, CustomStatefulWidget, CustomWidget, LabelSuggestionItem, TextInput,
            DEFAULT_HIGHLIGHT_SYMBOL_PREFIX, NEW_LABEL_PREFIX, SECRET_LABEL_PREFIX,
        },
        ExecutionContext, InteractiveProcess,
    },
    model::LabeledCommand,
    storage::SqliteStorage,
    Process, ProcessOutput,
};

/// Process to complete [LabeledCommand]
pub struct LabelProcess<'s> {
    /// Storage
    storage: &'s SqliteStorage,
    /// Command
    command: CustomParagraph<LabeledCommand>,
    /// Current label index
    current_label_ix: usize,
    /// Current label name
    current_label: String,
    /// Suggestions for the current label
    suggestions: CustomStatefulList<LabelSuggestionItem>,
    // Execution context
    ctx: ExecutionContext,
}

impl<'s> LabelProcess<'s> {
    pub fn new(storage: &'s SqliteStorage, command: LabeledCommand, ctx: ExecutionContext) -> Result<Self> {
        let (current_label_ix, current_label) = command
            .next_label()
            .ok_or_else(|| anyhow::anyhow!("Command doesn't have labels"))?;
        let current_label = current_label.to_owned();
        let suggestions = Self::suggestion_items_for(storage, &command.root, &current_label, TextInput::default())?;

        let suggestions = CustomStatefulList::new(suggestions)
            .inline(ctx.inline)
            .style(Style::default().fg(ctx.theme.main))
            .highlight_style(
                Style::default()
                    .bg(ctx.theme.selected_background)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(DEFAULT_HIGHLIGHT_SYMBOL_PREFIX);

        let command = CustomParagraph::new(command)
            .inline(ctx.inline)
            .block_title("Command")
            .style(Style::default().fg(ctx.theme.main));

        Ok(Self {
            storage,
            command,
            current_label_ix,
            current_label,
            suggestions,
            ctx,
        })
    }

    fn suggestion_items_for(
        storage: &SqliteStorage,
        root_cmd: &str,
        label: &str,
        new_suggestion: TextInput,
    ) -> Result<Vec<LabelSuggestionItem>> {
        if is_secret_label(label) {
            Ok(vec![LabelSuggestionItem::Secret(new_suggestion)])
        } else {
            let mut suggestions = storage
                .find_suggestions_for(root_cmd, label)?
                .into_iter()
                .map(LabelSuggestionItem::Persisted)
                .collect_vec();

            let mut suggestions_from_label = label
                .split('|')
                .map(|l| LabelSuggestionItem::Label(l.to_owned()))
                .collect_vec();
            suggestions.append(&mut suggestions_from_label);

            if !new_suggestion.as_str().is_empty() {
                suggestions.retain(|s| match s {
                    LabelSuggestionItem::Secret(_) => true,
                    LabelSuggestionItem::New(_) => true,
                    LabelSuggestionItem::Label(l) => l.contains(new_suggestion.as_str()),
                    LabelSuggestionItem::Persisted(s) => s.suggestion.contains(new_suggestion.as_str()),
                })
            }
            suggestions.insert(0, LabelSuggestionItem::New(new_suggestion));

            Ok(suggestions)
        }
    }
}

impl<'s> Process for LabelProcess<'s> {
    fn min_height(&self) -> usize {
        (self.suggestions.len() + 1).clamp(4, 15)
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) {
        // Prepare main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(!self.ctx.inline as u16)
            .constraints([Constraint::Length(self.command.min_size().height), Constraint::Min(1)])
            .split(area);

        let header = chunks[0];
        let body = chunks[1];

        // Display command
        self.command.render_in(frame, header, self.ctx.theme);

        // Display label suggestions
        self.suggestions.render_in(frame, body, self.ctx.theme);
        // check if cursor is needed
        match self.suggestions.current() {
            Some(LabelSuggestionItem::Secret(t)) => {
                frame.set_cursor(
                    // Put cursor at the input text offset
                    body.x
                        + DEFAULT_HIGHLIGHT_SYMBOL_PREFIX.len() as u16
                        + SECRET_LABEL_PREFIX.len() as u16
                        + t.cursor().x
                        + (!self.ctx.inline as u16),
                    // Move one line down, from the border to the input line
                    body.y + (!self.ctx.inline as u16),
                );
            }
            Some(LabelSuggestionItem::New(t)) => {
                frame.set_cursor(
                    // Put cursor at the input text offset
                    body.x
                        + DEFAULT_HIGHLIGHT_SYMBOL_PREFIX.len() as u16
                        + NEW_LABEL_PREFIX.len() as u16
                        + t.cursor().x
                        + (!self.ctx.inline as u16),
                    // Move one line down, from the border to the input line
                    body.y + (!self.ctx.inline as u16),
                );
            }
            _ => (),
        }
    }

    fn process_raw_event(&mut self, event: Event) -> Result<Option<ProcessOutput>> {
        self.process_event(event)
    }
}

impl<'s> InteractiveProcess for LabelProcess<'s> {
    fn move_up(&mut self) {
        self.suggestions.previous()
    }

    fn move_down(&mut self) {
        self.suggestions.next()
    }

    fn move_left(&mut self) {
        match self.suggestions.current_mut() {
            Some(LabelSuggestionItem::Secret(suggestion)) | Some(LabelSuggestionItem::New(suggestion)) => {
                suggestion.move_left()
            }
            _ => (),
        }
    }

    fn move_right(&mut self) {
        match self.suggestions.current_mut() {
            Some(LabelSuggestionItem::Secret(suggestion)) | Some(LabelSuggestionItem::New(suggestion)) => {
                suggestion.move_right()
            }
            _ => (),
        }
    }

    fn prev(&mut self) {
        self.suggestions.previous()
    }

    fn next(&mut self) {
        self.suggestions.next()
    }

    fn home(&mut self) {
        self.suggestions.first()
    }

    fn end(&mut self) {
        self.suggestions.last()
    }

    fn insert_text(&mut self, text: String) -> Result<()> {
        match self.suggestions.current_mut() {
            Some(LabelSuggestionItem::Secret(suggestion)) => {
                suggestion.insert_text(text);
            }
            Some(LabelSuggestionItem::New(suggestion)) => {
                suggestion.insert_text(text);
                let suggestion = suggestion.clone();
                self.suggestions.update_items(Self::suggestion_items_for(
                    self.storage,
                    &self.command.inner().root,
                    &self.current_label,
                    suggestion,
                )?);
            }
            _ => (),
        }
        Ok(())
    }

    fn insert_char(&mut self, c: char) -> Result<()> {
        match self.suggestions.current_mut() {
            Some(LabelSuggestionItem::Secret(suggestion)) => {
                suggestion.insert_char(c);
            }
            Some(LabelSuggestionItem::New(suggestion)) => {
                suggestion.insert_char(c);
                let suggestion = suggestion.clone();
                self.suggestions.update_items(Self::suggestion_items_for(
                    self.storage,
                    &self.command.inner().root,
                    &self.current_label,
                    suggestion,
                )?);
            }
            _ => (),
        }
        Ok(())
    }

    fn delete_char(&mut self, backspace: bool) -> Result<()> {
        match self.suggestions.current_mut() {
            Some(LabelSuggestionItem::Secret(suggestion)) => {
                suggestion.delete_char(backspace);
            }
            Some(LabelSuggestionItem::New(suggestion)) => {
                if suggestion.delete_char(backspace) {
                    let suggestion = suggestion.clone();
                    self.suggestions.update_items(Self::suggestion_items_for(
                        self.storage,
                        &self.command.inner().root,
                        &self.current_label,
                        suggestion,
                    )?);
                }
            }
            _ => (),
        }
        Ok(())
    }

    fn edit_current(&mut self) -> Result<()> {
        Ok(())
    }

    fn delete_current(&mut self) -> Result<()> {
        if let Some(LabelSuggestionItem::Persisted(_)) = self.suggestions.current() {
            if let Some(LabelSuggestionItem::Persisted(suggestion)) = self.suggestions.delete_current() {
                self.storage.delete_label_suggestion(&suggestion)?;
            }
        }
        Ok(())
    }

    fn accept_current(&mut self) -> Result<Option<ProcessOutput>> {
        if let Some(suggestion) = self.suggestions.current_mut() {
            match suggestion {
                LabelSuggestionItem::Secret(value) => {
                    self.command.inner_mut().set_next_label(value.as_str());
                }
                LabelSuggestionItem::New(value) => {
                    if !value.as_str().is_empty() {
                        let suggestion = self
                            .command
                            .inner()
                            .new_suggestion_for(&self.current_label, value.as_str());
                        self.storage.insert_label_suggestion(&suggestion)?;
                    }
                    self.command.inner_mut().set_next_label(value.as_str());
                }
                LabelSuggestionItem::Label(value) => {
                    self.command.inner_mut().set_next_label(value.clone());
                }
                LabelSuggestionItem::Persisted(suggestion) => {
                    suggestion.increment_usage();
                    self.storage.update_label_suggestion(suggestion)?;
                    self.command.inner_mut().set_next_label(&suggestion.suggestion);
                }
            }
            match self.command.inner().next_label() {
                Some((ix, label)) => {
                    self.current_label_ix = ix;
                    self.current_label = label.to_owned();

                    let suggestions = Self::suggestion_items_for(
                        self.storage,
                        &self.command.inner().root,
                        label,
                        TextInput::default(),
                    )?;
                    self.suggestions.update_items(suggestions);
                    self.suggestions.reset_state();

                    Ok(None)
                }
                None => Ok(Some(ProcessOutput::output(self.command.inner().to_string()))),
            }
        } else {
            bail!("Expected at least one suggestion")
        }
    }

    fn exit(&mut self) -> Result<ProcessOutput> {
        Ok(ProcessOutput::output(self.command.inner().to_string()))
    }
}

fn is_secret_label(label_name: &str) -> bool {
    label_name.starts_with('*') && label_name.ends_with('*')
}
