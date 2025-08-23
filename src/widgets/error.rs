use ratatui::{Frame, layout::Rect, style::Style, text::Text, widgets::Clear};

use crate::config::Theme;

/// The number of ticks an error message will be displayed.
///
/// Calculated as 3 seconds * 10 ticks per second.
const ERROR_MESSAGE_DISPLAY_TICKS: u16 = 3 * 10;

/// Represents a popup for displaying error messages temporarily.
///
/// The error message is rendered at the bottom of a given area and disappears after a predefined timeout.
pub struct ErrorPopup<'a> {
    style: Style,
    message: Option<Text<'a>>,
    timeout_ticks: Option<u16>,
}

impl<'a> ErrorPopup<'a> {
    /// Creates a new, empty [`ErrorPopup`]
    pub fn empty(theme: &Theme) -> Self {
        Self {
            style: theme.error.into(),
            message: None,
            timeout_ticks: None,
        }
    }

    /// Sets or replaces the error message to be permanently displayed.
    ///
    /// The message will remain displayed until a temporary message is set ot the message is cleared.
    pub fn set_perm_message(&mut self, message: impl Into<Text<'a>>) {
        self.message = Some(message.into().centered().style(self.style));
    }

    /// Sets or replaces the error message to be temporarily displayed.
    ///
    /// The message will remain visible for a short period of time, after which it will disappear.
    pub fn set_temp_message(&mut self, message: impl Into<Text<'a>>) {
        self.message = Some(message.into().centered().style(self.style));
        self.timeout_ticks.replace(ERROR_MESSAGE_DISPLAY_TICKS);
    }

    /// Clears the error message.
    pub fn clear_message(&mut self) {
        self.message = None;
        self.timeout_ticks = None;
    }

    /// Advances the state of the error popup by one tick.
    ///
    /// If an error message is currently displayed, its timeout is decremented.
    /// If the timeout reaches zero, the message is cleared.
    pub fn tick(&mut self) {
        // Check if there's an active timer for the error message
        if let Some(mut remaining_ticks) = self.timeout_ticks {
            if remaining_ticks > 0 {
                remaining_ticks -= 1;
                self.timeout_ticks.replace(remaining_ticks);
            } else {
                // Timer has expired, clear the error message and the timer
                self.message = None;
                self.timeout_ticks = None;
            }
        }
    }

    /// Renders the error message popup within the given area if a message is active.
    ///
    /// The message is displayed as an overlay on the last line of the specified `area`.
    pub fn render_in(&mut self, frame: &mut Frame, area: Rect) {
        // Render the error message as an overlay, if it exists
        if let Some(ref text) = self.message {
            // Define the rectangle for the error message overlay: the last line of the component's total area
            let error_overlay_rect = Rect {
                x: area.x,
                y: area.bottom() - 1,
                width: area.width,
                height: 1,
            };

            // Clear the area behind it and render it
            frame.render_widget(Clear, error_overlay_rect);
            frame.render_widget(text, error_overlay_rect);
        }
    }
}
