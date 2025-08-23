use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
};

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Text,
    widgets::{Block, Borders, Paragraph, Widget},
};
use tui_textarea::{CursorMove, TextArea};

use crate::utils::remove_newlines;

const SPINNER_CHARS: [char; 6] = ['✸', '✷', '✹', '✺', '✹', '✷'];
const DEFAULT_STYLE: Style = Style::new();

/// A custom text area widget
#[derive(Clone)]
pub struct CustomTextArea<'a> {
    inline: bool,
    inline_title: Option<Text<'a>>,
    textarea: TextArea<'a>,
    cursor_style: Style,
    focus: bool,
    multiline: bool,
    ai_loading: bool,
    spinner_state: usize,
    original_title: Option<Cow<'a, str>>,
}

impl<'a> CustomTextArea<'a> {
    /// Creates a new custom text area
    pub fn new(style: impl Into<Style>, inline: bool, multiline: bool, text: impl Into<String>) -> Self {
        let style = style.into();
        let cursor_style = style.add_modifier(Modifier::REVERSED);
        let cursor_line_style = style;

        let text = text.into();
        let mut textarea = if multiline {
            TextArea::from(
                text.split('\n')
                    .map(|s| s.strip_suffix('\r').unwrap_or(s).to_string())
                    .collect::<Vec<_>>(),
            )
        } else {
            TextArea::from([remove_newlines(text)])
        };
        textarea.set_style(style);
        textarea.set_cursor_style(DEFAULT_STYLE);
        textarea.set_cursor_line_style(cursor_line_style);
        textarea.move_cursor(CursorMove::Jump(u16::MAX, u16::MAX));
        if !inline {
            textarea.set_block(Block::default().borders(Borders::ALL).style(style));
        }

        Self {
            inline,
            inline_title: None,
            textarea,
            cursor_style,
            focus: false,
            multiline,
            ai_loading: false,
            spinner_state: 0,
            original_title: None,
        }
    }

    /// Updates the title of the text area
    pub fn title(mut self, title: impl Into<Cow<'a, str>>) -> Self {
        self.set_title(title);
        self
    }

    /// Updates the text area to be focused
    pub fn focused(mut self) -> Self {
        self.set_focus(true);
        self
    }

    /// Updates the textarea mask char
    pub fn secret(mut self, secret: bool) -> Self {
        self.set_secret(secret);
        self
    }

    /// Returns whether the text area supports multiple lines or not
    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    /// Returns whether the text area is currently focused or not
    pub fn is_focused(&self) -> bool {
        self.focus
    }

    /// Sets or clear the the text area mask char
    pub fn set_secret(&mut self, secret: bool) {
        if secret {
            self.textarea.set_mask_char('●');
        } else {
            self.textarea.clear_mask_char();
        }
    }

    /// Sets the focus state of the text area
    pub fn set_focus(&mut self, focus: bool) {
        if focus != self.focus {
            self.focus = focus;
            if self.focus {
                self.textarea.set_cursor_style(self.cursor_style);
            } else {
                self.textarea.set_cursor_style(DEFAULT_STYLE);
            }
        }
    }

    /// Updates the title of the text area
    pub fn set_title(&mut self, new_title: impl Into<Cow<'a, str>>) {
        let new_title = new_title.into();
        self.original_title = Some(new_title.clone());
        let style = self.textarea.style();

        if self.inline {
            self.inline_title = Some(Text::from(new_title).style(style));
        } else {
            let title_content = if self.ai_loading {
                let spinner_char = SPINNER_CHARS[self.spinner_state];
                Cow::from(format!("{new_title}{spinner_char}  "))
            } else {
                new_title
            };
            let new_block = Block::default().borders(Borders::ALL).style(style).title(title_content);
            self.textarea.set_block(new_block);
        }
    }

    /// Updates the style of this text area
    pub fn set_style(&mut self, style: impl Into<Style>) {
        let style = style.into();
        self.cursor_style = style.add_modifier(Modifier::REVERSED);

        self.textarea.set_style(style);
        self.textarea
            .set_cursor_style(if self.focus { self.cursor_style } else { DEFAULT_STYLE });
        self.textarea.set_cursor_line_style(style);

        if let Some(ref mut inline_title) = self.inline_title {
            *inline_title = inline_title.clone().style(style);
        } else if let Some(block) = self.textarea.block().cloned() {
            self.textarea.set_block(block.style(style));
        }
    }

    /// Sets the AI loading state, showing or hiding a spinner
    pub fn set_ai_loading(&mut self, loading: bool) {
        self.ai_loading = loading;
        if !loading {
            self.spinner_state = 0;
            if !self.inline
                && let Some(title) = self.original_title.clone()
            {
                let style = self.textarea.style();
                let new_block = Block::default().borders(Borders::ALL).style(style).title(title);
                self.textarea.set_block(new_block);
            }
        }
    }

