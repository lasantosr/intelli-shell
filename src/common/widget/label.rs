use itertools::Itertools;
use tui::{
    style::{Modifier, Style},
    text::{Span, Spans, Text},
    widgets::ListItem,
};

use super::{Area, IntoCursorWidget, Offset, TextInput};
use crate::{
    common::StrExt,
    model::{CommandPart, LabelSuggestion, LabeledCommand},
    theme::Theme,
};

pub const NEW_LABEL_PREFIX: &str = "(new) ";

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum LabelSuggestionItem {
    New(TextInput),
    Label(String),
    Persisted(LabelSuggestion),
}

impl<'a> From<&'a LabelSuggestionItem> for ListItem<'a> {
    fn from(item: &'a LabelSuggestionItem) -> Self {
        match item {
            LabelSuggestionItem::New(value) => ListItem::new(Spans::from(vec![
                Span::styled(NEW_LABEL_PREFIX, Style::default().add_modifier(Modifier::ITALIC)),
                Span::raw(value.as_str()),
            ])),
            LabelSuggestionItem::Label(value) => ListItem::new(value.clone()),
            LabelSuggestionItem::Persisted(e) => ListItem::new(e.suggestion.clone()),
        }
    }
}

impl<'a> IntoCursorWidget<Text<'a>> for &'a LabeledCommand {
    fn into_widget_and_cursor(self, theme: Theme) -> (Text<'a>, Option<(Offset, Area)>) {
        let mut first_label_found = false;
        let mut first_label_offset_x = 0;
        let mut first_label_width = 0;

        let text = Spans::from(
            self.parts
                .iter()
                .map(|p| {
                    let span = match p {
                        CommandPart::Text(t) | CommandPart::LabelValue(t) => {
                            Span::styled(t, Style::default().fg(theme.disabled))
                        }
                        CommandPart::Label(l) => {
                            let style = if !first_label_found {
                                first_label_found = true;
                                first_label_width = l.len_chars() as u16 + 4;
                                Style::default().fg(theme.main).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.disabled)
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
