use itertools::Itertools;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::ListItem,
};

use super::{Area, IntoCursorWidget, Offset, TextInput};
use crate::{
    common::StrExt,
    model::{CommandPart, LabelSuggestion, LabeledCommand},
    theme::Theme,
};

const SECRET_LABEL_PREFIX: &str = "(secret) ";
const NEW_LABEL_PREFIX: &str = "(new) ";
const EDIT_LABEL_PREFIX: &str = "(edit) ";

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum LabelSuggestionItem {
    Secret(TextInput),
    New(TextInput),
    Label(String),
    Persisted(LabelSuggestion, Option<TextInput>),
}

impl<'a> IntoCursorWidget<ListItem<'a>> for &'a LabelSuggestionItem {
    fn into_widget_and_cursor(self, theme: Theme) -> (ListItem<'a>, Option<(Offset, Area)>) {
        match self {
            LabelSuggestionItem::Secret(value) => (
                ListItem::new(Line::from(vec![
                    Span::styled(
                        SECRET_LABEL_PREFIX,
                        Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC),
                    ),
                    Span::raw(value.as_str()),
                ])),
                Some((
                    value.cursor() + Offset::new(SECRET_LABEL_PREFIX.len() as u16, 0),
                    Area::default_visible(),
                )),
            ),
            LabelSuggestionItem::New(value) => (
                ListItem::new(Line::from(vec![
                    Span::styled(
                        NEW_LABEL_PREFIX,
                        Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC),
                    ),
                    Span::raw(value.as_str()),
                ])),
                Some((
                    value.cursor() + Offset::new(NEW_LABEL_PREFIX.len() as u16, 0),
                    Area::default_visible(),
                )),
            ),
            LabelSuggestionItem::Label(value) => (ListItem::new(value.as_str()), None),
            LabelSuggestionItem::Persisted(e, input) => match input {
                Some(value) => (
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            EDIT_LABEL_PREFIX,
                            Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC),
                        ),
                        Span::raw(value.as_str()),
                    ])),
                    Some((
                        value.cursor() + Offset::new(EDIT_LABEL_PREFIX.len() as u16, 0),
                        Area::default_visible(),
                    )),
                ),
                None => (ListItem::new(e.suggestion.as_str()), None),
            },
        }
    }
}

impl<'a> IntoCursorWidget<Text<'a>> for &'a LabeledCommand {
    fn into_widget_and_cursor(self, theme: Theme) -> (Text<'a>, Option<(Offset, Area)>) {
        let mut first_label_found = false;
        let mut first_label_offset_x = 0;
        let mut first_label_width = 0;

        let text = Line::from(
            self.parts
                .iter()
                .map(|p| {
                    let span = match p {
                        CommandPart::Text(t) | CommandPart::LabelValue(t) => {
                            Span::styled(t, Style::default().fg(theme.secondary))
                        }
                        CommandPart::Label(l) => {
                            let style = if !first_label_found {
                                first_label_found = true;
                                first_label_width = l.len_chars() as u16 + 4;
                                Style::default().add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.secondary)
                            };
                            Span::styled(format!("{{{{{l}}}}}"), style)
                        }
                    };
                    if !first_label_found {
                        first_label_offset_x += span.width() as u16;
                    }
                    span
                })
                .collect_vec(),
        )
        .into();

        (
            text,
            if first_label_found {
                Some((
                    Offset::new(first_label_offset_x, 0),
                    Area::default_visible().min_width(first_label_width),
                ))
            } else {
                None
            },
        )
    }
}
