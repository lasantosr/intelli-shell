/// Macro to format an error message with theme's error style.
///
/// # Examples
/// ```rust
/// # use intelli_shell::format_error;
/// # use intelli_shell::config::Theme;
/// # let theme = Theme::default();
/// let msg = format_error!(theme, "Invalid value");
/// let msg = format_error!(theme, "Invalid value: {}", 42);
/// ```
#[macro_export]
macro_rules! format_error {
    ($theme:expr, $($arg:tt)*) => {
        format!("{}{}", $theme.error.apply("[Error] "), format!($($arg)*))
    }
}

/// Macro to format an information message with theme's style.
///
/// # Examples
/// ```rust
/// # use intelli_shell::format_msg;
/// # use intelli_shell::config::Theme;
/// # let theme = Theme::default();
/// let msg = format_msg!(theme, "Succesful operation");
/// ```
#[macro_export]
macro_rules! format_msg {
    ($theme:expr, $($arg:tt)*) => {
        format!("{}{}", $theme.accent.apply("-> "), format!($($arg)*))
    }
}

/// Declares a `mod` and uses it
#[macro_export]
macro_rules! using {
    ($($v:vis $p:ident),* $(,)?) => {
        $(
            mod $p;
            $v use self::$p::*;
        )*
    }
}

using! {
    pub process,
    pub string,
    pub tags,
    pub fuzzy,
    pub variable,
}
