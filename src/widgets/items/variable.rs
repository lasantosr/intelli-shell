use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::Style,
    text::Line,
    widgets::Widget,
};

use crate::{
    config::Theme,
    model::{VariableSuggestion, VariableValue},
    utils::format_env_var,
    widgets::{CustomListItem, CustomTextArea},
};

const SECRET_VARIABLE_TITLE: &str = "(secret)";
const NEW_VARIABLE_TITLE: &str = "(new)";
const EDIT_VARIABLE_TITLE: &str = "(edit)";

/// An identifier for a [`VariableSuggestionItem`]
#[derive(PartialEq, Eq)]
pub enum VariableSuggestionItemIdentifier {
    New,
    Previous(String),
    Environment(String),
    Existing(Option<i32>, String),
    Completion(String),
    Derived(String),
}

/// Actual item to be listed for a [`VariableSuggestion`], preserving editing state
#[derive(Clone)]
pub enum VariableSuggestionItem<'a> {
    New {
        sort_index: u8,
        is_secret: bool,
        textarea: CustomTextArea<'a>,
    },
    Previous {
        sort_index: u8,
        value: String,
        score: f64,
    },
    Environment {
        sort_index: u8,
        content: String,
        is_value: bool,
        score: f64,
    },
    Existing {
        sort_index: u8,
        value: VariableValue,
        score: f64,
        completion_merged: bool,
        editing: Option<CustomTextArea<'a>>,
    },
    Completion {
        sort_index: u8,
        value: String,
        score: f64,
    },
    Derived {
        sort_index: u8,
        value: String,
    },
}

impl<'a> VariableSuggestionItem<'a> {
    /// Retrieves an identifier for this item, two items are considered the same if their identifiers are equal
    pub fn identifier(&self) -> VariableSuggestionItemIdentifier {
        match self {
            VariableSuggestionItem::New { .. } => VariableSuggestionItemIdentifier::New,
            VariableSuggestionItem::Previous { value, .. } => VariableSuggestionItemIdentifier::Previous(value.clone()),
            VariableSuggestionItem::Environment { content, .. } => {
                VariableSuggestionItemIdentifier::Environment(content.clone())
            }
            VariableSuggestionItem::Existing { value, .. } => {
                VariableSuggestionItemIdentifier::Existing(value.id, value.value.clone())
            }
            VariableSuggestionItem::Completion { value, .. } => {
                VariableSuggestionItemIdentifier::Completion(value.clone())
            }
            VariableSuggestionItem::Derived { value, .. } => VariableSuggestionItemIdentifier::Derived(value.clone()),
        }
    }

    /// On [Existing](VariableSuggestionItem::Existing) variants, enter edit mode if not already entered
    pub fn enter_edit_mode(&mut self) {
        if let VariableSuggestionItem::Existing { value, editing, .. } = self
            && editing.is_none()
        {
            *editing = Some(
                CustomTextArea::new(Style::default(), true, false, value.value.clone())
                    .title(EDIT_VARIABLE_TITLE)
                    .focused(),
            );
        }
    }

    pub fn sort_index(&self) -> u8 {
        match self {
            VariableSuggestionItem::New { sort_index, .. }
            | VariableSuggestionItem::Previous { sort_index, .. }
            | VariableSuggestionItem::Environment { sort_index, .. }
            | VariableSuggestionItem::Existing { sort_index, .. }
            | VariableSuggestionItem::Completion { sort_index, .. }
            | VariableSuggestionItem::Derived { sort_index, .. } => *sort_index,
        }
    }

    pub fn score(&self) -> f64 {
        match self {
            VariableSuggestionItem::Previous { score, .. }
            | VariableSuggestionItem::Environment { score, .. }
            | VariableSuggestionItem::Existing { score, .. }
            | VariableSuggestionItem::Completion { score, .. } => *score,
            _ => 0.0,
        }
    }
}

impl<'a> From<(u8, VariableSuggestion, f64)> for VariableSuggestionItem<'a> {
    fn from((sort_index, suggestion, score): (u8, VariableSuggestion, f64)) -> Self {
        match suggestion {
            VariableSuggestion::Secret => Self::New {
                sort_index,
                is_secret: true,
                textarea: CustomTextArea::new(Style::default(), true, false, "")
                    .title(SECRET_VARIABLE_TITLE)
                    .focused(),
            },
            VariableSuggestion::New => Self::New {
                sort_index,
                is_secret: false,
                textarea: CustomTextArea::new(Style::default(), true, false, "")
                    .title(NEW_VARIABLE_TITLE)
                    .focused(),
            },
            VariableSuggestion::Previous(value) => Self::Previous {
                sort_index,
                value,
                score,
            },
            VariableSuggestion::Environment { env_var_name, value } => {
                if let Some(value) = value {
                    Self::Environment {
                        sort_index,
                        content: value,
                        is_value: true,
                        score,
                    }
                } else {
                    Self::Environment {
                        sort_index,
                        content: format_env_var(env_var_name),
                        is_value: false,
                        score,
                    }
                }
            }
            VariableSuggestion::Existing(value) => Self::Existing {
                sort_index,
                value,
                score,
                completion_merged: false,
                editing: None,
            },
            VariableSuggestion::Completion(value) => Self::Completion {
                sort_index,
                value,
                score,
            },
            VariableSuggestion::Derived(value) => Self::Derived { sort_index, value },
        }
    }
}

impl<'i> CustomListItem for VariableSuggestionItem<'i> {
    type Widget<'w>
        = VariableSuggestionItemWidget<'w>
    where
        'i: 'w;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        match self {
            VariableSuggestionItem::New { textarea, .. }
            | VariableSuggestionItem::Existing {
                editing: Some(textarea),
                ..
            } => {
                let style = match (is_highlighted, is_discarded) {
                    (true, true) => theme.highlight_secondary_full(),
                    (true, false) => theme.highlight_primary_full(),
                    (false, true) => theme.secondary,
                    (false, false) => theme.primary,
                };

                let mut ta_render = textarea.clone();
                ta_render.set_focus(is_highlighted);
                ta_render.set_style(style);
                (
                    VariableSuggestionItemWidget(VariableSuggestionItemWidgetInner::TextArea(ta_render)),
                    Size::new(10, 1),
                )
            }
            VariableSuggestionItem::Existing {
                value: VariableValue { value: text, .. },
                ..
            }
            | VariableSuggestionItem::Previous { value: text, .. }
            | VariableSuggestionItem::Environment { content: text, .. }
            | VariableSuggestionItem::Completion { value: text, .. }
            | VariableSuggestionItem::Derived { value: text, .. } => {
                let (line, size) = text.as_widget(theme, inline, is_highlighted, is_discarded);
                (
                    VariableSuggestionItemWidget(VariableSuggestionItemWidgetInner::Literal(line)),
                    size,
                )
            }
        }
    }
}

/// Widget for a [`VariableSuggestionItem`]
pub struct VariableSuggestionItemWidget<'a>(VariableSuggestionItemWidgetInner<'a>);
#[allow(clippy::large_enum_variant)]
enum VariableSuggestionItemWidgetInner<'a> {
    TextArea(CustomTextArea<'a>),
    Literal(Line<'a>),
}

impl<'a> Widget for VariableSuggestionItemWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        match self.0 {
            VariableSuggestionItemWidgetInner::TextArea(ta) => ta.render(area, buf),
            VariableSuggestionItemWidgetInner::Literal(l) => l.render(area, buf),
        }
    }
}
