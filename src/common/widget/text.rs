use std::{borrow::Cow, fmt::Display};

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use super::{
    super::{StrExt, StringExt},
    Area, CustomWidget, IntoCursorWidget, Offset,
};
use crate::{common::unify_newlines, theme::Theme};

pub struct CustomParagraph<T> {
    text: T,
    inline: bool,
    inline_title: Option<&'static str>,
    block_title: Option<&'static str>,
    focus: bool,
    style: Style,
}

impl<'s, T: 's> CustomParagraph<T>
where
    &'s T: IntoCursorWidget<Text<'s>>,
{
    pub fn new(text: T) -> Self {
        Self {
            text,
            inline: true,
            inline_title: None,
            block_title: None,
            focus: false,
            style: Style::default(),
        }
    }

    pub fn inline(mut self, inline: bool) -> Self {
        self.inline = inline;
        self
    }

    pub fn inline_title(mut self, inline_title: &'static str) -> Self {
        self.inline_title = Some(inline_title);
        self
    }

    pub fn block_title(mut self, block_title: &'static str) -> Self {
        self.block_title = Some(block_title);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn focus(mut self, focus: bool) -> Self {
        self.focus = focus;
        self
    }

    pub fn set_focus(&mut self, focus: bool) -> &mut Self {
        self.focus = focus;
        self
    }

    pub fn inner(&self) -> &T {
        &self.text
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.text
    }
}

impl<'s, T: 's> CustomWidget<'s> for CustomParagraph<T>
where
    &'s T: IntoCursorWidget<Text<'s>>,
{
    type Inner = Paragraph<'s>;

    fn min_size(&self) -> Area {
        let borders = 2 * (!self.inline as u16);
        let height = 1 + borders;
        let width = 50 + borders;
        Area::new(width, height)
    }

    fn is_focused(&self) -> bool {
        self.focus
    }

    fn prepare(&'s self, area: Rect, theme: Theme) -> (Option<Offset>, Self::Inner) {
        let (mut text, data) = (&self.text).into_widget_and_cursor(theme);
        let (mut cursor, visible_area) = data.unzip();
        // Cap cursor and ending offset based on the text
        let mut end_offset = if let Some(cursor) = cursor.as_mut() {
            cursor.x = cursor.x.min(text.width() as u16);
            cursor.y = cursor.y.min(text.lines.len().max(1) as u16 - 1);
            let visible_area = visible_area.unwrap_or_else(Area::default_visible);
            let mut end_offset = Offset::new(cursor.x + visible_area.width, cursor.y + visible_area.height - 1);
            end_offset.x = end_offset.x.min(text.width() as u16);
            end_offset.y = end_offset.y.min(text.lines.len().max(1) as u16 - 1);
            Some(end_offset)
        } else {
            None
        };
        // Always allow an extra char
        let mut max_width = area.width - 1;
        let mut max_height = area.height;
        // If inline, prefix the title and shift the offset
        if self.inline {
            if let Some(inline_title) = self.inline_title {
                if let Some(line) = text.lines.get_mut(0) {
                    line.spans.insert(0, Span::raw(inline_title));
                    line.spans.insert(1, Span::raw(" "));
                } else {
                    text.lines
                        .push(Line::from(vec![Span::raw(inline_title), Span::raw(" ")]));
                }
                // Shift cursor if on the first line
                if let (Some(cursor), Some(end_offset)) = (cursor.as_mut(), end_offset.as_mut()) {
                    if cursor.y == 0 {
                        let extra_offset = inline_title.len_chars() as u16 + 1;
                        cursor.x += extra_offset;
                        end_offset.x += extra_offset;
                    }
                }
            }
        }
        let mut paragraph = Paragraph::new(text).style(self.style);
        // If not inline, include bordered block
        if !self.inline {
            let mut block = Block::default().borders(Borders::ALL);
            if let Some(block_title) = self.block_title {
                block = block.title(format!(" {block_title} "));
            }
            paragraph = paragraph.block(block);
            // Remove borders from max width & height
            max_width -= 2;
            if max_height > 2 {
                max_height -= 2;
            }
            // Shift offset because of borders
            if let (Some(cursor), Some(end_offset)) = (cursor.as_mut(), end_offset.as_mut()) {
                cursor.x += 1;
                cursor.y += 1;
                end_offset.x += 1;
                end_offset.y += 1;
            }
        }
        // If we can't fully "see" the visible offset, scroll
        if let (Some(cursor), Some(end_offset)) = (cursor.as_mut(), end_offset.as_mut()) {
            let mut scroll_x = 0;
            let mut scroll_y = 0;
            if end_offset.x > max_width {
                scroll_x = end_offset.x - max_width;
                scroll_x = scroll_x.min(cursor.x);
                cursor.x -= scroll_x;
            }
            if end_offset.y > max_height {
                scroll_y = end_offset.y - max_height;
                scroll_y = scroll_y.min(cursor.y);
                cursor.y -= scroll_y;
            }
            paragraph = paragraph.scroll((scroll_y, scroll_x));
        }
        // Return
        (cursor, paragraph)
    }
}

/// Convenience class to store input text (with cursor offset)
#[derive(Clone, Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TextInput {
    text: String,
    cursor: Offset,
}

impl TextInput {
    /// Builds a [TextInput] from any string
    pub fn new(text: impl Into<String>) -> Self {
        let text = unify_newlines(text.into());
        // `lines` excludes the last line ending, but we want to include it
        let lines_count = format!("{text}-").lines().count().max(1);
        Self {
            cursor: Offset::new(
                text.lines()
                    .nth(lines_count - 1)
                    .map(str::len_chars)
                    .unwrap_or_default() as u16,
                lines_count as u16 - 1,
            ),
            text,
        }
    }

