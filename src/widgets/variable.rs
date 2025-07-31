use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
};

use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::Style,
    text::Line,
    widgets::Widget,
};

use crate::{
    config::Theme,
    model::VariableValue,
    widgets::{AsWidget, CustomTextArea},
};

const SECRET_VARIABLE_TITLE: &str = "(secret)";
const NEW_VARIABLE_TITLE: &str = "(new)";
const EDIT_VARIABLE_TITLE: &str = "(edit)";

/// Represents a single row in a list of suggestions for a given variable's value
#[derive(Clone)]
pub enum VariableSuggestionRow<'a> {
    New(NewVariableValue<'a>),
    Environment(LiteralVariableValue<'a>, bool),
    Existing(ExistingVariableValue<'a>),
    Derived(LiteralVariableValue<'a>),
}

#[derive(Clone)]
pub struct NewVariableValue<'a> {
    highlighted: bool,
    primary: Style,
    highlight_primary: Style,
    secret: bool,
    textarea: CustomTextArea<'a>,
}

#[derive(Clone)]
pub struct ExistingVariableValue<'a> {
    highlighted: bool,
    primary: Style,
    highlight_primary: Style,
    pub value: VariableValue,
    pub editing: Option<CustomTextArea<'a>>,
}

#[derive(Clone)]
pub struct LiteralVariableValue<'a> {
    highlighted: bool,
    primary: Style,
    highlight_primary: Style,
    value: Cow<'a, str>,
}

impl<'a> NewVariableValue<'a> {
    pub fn new(theme: &Theme, secret: bool) -> Self {
        Self {
            highlighted: false,
            primary: theme.primary.into(),
            highlight_primary: theme.highlight_primary_full().into(),
            secret,
            textarea: CustomTextArea::new(theme.primary, true, false, "").title(if secret {
                SECRET_VARIABLE_TITLE
            } else {
                NEW_VARIABLE_TITLE
            }),
        }
    }

    pub fn is_secret(&self) -> bool {
        self.secret
    }
}

impl<'a> ExistingVariableValue<'a> {
    pub fn new(theme: &Theme, value: VariableValue) -> Self {
        Self {
            highlighted: false,
            primary: theme.primary.into(),
            highlight_primary: theme.highlight_primary_full().into(),
            value,
            editing: None,
        }
    }

    pub fn enter_edit_mode(&mut self) {
        if self.editing.is_none() {
            self.editing = Some(
                CustomTextArea::new(self.highlight_primary, true, false, self.value.value.clone())
                    .title(EDIT_VARIABLE_TITLE)
                    .focused(),
            );
        }
    }
}

impl<'a> LiteralVariableValue<'a> {
    pub fn new(theme: &Theme, value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            highlighted: false,
            primary: theme.primary.into(),
            highlight_primary: theme.highlight_primary_full().into(),
            value: value.into(),
        }
    }
}

impl<'a> Widget for &'a VariableSuggestionRow<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        match &self {
            VariableSuggestionRow::New(n) => n.render(area, buf),
            VariableSuggestionRow::Existing(e) => {
                if let Some(editing) = e.editing.as_ref() {
                    editing.render(area, buf);
                } else {
                    Line::from(e.value.value.as_str())
                        .style(if e.highlighted { e.highlight_primary } else { e.primary })
                        .render(area, buf);
                }
            }
            VariableSuggestionRow::Environment(l, _) | VariableSuggestionRow::Derived(l) => Line::from(l.as_ref())
                .style(if l.highlighted { l.highlight_primary } else { l.primary })
                .render(area, buf),
        }
    }
}

impl<'a> AsWidget for VariableSuggestionRow<'a> {
    fn set_highlighted(&mut self, is_highlighted: bool) {
        match self {
            VariableSuggestionRow::New(n) => {
                n.highlighted = is_highlighted;
                let style = if is_highlighted { n.highlight_primary } else { n.primary };
                n.set_focus(is_highlighted);
                n.set_style(style);
            }
            VariableSuggestionRow::Existing(e) => {
                e.highlighted = is_highlighted;
                if let Some(ta) = e.editing.as_mut() {
                    let style = if is_highlighted { e.highlight_primary } else { e.primary };
                    ta.set_focus(is_highlighted);
                    ta.set_style(style);
                }
            }
            VariableSuggestionRow::Environment(l, _) | VariableSuggestionRow::Derived(l) => {
                l.highlighted = is_highlighted;
            }
        }
    }

    fn as_widget<'b>(&'b self, _is_highlighted: bool) -> (impl Widget + 'b, Size) {
        (self, Size::new(10, 1))
    }
}

impl<'a> From<NewVariableValue<'a>> for CustomTextArea<'a> {
    fn from(val: NewVariableValue<'a>) -> Self {
        val.textarea
    }
}
impl<'a> Deref for NewVariableValue<'a> {
    type Target = CustomTextArea<'a>;

    fn deref(&self) -> &Self::Target {
        &self.textarea
    }
}
impl<'a> DerefMut for NewVariableValue<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.textarea
    }
}

impl<'a> From<ExistingVariableValue<'a>> for VariableValue {
    fn from(val: ExistingVariableValue<'a>) -> Self {
        val.value
    }
}

impl<'a> From<LiteralVariableValue<'a>> for Cow<'a, str> {
    fn from(val: LiteralVariableValue<'a>) -> Self {
        val.value
    }
}
impl<'a> Deref for LiteralVariableValue<'a> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref()
    }
}
