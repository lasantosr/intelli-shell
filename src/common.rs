use std::fmt::Display;

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
    fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect, inline: bool, theme: Theme);

    /// Process raw user input event and return [Some] to end user interaction or [None] to keep waiting for user input
    fn process_raw_event(&mut self, event: Event) -> Result<Option<WidgetOutput>>;

    /// Run this widget `render` and `process_event` until we've got an output
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
            if let Some(res) = self.process_raw_event(event)? {
                return Ok(res);
            }
        }
    }
}

/// Trait to implement input event capturing widgets
pub trait InputWidget: Widget {
    /// Process user input event and return [Some] to end user interaction or [None] to keep waiting for user input
    fn process_event(&mut self, event: Event) -> Result<Option<WidgetOutput>> {
        match event {
            Event::Paste(content) => self.insert_text(content)?,
            Event::Key(key) => {
                let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    // `ctrl + d` - Delete
                    KeyCode::Char(c) if has_ctrl && c == 'd' => self.delete_current()?,
                    // `ctrl + u` | `ctrl + e` | F2 - Edit / Update
                    KeyCode::F(f) if f == 2 => {
                        // TODO edit - delegate to widget?
                    }
                    KeyCode::Char(c) if has_ctrl && (c == 'e' || c == 'u') => {
                        // TODO edit
                    }
                    // Selection
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

    /// Inserts the given text into the currently selected input, if any
    fn insert_text(&mut self, text: String) -> Result<()>;
    /// Inserts the given char into the currently selected input, if any
    fn insert_char(&mut self, c: char) -> Result<()>;
    /// Removes a character from the currently selected input, if any
    fn delete_char(&mut self, backspace: bool) -> Result<()>;

    /// Deleted the currently selected item, if any
    fn delete_current(&mut self) -> Result<()>;
    /// Accepts the currently selected item, if any
    fn accept_current(&mut self) -> Result<Option<WidgetOutput>>;
    /// Exits with the current state
    fn exit(&mut self) -> Result<WidgetOutput>;
}

#[derive(Clone, Default)]
pub struct EditableText {
    text: String,
    offset: usize,
}
impl Display for EditableText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}
impl EditableText {
    pub fn from_str(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            offset: text.len(),
            text,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Moves internal cursor left
    pub fn move_left(&mut self) {
        if self.offset > 0 {
            self.offset -= 1;
        }
    }

    /// Moves internal cursor right
    pub fn move_right(&mut self) {
        if self.offset < self.text.len_chars() {
            self.offset += 1;
        }
    }

    /// Inserts the given text at the internal cursor
    pub fn insert_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        let len = text.len_chars();
        self.text.insert_safe_str(self.offset, text);
        self.offset += len;
    }

    /// Inserts the given char at the internal cursor
    pub fn insert_char(&mut self, c: char) {
        self.text.insert_safe(self.offset, c);
        self.offset += 1;
    }

    /// Deletes the char at the internal cursor and returns if any char was deleted
    pub fn delete_char(&mut self, backspace: bool) -> bool {
        if backspace {
            if !self.text.is_empty() && self.offset > 0 {
                self.text.remove_safe(self.offset - 1);
                self.offset -= 1;
                true
            } else {
                false
            }
        } else if !self.text.is_empty() && self.offset < self.text.len_chars() {
            self.text.remove_safe(self.offset);
            true
        } else {
            false
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
    /// Inserts a `char` at a given char index position.
    ///
    /// Unlike [`String::insert`](String::insert), the index is char-based, not byte-based.
    fn insert_safe(&mut self, char_index: usize, c: char);

    /// Inserts an `String` at a given char index position.
    ///
    /// Unlike [`String::insert`](String::insert), the index is char-based, not byte-based.
    fn insert_safe_str(&mut self, char_index: usize, str: impl Into<String>);

    /// Removes a `char` at a given char index position.
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

    fn insert_safe_str(&mut self, char_index: usize, str: impl Into<String>) {
        let mut v = self.graphemes(true).map(ToOwned::to_owned).collect_vec();
        v.insert(char_index, str.into());
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
