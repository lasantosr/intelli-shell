use std::ops::{Deref, DerefMut};

use ratatui::layout::Size;

use crate::{
    config::Theme,
    model::Command,
    widgets::{CommandWidget, CustomListItem},
};

/// Wrapper around `Command` to be rendered on a plain style
#[derive(Clone)]
pub struct PlainStyleCommand(Command);

impl CustomListItem for Command {
    type Widget<'w> = CommandWidget<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        let widget = CommandWidget::new(self, theme, inline, is_highlighted, is_discarded, false);
        let size = widget.size();
        (widget, size)
    }
}

impl CustomListItem for PlainStyleCommand {
    type Widget<'w> = CommandWidget<'w>;

    fn as_widget<'a>(
        &'a self,
        theme: &Theme,
        inline: bool,
        is_highlighted: bool,
        is_discarded: bool,
    ) -> (Self::Widget<'a>, Size) {
        let widget = CommandWidget::new(self, theme, inline, is_highlighted, is_discarded, true);
        let size = widget.size();
        (widget, size)
    }
}

impl Deref for PlainStyleCommand {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for PlainStyleCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<Command> for PlainStyleCommand {
    fn from(value: Command) -> Self {
        PlainStyleCommand(value)
    }
}
impl From<PlainStyleCommand> for Command {
    fn from(value: PlainStyleCommand) -> Self {
        value.0
    }
}
