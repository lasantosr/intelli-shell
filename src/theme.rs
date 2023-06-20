use ratatui::style::Color;

pub const LIGHT: Theme = Theme {
    secondary: Color::Gray,
    selected_background: Color::Gray,
    alias: Color::Yellow,
    description: Color::Rgb(0, 128, 0),
};

pub const DARK: Theme = Theme {
    secondary: Color::Rgb(154, 154, 154),
    selected_background: Color::Rgb(154, 154, 154),
    alias: Color::Yellow,
    description: Color::Rgb(71, 105, 56),
};

#[derive(Clone, Copy)]
pub struct Theme {
    pub secondary: Color,
    pub selected_background: Color,
    pub alias: Color,
    pub description: Color,
}
