use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use regex::{CaptureMatches, Captures, Regex};
use tui::{backend::Backend, layout::Rect, text::Text, widgets::ListState, Frame, Terminal};
use unicode_width::UnicodeWidthStr;
use unidecode::unidecode;

use crate::theme::Theme;

/// Applies [unidecode] to the given string and then converts it to lower case
pub fn flatten_str(s: impl AsRef<str>) -> String {
    unidecode(s.as_ref()).to_lowercase()
}

pub struct WidgetOutput {
    pub message: Option<String>,
    pub output: Option<String>,
}
impl WidgetOutput {
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

/// Trait to display Widgets on the shell
pub trait Widget {
    /// Minimum height needed to render the widget
    fn min_height(&self) -> usize;

    /// Peeks into the result to check wether the UI should be shown ([None]) or we can give a straight result
    /// ([Some])
    fn peek(&mut self) -> Result<Option<WidgetOutput>> {
        Ok(None)
    }

    /// Render `self` in the given area from the frame
    fn render<B: Backend>(&mut self, _frame: &mut Frame<B>, _area: Rect, _inline: bool, _theme: Theme) {
        unimplemented!()
    }

    /// Process user input event and return [Some] to end user interaction or [None] to keep waiting for user input
    fn process_event(&mut self, _event: Event) -> Result<Option<WidgetOutput>> {
        unimplemented!()
    }

    /// Run this widget `render` and `process_event` until we've got a result
    fn show<B, F>(mut self, terminal: &mut Terminal<B>, inline: bool, theme: Theme, mut area: F) -> Result<WidgetOutput>
    where
        B: Backend,
        F: FnMut(&Frame<B>) -> Rect,
        Self: Sized,
    {
        loop {
            // Draw UI
            terminal.draw(|f| {
                let area = area(f);
                self.render(f, area, inline, theme);
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
                        return Ok(WidgetOutput::empty());
                    }
                }
            }

            // Process event by widget
            if let Some(res) = self.process_event(event)? {
                return Ok(res);
            }
        }
    }
}

pub struct OverflowText;
impl OverflowText {
    /// Creates a new [Text]
    ///
    /// The `text` is not expected to contain any newlines
    #[allow(clippy::new_ret_no_self)]
    pub fn new(max_width: usize, text: &str) -> Text<'_> {
        let text_width = text.width();

        if text_width > max_width {
            let overflow = text_width - max_width;
            let text_visible = &text[overflow..];
            Text::raw(text_visible)
        } else {
            Text::raw(text)
        }
    }
}

/// List that keeps the selected item state
#[derive(Default)]
pub struct StatefulList<T> {
    state: ListState,
    items: Vec<T>,
}

impl<T> StatefulList<T> {
    /// Builds a new [StatefulList] from the given items
    pub fn with_items(items: Vec<T>) -> StatefulList<T> {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        StatefulList { state, items }
    }

    /// Updates this list inner items
    pub fn update_items(&mut self, items: Vec<T>) {
        self.items = items;

        if self.items.is_empty() {
            self.state.select(None);
        } else if let Some(selected) = self.state.selected() {
            if selected > self.items.len() - 1 {
                self.state.select(Some(self.items.len() - 1));
            }
        } else {
            self.state.select(Some(0));
        }
    }

    /// Returns the number of items on this list
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Borrows the list to retrieve both inner items and list state
    pub fn borrow(&mut self) -> (&Vec<T>, &mut ListState) {
        (&self.items, &mut self.state)
    }

    /// Selects the next item on the list
    pub fn next(&mut self) {
        if let Some(selected) = self.state.selected() {
            let i = if selected >= self.items.len() - 1 {
                0
            } else {
                selected + 1
            };
            self.state.select(Some(i));
        }
    }

    /// Selects the previous item on the list
    pub fn previous(&mut self) {
        if let Some(selected) = self.state.selected() {
            let i = if selected == 0 {
                self.items.len() - 1
            } else {
                selected - 1
            };
            self.state.select(Some(i));
        }
    }

    /// Returns a mutable reference to the current selected item
    pub fn current_mut(&mut self) -> Option<&mut T> {
        if let Some(selected) = self.state.selected() {
            self.items.get_mut(selected)
        } else {
            None
        }
    }

    /// Returns a reference to the current selected item
    pub fn current(&self) -> Option<&T> {
        if let Some(selected) = self.state.selected() {
            self.items.get(selected)
        } else {
            None
        }
    }

    /// Deletes the currently selected item and returns it
    pub fn delete_current(&mut self) -> Option<T> {
        if let Some(selected) = self.state.selected() {
            Some(self.items.remove(selected))
        } else {
            None
        }
    }
}

/// Iterator to split a test by a regex and capture both unmatched and captured groups
pub struct SplitCaptures<'r, 't> {
    finder: CaptureMatches<'r, 't>,
    text: &'t str,
    last: usize,
    caps: Option<Captures<'t>>,
}

impl<'r, 't> SplitCaptures<'r, 't> {
    /// Builds a new [SplitCaptures]
    pub fn new(re: &'r Regex, text: &'t str) -> SplitCaptures<'r, 't> {
        SplitCaptures {
            finder: re.captures_iter(text),
            text,
            last: 0,
            caps: None,
        }
    }
}

/// Represents each item of a [SplitCaptures]
#[derive(Debug)]
pub enum SplitItem<'t> {
    Unmatched(&'t str),
    Captured(Captures<'t>),
}

impl<'r, 't> Iterator for SplitCaptures<'r, 't> {
    type Item = SplitItem<'t>;

    fn next(&mut self) -> Option<SplitItem<'t>> {
        if let Some(caps) = self.caps.take() {
            return Some(SplitItem::Captured(caps));
        }
        match self.finder.next() {
            None => {
                if self.last >= self.text.len() {
                    None
                } else {
                    let s = &self.text[self.last..];
                    self.last = self.text.len();
                    Some(SplitItem::Unmatched(s))
                }
            }
            Some(caps) => {
                let m = caps.get(0).unwrap();
                let unmatched = &self.text[self.last..m.start()];
                self.last = m.end();
                self.caps = Some(caps);
                Some(SplitItem::Unmatched(unmatched))
            }
        }
    }
}