    /// Checks if the textarea has the ai loading state
    pub fn is_ai_loading(&self) -> bool {
        self.ai_loading
    }

    /// Advances the spinner animation if active
    pub fn tick(&mut self) {
        if self.ai_loading {
            self.spinner_state = (self.spinner_state + 1) % SPINNER_CHARS.len();
            if !self.inline
                && let Some(title) = &self.original_title
            {
                let style = self.textarea.style();
                let spinner_char = SPINNER_CHARS[self.spinner_state];
                let new_title = format!("{title}{spinner_char}  ");
                let new_block = Block::default().borders(Borders::ALL).style(style).title(new_title);
                self.textarea.set_block(new_block);
            }
        }
    }

    /// Retrieves the current text in the text area as a single string
    pub fn lines_as_string(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Moves the cursor to the left, optionally by word
    pub fn move_cursor_left(&mut self, word: bool) {
        if self.focus && !self.ai_loading {
            self.textarea
                .move_cursor(if word { CursorMove::WordBack } else { CursorMove::Back });
        }
    }

    /// Moves the cursor to the right, optionally by word
    pub fn move_cursor_right(&mut self, word: bool) {
        if self.focus && !self.ai_loading {
            self.textarea.move_cursor(if word {
                CursorMove::WordForward
            } else {
                CursorMove::Forward
            });
        }
    }

    /// Moves the cursor to the head of the line, or the absolute head
    pub fn move_home(&mut self, absolute: bool) {
        if self.focus && !self.ai_loading {
            self.textarea.move_cursor(if absolute {
                CursorMove::Jump(0, 0)
            } else {
                CursorMove::Head
            });
        }
    }

    /// Moves the cursor to the end of the line, or the absolute end
    pub fn move_end(&mut self, absolute: bool) {
        if self.focus && !self.ai_loading {
            self.textarea.move_cursor(if absolute {
                CursorMove::Jump(u16::MAX, u16::MAX)
            } else {
                CursorMove::End
            });
        }
    }

    /// Inserts a char at the current cursor position
    pub fn insert_char(&mut self, c: char) {
        if self.focus && !self.ai_loading && self.multiline || c != '\n' {
            self.textarea.insert_char(c);
        }
    }

    /// Inserts a text at the current cursor position
    pub fn insert_str<S>(&mut self, text: S)
    where
        S: AsRef<str>,
    {
        if self.focus && !self.ai_loading {
            if self.multiline {
                self.textarea.insert_str(text);
            } else {
                self.textarea.insert_str(remove_newlines(text.as_ref()));
            };
        }
    }

    /// Inserts a newline at the current cursor position, if multiline is enabled
    pub fn insert_newline(&mut self) {
        if self.focus && !self.ai_loading && self.multiline {
            self.textarea.insert_newline();
        }
    }

    /// Delete characters at the cursor position based on the backspace and word flags
    pub fn delete(&mut self, backspace: bool, word: bool) {
        if self.focus && !self.ai_loading {
            match (backspace, word) {
                (true, true) => self.textarea.delete_word(),
                (true, false) => self.textarea.delete_char(),
                (false, true) => self.textarea.delete_next_word(),
                (false, false) => self.textarea.delete_next_char(),
            };
        }
    }
}

impl<'a> Widget for &CustomTextArea<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if let Some(ref inline_title) = self.inline_title {
            if self.ai_loading {
                let layout = Layout::horizontal([
                    Constraint::Length(inline_title.width() as u16 + 1),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ]);
                let [title_area, spinner_area, textarea_area] = layout.areas(area);

                inline_title.render(title_area, buf);

                let spinner_char = SPINNER_CHARS[self.spinner_state];
                let spinner_widget = Paragraph::new(format!("{spinner_char}  ")).style(self.textarea.style());
                spinner_widget.render(spinner_area, buf);

                self.textarea.render(textarea_area, buf);
            } else {
                let layout =
                    Layout::horizontal([Constraint::Length(inline_title.width() as u16 + 1), Constraint::Min(1)]);
                let [title_area, textarea_area] = layout.areas(area);
                inline_title.render(title_area, buf);
                self.textarea.render(textarea_area, buf);
            }
        } else {
            self.textarea.render(area, buf);
        }
    }
}

impl<'a> Deref for CustomTextArea<'a> {
    type Target = TextArea<'a>;

    fn deref(&self) -> &Self::Target {
        &self.textarea
    }
}

impl<'a> DerefMut for CustomTextArea<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.textarea
    }
}
