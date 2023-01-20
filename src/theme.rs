use tui::style::Color;

pub const LIGHT: Theme = Theme {
    main: Color::Black,
    selected_background: Color::Gray,
    secondary: Color::Rgb(0, 128, 0),
};

pub const DARK: Theme = Theme {
    main: Color::White,
    selected_background: Color::Rgb(154, 154, 154),
    secondary: Color::Rgb(71, 105, 56),
};

#[derive(Clone, Copy)]
pub struct Theme {
    pub main: Color,
    pub selected_background: Color,
    pub secondary: Color,
}