    /// Retrieves the char index from `self.text` at the current cursor position
    fn char_idx_at_cursor(&self) -> usize {
        if self.cursor.y == 0 {
            self.cursor.x as usize
        } else {
            // Compute previous lines
            let mut idx = self
                .text
                .lines()
                .enumerate()
                .filter_map(|(ix, line)| if ix < self.cursor.y as usize { Some(line) } else { None })
                .map(str::len_chars)
                .sum();
            // Add newline chars
            idx += self.cursor.y as usize;
            // Add current line offset
            idx += self.cursor.x as usize;

            idx
        }
    }

    /// Returns the number of lines of the text (minimum of 1 even if empty)
    pub fn lines_count(&self) -> u16 {
        // `lines` excludes the last line ending, but we want to include it
        format!("{}-", self.text).lines().count().max(1) as u16
    }

    /// Returns the total number of characters of the current line
    pub fn current_line_length(&self) -> u16 {
        self.text
            .lines()
            .nth(self.cursor.y as usize)
            .map(|l| l.len_chars())
            .unwrap_or_default() as u16
    }

    /// Retrieves internal text
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Retrieves current cursor position
    pub fn cursor(&self) -> Offset {
        self.cursor
    }

    /// Moves internal cursor left
    pub fn move_left(&mut self) {
        if self.cursor.x > 0 {
            self.cursor.x -= 1;
        } else if self.cursor.y > 0 {
            self.cursor.y -= 1;
            self.cursor.x = self.current_line_length();
        }
    }

    /// Moves internal cursor right
    pub fn move_right(&mut self) {
        if self.cursor.x < self.current_line_length() {
            self.cursor.x += 1;
        } else if self.cursor.y < (self.lines_count() - 1) {
            self.cursor.y += 1;
            self.cursor.x = 0;
        }
    }

    /// Moves internal cursor up
    pub fn move_up(&mut self) {
        if self.cursor.y > 0 {
            self.cursor.y -= 1;
            self.cursor.x = self.cursor.x.clamp(0, self.current_line_length());
        }
    }

    /// Moves internal cursor down
    pub fn move_down(&mut self) {
        if self.cursor.y < (self.lines_count() - 1) {
            self.cursor.y += 1;
            self.cursor.x = self.cursor.x.clamp(0, self.current_line_length());
        }
    }

    /// Moves internal cursor to the line beginning
    pub fn move_beginning(&mut self) {
        self.cursor.x = 0;
    }

    /// Moves internal cursor to the line end
    pub fn move_end(&mut self) {
        self.cursor.x = self.current_line_length();
    }

    /// Inserts the given text at the internal cursor
    pub fn insert_text(&mut self, text: impl Into<String>) {
        let text = unify_newlines(text.into());
        let text_lines = format!("{}-", text).lines().count().max(1);
        let last_line_len = text.lines().nth(text_lines - 1).map(str::len_chars).unwrap_or_default() as u16;
        let char_idx = self.char_idx_at_cursor();
        self.text.insert_safe_str(char_idx, text);
        self.cursor.x = if text_lines > 1 {
            last_line_len
        } else {
            self.cursor.x + last_line_len
        };
        self.cursor.y += text_lines as u16 - 1;
    }

    /// Inserts a newline
    pub fn insert_newline(&mut self) {
        let char_idx = self.char_idx_at_cursor();
        self.text.insert_safe(char_idx, '\n');
        self.cursor.y += 1;
        self.cursor.x = 0;
    }

    /// Inserts the given char at the internal cursor
    pub fn insert_char(&mut self, c: char) {
        if c == '\n' || c == 0xA as char {
            self.insert_newline()
        } else {
            let char_idx = self.char_idx_at_cursor();
            self.text.insert_safe(char_idx, c);
            self.cursor.x += 1;
        }
    }

    /// Deletes the char at the internal cursor and returns if any char was deleted
    pub fn delete_char(&mut self, backspace: bool) -> bool {
        if self.text.is_empty() {
            return false;
        }

        match backspace {
            // Backspace
            true => {
                let char_idx = self.char_idx_at_cursor();
                if char_idx > 0 {
                    let mut prev_line_length = 0;
                    if self.cursor.y > 0 {
                        self.cursor.y -= 1;
                        prev_line_length = self.current_line_length();
                        self.cursor.y += 1;
                    }
                    self.text.remove_safe(char_idx - 1);
                    if self.cursor.x == 0 {
                        self.cursor.y -= 1;
                        self.cursor.x = prev_line_length;
                    } else {
                        self.cursor.x -= 1;
                    }
                    true
                } else {
                    false
                }
            }
            // Delete
            false => {
                let char_idx = self.char_idx_at_cursor();
                let text_len = self.text.len_chars();
                if char_idx < text_len {
                    self.text.remove_safe(char_idx);
                    true
                } else {
                    false
                }
            }
        }
    }
}

impl Display for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for TextInput {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for TextInput {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl<'a> From<&'a TextInput> for Cow<'a, str> {
    fn from(value: &'a TextInput) -> Self {
        value.as_str().into()
    }
}

impl<'a> IntoCursorWidget<Text<'a>> for &'a TextInput {
    fn into_widget_and_cursor(self, _theme: Theme) -> (Text<'a>, Option<(Offset, Area)>) {
        (self.as_str().into(), Some((self.cursor(), Area::default_visible())))
    }
}
