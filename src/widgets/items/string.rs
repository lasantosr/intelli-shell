use std::ops::{Deref, DerefMut};

use ratatui::{layout::Size, text::Line};

use crate::{config::Theme, widgets::CustomListItem};

/// Wrapper around `String` to be rendered with `comment` style
#[derive(Clone)]
pub struct CommentString(String);

impl CustomListItem for CommentString {
    type Widget<'w> = Line<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        _inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        let style = match (is_highlighted, is_discarded) {
            (true, true) => theme.highlight_secondary_full(),
            (true, false) => theme.highlight_comment_full(),
            (false, true) => theme.secondary,
            (false, false) => theme.comment,
        };
        let line = Line::raw(&self.0).style(style);
        let width = line.width() as u16;
        (line, Size::new(width, 1))
    }
}

impl CustomListItem for str {
    type Widget<'w> = Line<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        _inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        let style = match (is_highlighted, is_discarded) {
            (true, true) => theme.highlight_secondary_full(),
            (true, false) => theme.highlight_primary_full(),
            (false, true) => theme.secondary,
            (false, false) => theme.primary,
        };
        let line = Line::raw(self).style(style);
        let width = line.width() as u16;
        (line, Size::new(width, 1))
    }
}

impl CustomListItem for String {
    type Widget<'w> = Line<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        self.as_str().as_widget(theme, inline, is_highlighted, is_discarded)
    }
}

impl Deref for CommentString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for CommentString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<String> for CommentString {
    fn from(value: String) -> Self {
        CommentString(value)
    }
}
impl From<CommentString> for String {
    fn from(value: CommentString) -> Self {
        value.0
    }
}
