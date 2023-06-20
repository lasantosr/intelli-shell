use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, layout::Rect, Frame, Terminal};

use super::remove_newlines;
use crate::theme::Theme;

/// Output of a process
pub struct ProcessOutput {
    pub message: Option<String>,
    pub output: Option<String>,
}

impl ProcessOutput {
    pub fn new(message: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            output: Some(output.into()),
        }
    }

    pub fn empty() -> Self {
        Self {
            message: None,
            output: None,
        }
    }

    pub fn message(message: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            output: None,
        }
    }

    pub fn output(output: impl Into<String>) -> Self {
        Self {
            output: Some(output.into()),
            message: None,
        }
    }
}

/// Context of an execution
#[derive(Clone, Copy)]
pub struct ExecutionContext {
    pub inline: bool,
    pub theme: Theme,
}

/// Trait to display a process on the shell
pub trait Process {
    /// Minimum height needed to render the process
    fn min_height(&self) -> usize;

    /// Peeks into the result to check wether the UI should be shown ([None]) or we can give a straight result
    /// ([Some])
    fn peek(&mut self) -> Result<Option<ProcessOutput>> {
        Ok(None)
    }

    /// Render `self` in the given area from the frame
    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect);

    /// Process raw user input event and return [Some] to end user interaction or [None] to keep waiting for user input
    fn process_raw_event(&mut self, event: Event) -> Result<Option<ProcessOutput>>;

    /// Run this process `render` and `process_event` until we've got an output
    fn show<B, F>(mut self, terminal: &mut Terminal<B>, mut area: F) -> Result<ProcessOutput>
    where
        B: Backend,
        F: FnMut(&Frame<B>) -> Rect,
        Self: Sized,
    {
        loop {
            // Draw UI
            terminal.draw(|f| {
                let area = area(f);
                self.render(f, area);
            })?;

            let event = event::read()?;
            if let Event::Key(k) = &event {
                // Ignore release & repeat events, we're only counting Press
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                // Exit on Ctrl+C
                if let KeyCode::Char(c) = k.code {
                    if c == 'c' && k.modifiers.contains(KeyModifiers::CONTROL) {
                        return Ok(ProcessOutput::empty());
                    }
                }
            }

            // Process event
            if let Some(res) = self.process_raw_event(event)? {
                return Ok(res);
            }
        }
    }
}

/// Utility trait to implement an interactive process
pub trait InteractiveProcess: Process {
    /// Process user input event and return [Some] to end user interaction or [None] to keep waiting for user input
    fn process_event(&mut self, event: Event) -> Result<Option<ProcessOutput>> {
        match event {
            Event::Paste(content) => self.insert_text(remove_newlines(content))?,
            Event::Key(key) => {
                let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    // `ctrl + d` - Delete
                    KeyCode::Char(c) if has_ctrl && c == 'd' => self.delete_current()?,
                    // `ctrl + u` | `ctrl + e` | F2 - Edit / Update
                    KeyCode::F(f) if f == 2 => self.edit_current()?,
                    KeyCode::Char(c) if has_ctrl && (c == 'e' || c == 'u') => self.edit_current()?,
                    // Selection
                    KeyCode::Home => self.home(),
                    KeyCode::End => self.end(),
                    KeyCode::Char(c) if has_ctrl && c == 'k' => self.prev(),
                    KeyCode::Char(c) if has_ctrl && c == 'j' => self.next(),
                    KeyCode::Up => self.move_up(),
                    KeyCode::Down => self.move_down(),
                    KeyCode::Right => self.move_right(),
                    KeyCode::Left => self.move_left(),
                    // Text edit
                    KeyCode::Char(c) => self.insert_char(c)?,
                    KeyCode::Backspace => self.delete_char(true)?,
                    KeyCode::Delete => self.delete_char(false)?,
                    // Control flow
                    KeyCode::Enter | KeyCode::Tab => return self.accept_current(),
                    KeyCode::Esc => return self.exit().map(Some),
                    _ => (),
                }
            }
            _ => (),
        };

        // Keep waiting for input
        Ok(None)
    }

    /// Moves the selection up
    fn move_up(&mut self);
    /// Moves the selection down
    fn move_down(&mut self);
    /// Moves the selection left
    fn move_left(&mut self);
    /// Moves the selection right
    fn move_right(&mut self);

    /// Moves the selection to the previous item
    fn prev(&mut self);
    /// Moves the selection to the next item
    fn next(&mut self);

    /// Home button, usually moving selection to the first
    fn home(&mut self);
    /// End button, usually moving selection to the last
    fn end(&mut self);

    /// Inserts the given text into the currently selected input, if any
    fn insert_text(&mut self, text: String) -> Result<()>;
    /// Inserts the given char into the currently selected input, if any
    fn insert_char(&mut self, c: char) -> Result<()>;
    /// Removes a character from the currently selected input, if any
    fn delete_char(&mut self, backspace: bool) -> Result<()>;

    /// Deletes the currently selected item, if any
    fn delete_current(&mut self) -> Result<()>;
    /// Edits the currently selected item, if any
    fn edit_current(&mut self) -> Result<()>;
    /// Accepts the currently selected item, if any
    fn accept_current(&mut self) -> Result<Option<ProcessOutput>>;
    /// Exits with the current state
    fn exit(&mut self) -> Result<ProcessOutput>;
}
