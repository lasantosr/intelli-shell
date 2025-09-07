use std::ops::{Deref, DerefMut};

use ratatui::layout::Size;

use crate::{
    config::Theme,
    model::VariableCompletion,
    widgets::{CustomListItem, VariableCompletionWidget},
};

/// Wrapper around `VariableCompletion` to be rendered on a plain style
#[derive(Clone)]
pub struct PlainStyleVariableCompletion(VariableCompletion);

impl CustomListItem for VariableCompletion {
    type Widget<'w> = VariableCompletionWidget<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        _inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        let widget = VariableCompletionWidget::new(self, theme, is_highlighted, is_discarded, false, false);
        let size = widget.size();
        (widget, size)
    }
}

impl CustomListItem for PlainStyleVariableCompletion {
    type Widget<'w> = VariableCompletionWidget<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        _inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        let widget = VariableCompletionWidget::new(self, theme, is_highlighted, is_discarded, true, true);
        let size = widget.size();
        (widget, size)
    }
}

impl Deref for PlainStyleVariableCompletion {
    type Target = VariableCompletion;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for PlainStyleVariableCompletion {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<VariableCompletion> for PlainStyleVariableCompletion {
    fn from(value: VariableCompletion) -> Self {
        PlainStyleVariableCompletion(value)
    }
}
impl From<PlainStyleVariableCompletion> for VariableCompletion {
    fn from(value: PlainStyleVariableCompletion) -> Self {
        value.0
    }
}
