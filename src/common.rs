use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use itertools::Itertools;
use regex::{CaptureMatches, Captures, Regex};
use tui::{backend::Backend, layout::Rect, text::Text, widgets::ListState, Frame, Terminal};
use unicode_segmentation::UnicodeSegmentation;
use unidecode::unidecode;

use crate::theme::Theme;

/// Applies [unidecode] to the given string and then converts it to lower case
pub fn flatten_str(s: impl AsRef<str>) -> String {
    unidecode(s.as_ref()).to_lowercase()
}

pub struct WidgetOutput<T: ToString> {
    pub message: Option<String>,
    pub output: Option<T>,
}
impl<T: ToString> WidgetOutput<T> {
    pub fn new(message: impl Into<String>, output: impl Into<T>) -> Self {
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

    pub fn output(output: impl Into<T>) -> Self {
        Self {
            output: Some(output.into()),
            message: None,
        }
    }

    pub fn map<F: FnOnce(T) -> O, O: ToString>(self, f: F) -> WidgetOutput<O> {
        let Self { message, output } = self;
        WidgetOutput {
            message,
            output: output.map(f),
        }
    }
}

pub trait ResultExt<E> {
    fn map_output_str(self) -> Result<WidgetOutput<String>, E>;
}
impl<T: ToString, E> ResultExt<E> for Result<WidgetOutput<T>, E> {
    fn map_output_str(self) -> Result<WidgetOutput<String>, E> {
        self.map(|w| w.map(|o| o.to_string()))
    }
}

/// Trait to display Widgets on the shell
pub trait Widget {
    type Output: ToString;

    /// Minimum height needed to render the widget
    fn min_height(&self) -> usize;

    /// Peeks into the result to check wether the UI should be shown ([None]) or we can give a straight result
    /// ([Some])
    fn peek(&mut self) -> Result<Option<WidgetOutput<Self::Output>>> {
        Ok(None)
    }

    /// Render `self` in the given area from the frame
    fn render<B: Backend>(&mut self, _frame: &mut Frame<B>, _area: Rect, _inline: bool, _theme: Theme) {
        unimplemented!()
    }

    /// Process user input event and return [Some] to end user interaction or [None] to keep waiting for user input
    fn process_event(&mut self, _event: Event) -> Result<Option<WidgetOutput<Self::Output>>> {
        unimplemented!()
    }

    /// Run this widget `render` and `process_event` until we've got a result
    fn show<B, F>(
        mut self,
        terminal: &mut Terminal<B>,
        inline: bool,
        theme: Theme,
        mut area: F,
    ) -> Result<WidgetOutput<Self::Output>>
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
        let text_width = text.len_chars();

        if text_width > max_width {
            let overflow = text_width - max_width;
            let mut text_visible = text.to_owned();
            for _ in 0..overflow {
                text_visible.remove_safe(0);
            }
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
            if self.items.is_empty() {
                self.state.select(None);
            } else {
                let i = if selected >= self.items.len() - 1 {
                    0
                } else {
                    selected + 1
                };
                self.state.select(Some(i));
            }
        }
    }

    /// Selects the previous item on the list
    pub fn previous(&mut self) {
        if let Some(selected) = self.state.selected() {
            if self.items.is_empty() {
                self.state.select(None);
            } else {
                let i = if selected == 0 {
                    self.items.len() - 1
                } else {
                    selected - 1
                };
                self.state.select(Some(i));
            }
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
        let deleted = if let Some(selected) = self.state.selected() {
            Some(self.items.remove(selected))
        } else {
            None
        };

        if self.items.is_empty() {
            self.state.select(None);
        } else if let Some(selected) = self.state.selected() {
            if selected > self.items.len() - 1 {
                self.state.select(Some(self.items.len() - 1));
            }
        } else {
            self.state.select(Some(0));
        }

        deleted
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

/// String utilities to work with [grapheme clusters](https://doc.rust-lang.org/book/ch08-02-strings.html#bytes-and-scalar-values-and-grapheme-clusters-oh-my)
pub trait StringExt {
    /// Inserts a char at a given char index position.
    ///
    /// Unlike [`String::insert`](String::insert), the index is char-based, not byte-based.
    fn insert_safe(&mut self, char_index: usize, c: char);

    /// Removes a char at a given char index position.
    ///
    /// Unlike [`String::remove`](String::remove), the index is char-based, not byte-based.
    fn remove_safe(&mut self, char_index: usize);
}
pub trait StrExt {
    /// Returns the number of characters.
    ///
    /// Unlike [`String::len`](String::len), the number is char-based, not byte-based.
    fn len_chars(&self) -> usize;
}

impl StringExt for String {
    fn insert_safe(&mut self, char_index: usize, new_char: char) {
        let mut v = self.graphemes(true).map(ToOwned::to_owned).collect_vec();
        v.insert(char_index, new_char.to_string());
        *self = v.join("");
    }

    fn remove_safe(&mut self, char_index: usize) {
        *self = self
            .graphemes(true)
            .enumerate()
            .filter_map(|(i, c)| if i != char_index { Some(c) } else { None })
            .collect_vec()
            .join("");
    }
}

impl StrExt for String {
    fn len_chars(&self) -> usize {
        self.graphemes(true).count()
    }
}

impl StrExt for str {
    fn len_chars(&self) -> usize {
        self.graphemes(true).count()
    }
}
