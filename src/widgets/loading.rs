use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Clear, Paragraph},
};

use crate::config::Theme;

/// The characters for the spinner animation
pub const SPINNER_CHARS: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A widget to display a centered loading spinner and message as an overlay
pub struct LoadingSpinner<'a> {
    /// The current state of the loading spinner animation
    spinner_state: usize,
    /// The style for the spinner text
    style: Style,
    /// Optional message to display with the spinner
    message: Option<Cow<'a, str>>,
}

impl<'a> LoadingSpinner<'a> {
    /// Creates a new [`LoadingSpinner`] styled according to the provided theme
    pub fn new(theme: &Theme) -> Self {
        Self {
            spinner_state: 0,
            style: theme.primary.into(),
            message: None,
        }
    }

    /// Sets or replaces the message to be displayed with the spinner
    pub fn with_message(mut self, message: impl Into<Cow<'a, str>>) -> Self {
        self.set_message(message);
        self
    }

    /// Sets or replaces the message to be displayed with the spinner
    pub fn set_message(&mut self, message: impl Into<Cow<'a, str>>) {
        self.message = Some(message.into());
    }

    /// Advances the spinner animation by one tick
    pub fn tick(&mut self) {
        self.spinner_state = (self.spinner_state + 1) % SPINNER_CHARS.len();
    }

    /// Renders the loading spinner in the center of the given area
    pub fn render_in(&self, frame: &mut Frame, area: Rect) {
        let spinner_char = SPINNER_CHARS[self.spinner_state];
        let loading_text = if let Some(ref msg) = self.message {
            format!("{spinner_char} {msg}")
        } else {
            spinner_char.to_string()
        };

        // Create the main paragraph widget
        let loading_paragraph = Paragraph::new(loading_text).style(self.style);

        // Clear the entire area before rendering
        frame.render_widget(Clear, area);
        frame.render_widget(loading_paragraph, area);
    }
}
