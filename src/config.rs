use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
};

use color_eyre::{
    Result,
    eyre::{Context, ContextCompat, eyre},
};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Attribute, Attributes, Color, ContentStyle},
};
use directories::ProjectDirs;
use itertools::Itertools;
use serde::{
    Deserialize,
    de::{Deserializer, Error},
};

use crate::model::SearchMode;

/// Main configuration struct for the application
#[derive(Clone, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct Config {
    /// Directory where the data must be stored
    pub data_dir: PathBuf,
    /// Whether to check for updates
    pub check_updates: bool,
    /// Whether the TUI must be rendered "inline" below the shell prompt
    pub inline: bool,
    /// Configuration for the search command
    pub search: SearchConfig,
    /// Configuration settings for application logging
    pub logs: LogsConfig,
    /// Configuration for the key bindings used within the TUI
    pub keybindings: KeyBindingsConfig,
    /// Configuration for the visual theme of the TUI
    pub theme: Theme,
    /// Configuration for the default gist when importing or exporting
    pub gist: GistConfig,
    /// Configuration to tune the search algorithm
    pub tuning: SearchTuning,
}

/// Configuration for the search command
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchConfig {
    /// The delay (in ms) to wait and accumulate type events before triggering the query
    pub delay: u64,
    /// The default search mode
    pub mode: SearchMode,
    /// Whether to search for user commands only by default (excluding tldr)
    pub user_only: bool,
}

/// Configuration settings for application logging
#[derive(Clone, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct LogsConfig {
    /// Whether application logging is enabled
    pub enabled: bool,
    /// The log filter to apply, controlling which logs are recorded.
    ///
    /// This string supports the `tracing-subscriber`'s environment filter syntax.
    pub filter: String,
}

/// Configuration for the key bindings used in the Terminal User Interface (TUI).
///
/// This struct holds the `KeyBinding` instances for various actions within the application's TUI, allowing users to
/// customize their interaction with the interface.
#[derive(Clone, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct KeyBindingsConfig(
    #[serde(deserialize_with = "deserialize_bindings_with_defaults")] BTreeMap<KeyBindingAction, KeyBinding>,
);

/// Represents the distinct actions within the application that can be configured with specific key bindings
#[derive(Copy, Clone, Deserialize, PartialOrd, PartialEq, Eq, Ord, Debug)]
#[cfg_attr(test, derive(strum::EnumIter))]
#[serde(rename_all = "snake_case")]
pub enum KeyBindingAction {
    /// Exit the TUI gracefully
    Quit,
    /// Update the currently highlighted record or item
    Update,
    /// Delete the currently highlighted record or item
    Delete,
    /// Confirm a selection or action related to the highlighted record
    Confirm,
    /// Execute the action associated with the highlighted record or item
    Execute,
    /// Toggle the search mode
    SearchMode,
    /// Toggle whether to search for user commands only or include tldr's
    SearchUserOnly,
}

/// Represents a single logical key binding that can be triggered by one or more physical `KeyEvent`s.
///
/// Internally, it is stored as a `Vec<KeyEvent>` because multiple different key press combinations can map to the same
/// action.
#[derive(Clone, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
pub struct KeyBinding(#[serde(deserialize_with = "deserialize_key_events")] Vec<KeyEvent>);

/// TUI theme configuration.
///
/// Defines the colors, styles, and highlighting behavior for the Terminal User Interface.
#[derive(Clone, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct Theme {
    /// To be used as the primary style, like for selected items or main text
    #[serde(deserialize_with = "deserialize_style")]
    pub primary: ContentStyle,
    /// To be used as the secondary style, like for unselected items or less important text
    #[serde(deserialize_with = "deserialize_style")]
    pub secondary: ContentStyle,
    /// Accent style, typically used for highlighting specific elements like aliases or important keywords
    #[serde(deserialize_with = "deserialize_style")]
    pub accent: ContentStyle,
    /// Style for comments or less prominent information
    #[serde(deserialize_with = "deserialize_style")]
    pub comment: ContentStyle,
    /// Style for errors
    #[serde(deserialize_with = "deserialize_style")]
    pub error: ContentStyle,
    /// Optional background color for highlighted items
    #[serde(deserialize_with = "deserialize_color")]
    pub highlight: Option<Color>,
    /// The symbol displayed next to a highlighted item
    pub highlight_symbol: String,
    /// Primary style applied when an item is highlighted
    #[serde(deserialize_with = "deserialize_style")]
    pub highlight_primary: ContentStyle,
    /// Secondary style applied when an item is highlighted
    #[serde(deserialize_with = "deserialize_style")]
    pub highlight_secondary: ContentStyle,
    /// Accent style applied when an item is highlighted
    #[serde(deserialize_with = "deserialize_style")]
    pub highlight_accent: ContentStyle,
    /// Comments style applied when an item is highlighted
    #[serde(deserialize_with = "deserialize_style")]
    pub highlight_comment: ContentStyle,
}

/// Configuration settings for the default gist
#[derive(Clone, Default, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
pub struct GistConfig {
    /// Gist unique identifier
    pub id: String,
    /// Authentication token to use when writing to the gist
    pub token: String,
}

/// Holds all tunable parameters for the command and variable search ranking algorithms
#[derive(Clone, Copy, Default, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchTuning {
    /// Configuration for the command search ranking
    pub commands: SearchCommandTuning,
    /// Configuration for the variable values ranking
    pub variables: SearchVariableTuning,
}

/// Configures the ranking parameters for command search
#[derive(Clone, Copy, Default, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchCommandTuning {
    /// Defines weights and points for the text relevance component
    pub text: SearchCommandsTextTuning,
    /// Defines weights and points for the path-aware usage component
    pub path: SearchPathTuning,
    /// Defines points for the total usage component
    pub usage: SearchUsageTuning,
}

/// Defines weights and points for the text relevance (FTS) score component
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchCommandsTextTuning {
    /// Points assigned to the normalized text relevance score in the final calculation
    pub points: u32,
    /// Weight for the command within the FTS bm25 calculation
    pub command: f64,
    /// Weight for the description field within the FTS bm25 calculation
    pub description: f64,
    /// Specific weights for the different strategies within the 'auto' search algorithm
    pub auto: SearchCommandsTextAutoTuning,
}

/// Tunable weights for the different matching strategies within the 'auto' search mode
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchCommandsTextAutoTuning {
    /// Weight multiplier for results from the prefix-based FTS query
    pub prefix: f64,
    /// Weight multiplier for results from the fuzzy, all-words-match FTS query
    pub fuzzy: f64,
    /// Weight multiplier for results from the relaxed, any-word-match FTS query
    pub relaxed: f64,
    /// Boost multiplier to add when the first search term matches the start of the command's text
    pub root: f64,
}

/// Configures the path-aware scoring model
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchPathTuning {
    /// Points assigned to the normalized path score in the final calculation
    pub points: u32,
    /// Weight for a usage record that matches the current working directory exactly
    pub exact: f64,
    /// Weight for a usage record from an ancestor (parent) directory
    pub ancestor: f64,
    /// Weight for a usage record from a descendant (child) directory
    pub descendant: f64,
    /// Weight for a usage record from any other unrelated path
    pub unrelated: f64,
}

/// Configures the total usage scoring model
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchUsageTuning {
    /// Points assigned to the normalized total usage in the final calculation
    pub points: u32,
}

/// Configures the ranking parameters for variable values ranking
#[derive(Clone, Copy, Default, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchVariableTuning {
    /// Defines points for the context relevance component
    pub context: SearchVariableContextTuning,
    /// Defines weights and points for the path-aware usage component
    pub path: SearchPathTuning,
}

/// Defines points for the context relevance score component of variable values
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, PartialEq))]
#[cfg_attr(not(test), serde(default))]
pub struct SearchVariableContextTuning {
    /// Points assigned for matching contextual information (e.g. other selected values)
    pub points: u32,
}

impl Config {
    /// Initializes the application configuration.
    ///
    /// Attempts to load the configuration from the user's config directory (`config.toml`). If the file does not exist
    /// or has missing fields, it falls back to default values.
    pub fn init(config_file: Option<PathBuf>) -> Result<Self> {
        // Initialize directories
        let proj_dirs = ProjectDirs::from("org", "IntelliShell", "Intelli-Shell")
            .wrap_err("Couldn't initialize project directory")?;
        let config_dir = proj_dirs.config_dir().to_path_buf();

        // Initialize the config
        let config_path = config_file.unwrap_or_else(|| config_dir.join("config.toml"));
        let mut config = if config_path.exists() {
            // Read from the config file, if found
            let config_str = fs::read_to_string(&config_path)
                .wrap_err_with(|| format!("Couldn't read config file {}", config_path.display()))?;
            toml::from_str(&config_str)
                .wrap_err_with(|| format!("Couldn't parse config file {}", config_path.display()))?
        } else {
            // Use default values if not found
            Config::default()
        };
        // If no data dir is provided, use the default
        if config.data_dir.as_os_str().is_empty() {
            config.data_dir = proj_dirs.data_dir().to_path_buf();
        }

        // Validate there are no conflicts on the key bindings
        let conflicts = config.keybindings.find_conflicts();
        if !conflicts.is_empty() {
            return Err(eyre!(
                "Invalid config, there are some key binding conflicts:\n{}",
                conflicts
                    .into_iter()
                    .map(|(_, a)| format!("- {}", a.into_iter().map(|a| format!("{a:?}")).join(", ")))
                    .join("\n")
            ));
        }

        // Create the data directory if not found
        fs::create_dir_all(&config.data_dir)
            .wrap_err_with(|| format!("Could't create data dir {}", config.data_dir.display()))?;

        Ok(config)
    }
}

impl KeyBindingsConfig {
    /// Retrieves the [KeyBinding] for a specific action
    pub fn get(&self, action: &KeyBindingAction) -> &KeyBinding {
        self.0.get(action).unwrap()
    }

    /// Finds the [KeyBindingAction] associated with the given [KeyEvent], if any
    pub fn get_action_matching(&self, event: &KeyEvent) -> Option<KeyBindingAction> {
        self.0.iter().find_map(
            |(action, binding)| {
                if binding.matches(event) { Some(*action) } else { None }
            },
        )
    }

    /// Finds all ambiguous key bindings where a single `KeyEvent` maps to multiple `KeyBindingAction`s
    pub fn find_conflicts(&self) -> Vec<(KeyEvent, Vec<KeyBindingAction>)> {
        // A map to store each KeyEvent and the list of actions it's bound to.
        let mut event_to_actions_map: HashMap<KeyEvent, Vec<KeyBindingAction>> = HashMap::new();

        // Iterate over all configured actions and their bindings.
        for (action, key_binding) in self.0.iter() {
            // For each KeyEvent defined within the current KeyBinding...
            for event_in_binding in key_binding.0.iter() {
                // Record that this event maps to the current action.
                event_to_actions_map.entry(*event_in_binding).or_default().push(*action);
            }
        }

        // Filter the map to find KeyEvents that map to more than one action.
        event_to_actions_map
            .into_iter()
            .filter_map(|(key_event, actions)| {
                if actions.len() > 1 {
                    Some((key_event, actions))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl KeyBinding {
    /// Checks if a given `KeyEvent` matches any of the key events configured for this key binding, considering only the
    /// key `code` and its `modifiers`.
    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.0
            .iter()
            .any(|e| e.code == event.code && e.modifiers == event.modifiers)
    }
}

impl Theme {
    /// Primary style applied when an item is highlighted, including the background color
    pub fn highlight_primary_full(&self) -> ContentStyle {
        if let Some(color) = self.highlight {
            let mut ret = self.highlight_primary;
            ret.background_color = Some(color);
            ret
        } else {
            self.highlight_primary
        }
    }

    /// Secondary style applied when an item is highlighted, including the background color
    pub fn highlight_secondary_full(&self) -> ContentStyle {
        if let Some(color) = self.highlight {
            let mut ret = self.highlight_secondary;
            ret.background_color = Some(color);
            ret
        } else {
            self.highlight_secondary
        }
    }

    /// Accent style applied when an item is highlighted, including the background color
    pub fn highlight_accent_full(&self) -> ContentStyle {
        if let Some(color) = self.highlight {
            let mut ret = self.highlight_accent;
            ret.background_color = Some(color);
            ret
        } else {
            self.highlight_accent
        }
    }

    /// Comments style applied when an item is highlighted, including the background color
    pub fn highlight_comment_full(&self) -> ContentStyle {
        if let Some(color) = self.highlight {
            let mut ret = self.highlight_comment;
            ret.background_color = Some(color);
            ret
        } else {
            self.highlight_comment
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::new(),
            check_updates: true,
            inline: true,
            search: SearchConfig::default(),
            logs: LogsConfig::default(),
            keybindings: KeyBindingsConfig::default(),
            theme: Theme::default(),
            gist: GistConfig::default(),
            tuning: SearchTuning::default(),
        }
    }
}
impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            delay: 250,
            mode: SearchMode::Auto,
            user_only: false,
        }
    }
}
impl Default for LogsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            filter: String::from("info"),
        }
    }
}
impl Default for KeyBindingsConfig {
    fn default() -> Self {
        Self(BTreeMap::from([
            (KeyBindingAction::Quit, KeyBinding(vec![KeyEvent::from(KeyCode::Esc)])),
            (
                KeyBindingAction::Update,
                KeyBinding(vec![
                    KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
                    KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
                    KeyEvent::from(KeyCode::F(2)),
                ]),
            ),
            (
                KeyBindingAction::Delete,
                KeyBinding(vec![KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)]),
            ),
            (
                KeyBindingAction::Confirm,
                KeyBinding(vec![KeyEvent::from(KeyCode::Tab), KeyEvent::from(KeyCode::Enter)]),
            ),
            (
                KeyBindingAction::Execute,
                KeyBinding(vec![
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
                    KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
                ]),
            ),
            (
                KeyBindingAction::SearchMode,
                KeyBinding(vec![KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)]),
            ),
            (
                KeyBindingAction::SearchUserOnly,
                KeyBinding(vec![KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)]),
            ),
        ]))
    }
}
impl Default for Theme {
    fn default() -> Self {
        let primary = ContentStyle::new();
        let highlight_primary = primary;

        let mut secondary = ContentStyle::new();
        secondary.attributes.set(Attribute::Dim);
        let highlight_secondary = secondary;

        let mut accent = ContentStyle::new();
        accent.foreground_color = Some(Color::Yellow);
        let highlight_accent = accent;

        let mut comment = ContentStyle::new();
        comment.foreground_color = Some(Color::Green);
        comment.attributes.set(Attribute::Italic);
        let highlight_comment = comment;

        let mut error = ContentStyle::new();
        error.foreground_color = Some(Color::DarkRed);

        Self {
            primary,
            secondary,
            accent,
            comment,
            error,
            highlight: Some(Color::DarkGrey),
            highlight_symbol: String::from("» "),
            highlight_primary,
            highlight_secondary,
            highlight_accent,
            highlight_comment,
        }
    }
}
impl Default for SearchCommandsTextTuning {
    fn default() -> Self {
        Self {
            points: 600,
            command: 2.0,
            description: 1.0,
            auto: SearchCommandsTextAutoTuning::default(),
        }
    }
}
impl Default for SearchCommandsTextAutoTuning {
    fn default() -> Self {
        Self {
            prefix: 1.5,
            fuzzy: 1.0,
            relaxed: 0.5,
            root: 2.0,
        }
    }
}
impl Default for SearchUsageTuning {
    fn default() -> Self {
        Self { points: 100 }
    }
}
impl Default for SearchPathTuning {
    fn default() -> Self {
        Self {
            points: 300,
            exact: 1.0,
            ancestor: 0.5,
            descendant: 0.25,
            unrelated: 0.1,
        }
    }
}
impl Default for SearchVariableContextTuning {
    fn default() -> Self {
        Self { points: 700 }
    }
}

/// Custom deserialization function for the BTreeMap in KeyBindingsConfig.
///
/// Behavior depends on whether compiled for test or not:
/// - In test (`#[cfg(test)]`): Requires all `KeyBindingAction` variants to be present; otherwise, errors. No merging.
/// - In non-test (`#[cfg(not(test))]`): Merges user-provided bindings with defaults.
fn deserialize_bindings_with_defaults<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<KeyBindingAction, KeyBinding>, D::Error>
where
    D: Deserializer<'de>,
{
    // Deserialize the map as provided in the config.
    let user_provided_bindings = BTreeMap::<KeyBindingAction, KeyBinding>::deserialize(deserializer)?;

    #[cfg(test)]
    {
        use strum::IntoEnumIterator;
        // In test mode, all actions must be explicitly defined. No defaults are merged.
        for action_variant in KeyBindingAction::iter() {
            if !user_provided_bindings.contains_key(&action_variant) {
                return Err(D::Error::custom(format!(
                    "Missing key binding for action '{action_variant:?}'."
                )));
            }
        }
        Ok(user_provided_bindings)
    }
    #[cfg(not(test))]
    {
        // In non-test (production) mode, merge with defaults.
        // User-provided bindings override defaults for the actions they specify.
        let mut final_bindings = user_provided_bindings;
        let default_bindings = KeyBindingsConfig::default();

        for (action, default_binding) in default_bindings.0 {
            final_bindings.entry(action).or_insert(default_binding);
        }
        Ok(final_bindings)
    }
}

/// Deserializes a string or a vector of strings into a `Vec<KeyEvent>`.
///
/// This allows a key binding to be specified as a single string or a list of strings in the config file.
fn deserialize_key_events<'de, D>(deserializer: D) -> Result<Vec<KeyEvent>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Single(String),
        Multiple(Vec<String>),
    }

    let strings = match StringOrVec::deserialize(deserializer)? {
        StringOrVec::Single(s) => vec![s],
        StringOrVec::Multiple(v) => v,
    };

    strings
        .iter()
        .map(String::as_str)
        .map(parse_key_event)
        .map(|r| r.map_err(D::Error::custom))
        .collect()
}

/// Deserializes a string into an optional [`Color`].
///
/// Supports color names, RGB (e.g., `rgb(255, 0, 100)`), hex (e.g., `#ff0064`), indexed colors (e.g., `6`), and "none"
/// for no color.
fn deserialize_color<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
where
    D: Deserializer<'de>,
{
    parse_color(&String::deserialize(deserializer)?).map_err(D::Error::custom)
}

/// Deserializes a string into a [`ContentStyle`].
///
/// Supports color names and modifiers (e.g., "red", "bold", "italic blue", "underline dim green").
fn deserialize_style<'de, D>(deserializer: D) -> Result<ContentStyle, D::Error>
where
    D: Deserializer<'de>,
{
    parse_style(&String::deserialize(deserializer)?).map_err(D::Error::custom)
}

/// Parses a string representation of a key event into a [`KeyEvent`].
///
/// Supports modifiers like `ctrl-`, `alt-`, `shift-` and standard key names/characters.
fn parse_key_event(raw: &str) -> Result<KeyEvent, String> {
    let raw_lower = raw.to_ascii_lowercase();
    let (remaining, modifiers) = extract_key_modifiers(&raw_lower);
    parse_key_code_with_modifiers(remaining, modifiers)
}

/// Extracts key modifiers (ctrl, shift, alt) from the beginning of a key event string.
///
/// Returns the remaining string and the parsed modifiers.
fn extract_key_modifiers(raw: &str) -> (&str, KeyModifiers) {
    let mut modifiers = KeyModifiers::empty();
    let mut current = raw;

    loop {
        match current {
            rest if rest.starts_with("ctrl-") || rest.starts_with("ctrl+") => {
                modifiers.insert(KeyModifiers::CONTROL);
                current = &rest[5..];
            }
            rest if rest.starts_with("shift-") || rest.starts_with("shift+") => {
                modifiers.insert(KeyModifiers::SHIFT);
                current = &rest[6..];
            }
            rest if rest.starts_with("alt-") || rest.starts_with("alt+") => {
                modifiers.insert(KeyModifiers::ALT);
                current = &rest[4..];
            }
            _ => break,
        };
    }

    (current, modifiers)
}

/// Parses the remaining string after extracting modifiers into a [`KeyCode`]
fn parse_key_code_with_modifiers(raw: &str, mut modifiers: KeyModifiers) -> Result<KeyEvent, String> {
    let code = match raw {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backtab" => {
            modifiers.insert(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" | "spacebar" => KeyCode::Char(' '),
        "hyphen" => KeyCode::Char('-'),
        "minus" => KeyCode::Char('-'),
        "tab" => KeyCode::Tab,
        c if c.len() == 1 => {
            let mut c = c.chars().next().expect("just checked");
            if modifiers.contains(KeyModifiers::SHIFT) {
                c = c.to_ascii_uppercase();
            }
            KeyCode::Char(c)
        }
        _ => return Err(format!("Unable to parse key binding: {raw}")),
    };
    Ok(KeyEvent::new(code, modifiers))
}

/// Parses a string into an optional [`Color`].
///
/// Handles named colors, RGB, hex, indexed colors, and "none".
fn parse_color(raw: &str) -> Result<Option<Color>, String> {
    let raw_lower = raw.to_ascii_lowercase();
    if raw.is_empty() || raw == "none" {
        Ok(None)
    } else {
        Ok(Some(parse_color_inner(&raw_lower)?))
    }
}

/// Parses a string into a [`ContentStyle`], including attributes and foreground color.
///
/// Examples: "red", "bold", "italic blue", "underline dim green".
fn parse_style(raw: &str) -> Result<ContentStyle, String> {
    let raw_lower = raw.to_ascii_lowercase();
    let (remaining, attributes) = extract_style_attributes(&raw_lower);
    let mut style = ContentStyle::new();
    style.attributes = attributes;
    if !remaining.is_empty() && remaining != "default" {
        style.foreground_color = Some(parse_color_inner(remaining)?);
    }
    Ok(style)
}

/// Extracts style attributes (bold, dim, italic, underline) from the beginning of a style string.
///
/// Returns the remaining string and the parsed attributes.
fn extract_style_attributes(raw: &str) -> (&str, Attributes) {
    let mut attributes = Attributes::none();
    let mut current = raw;

    loop {
        match current {
            rest if rest.starts_with("bold") => {
                attributes.set(Attribute::Bold);
                current = &rest[4..];
                if current.starts_with(' ') {
                    current = &current[1..];
                }
            }
            rest if rest.starts_with("dim") => {
                attributes.set(Attribute::Dim);
                current = &rest[3..];
                if current.starts_with(' ') {
                    current = &current[1..];
                }
            }
            rest if rest.starts_with("italic") => {
                attributes.set(Attribute::Italic);
                current = &rest[6..];
                if current.starts_with(' ') {
                    current = &current[1..];
                }
            }
            rest if rest.starts_with("underline") => {
                attributes.set(Attribute::Underlined);
                current = &rest[9..];
                if current.starts_with(' ') {
                    current = &current[1..];
                }
            }
            rest if rest.starts_with("underlined") => {
                attributes.set(Attribute::Underlined);
                current = &rest[10..];
                if current.starts_with(' ') {
                    current = &current[1..];
                }
            }
            _ => break,
        };
    }

    (current.trim(), attributes)
}

/// Parses the color part of a style string.
///
/// Handles named colors, rgb, hex, and ansi values.
fn parse_color_inner(raw: &str) -> Result<Color, String> {
    Ok(match raw {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Grey,
        "dark gray" | "darkgray" | "dark grey" | "darkgrey" => Color::DarkGrey,
        "dark red" | "darkred" => Color::DarkRed,
        "dark green" | "darkgreen" => Color::DarkGreen,
        "dark yellow" | "darkyellow" => Color::DarkYellow,
        "dark blue" | "darkblue" => Color::DarkBlue,
        "dark magenta" | "darkmagenta" => Color::DarkMagenta,
        "dark cyan" | "darkcyan" => Color::DarkCyan,
        "white" => Color::White,
        rgb if rgb.starts_with("rgb(") => {
            let rgb = rgb.trim_start_matches("rgb(").trim_end_matches(")").split(',');
            let rgb = rgb
                .map(|c| c.trim().parse::<u8>())
                .collect::<Result<Vec<u8>, _>>()
                .map_err(|_| format!("Unable to parse color: {raw}"))?;
            if rgb.len() != 3 {
                return Err(format!("Unable to parse color: {raw}"));
            }
            Color::Rgb {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            }
        }
        hex if hex.starts_with("#") => {
            let hex = hex.trim_start_matches("#");
            if hex.len() != 6 {
                return Err(format!("Unable to parse color: {raw}"));
            }
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| format!("Unable to parse color: {raw}"))?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| format!("Unable to parse color: {raw}"))?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| format!("Unable to parse color: {raw}"))?;
            Color::Rgb { r, g, b }
        }
        c => {
            if let Ok(c) = c.parse::<u8>() {
                Color::AnsiValue(c)
            } else {
                return Err(format!("Unable to parse color: {raw}"));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn test_default_config() -> Result<()> {
        let config_str = fs::read_to_string("default_config.toml").wrap_err("Couldn't read default config file")?;
        let config: Config = toml::from_str(&config_str).wrap_err("Couldn't parse default config file")?;

        assert_eq!(Config::default(), config);

        Ok(())
    }

    #[test]
    fn test_default_keybindings_complete() {
        let config = KeyBindingsConfig::default();

        for action in KeyBindingAction::iter() {
            assert!(
                config.0.contains_key(&action),
                "Missing default binding for action: {action:?}"
            );
        }
    }

    #[test]
    fn test_default_keybindings_no_conflicts() {
        let config = KeyBindingsConfig::default();

        let conflicts = config.find_conflicts();
        assert_eq!(conflicts.len(), 0, "Key binding conflicts: {conflicts:?}");
    }

    #[test]
    fn test_keybinding_matches() {
        let binding = KeyBinding(vec![
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
            KeyEvent::from(KeyCode::Enter),
        ]);

        // Should match exact events
        assert!(binding.matches(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)));
        assert!(binding.matches(&KeyEvent::from(KeyCode::Enter)));

        // Should not match events with different modifiers
        assert!(!binding.matches(&KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::ALT
        )));

        // Should not match different key codes
        assert!(!binding.matches(&KeyEvent::from(KeyCode::Esc)));
    }

    #[test]
    fn test_simple_keys() {
        assert_eq!(
            parse_key_event("a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
        );

        assert_eq!(
            parse_key_event("esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())
        );
    }

    #[test]
    fn test_with_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        );

        assert_eq!(
            parse_key_event("alt-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)
        );

        assert_eq!(
            parse_key_event("shift-esc").unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_multiple_modifiers() {
        assert_eq!(
            parse_key_event("ctrl-alt-a").unwrap(),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::ALT)
        );

        assert_eq!(
            parse_key_event("ctrl-shift-enter").unwrap(),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn test_invalid_keys() {
        let res = parse_key_event("invalid-key");
        assert_eq!(res, Err(String::from("Unable to parse key binding: invalid-key")));
    }

    #[test]
    fn test_parse_color_none() {
        let color = parse_color("none").unwrap();
        assert_eq!(color, None);
    }

    #[test]
    fn test_parse_color_simple() {
        let color = parse_color("red").unwrap();
        assert_eq!(color, Some(Color::Red));
    }

    #[test]
    fn test_parse_color_rgb() {
        let color = parse_color("rgb(50, 25, 15)").unwrap();
        assert_eq!(color, Some(Color::Rgb { r: 50, g: 25, b: 15 }));
    }

    #[test]
    fn test_parse_color_rgb_out_of_range() {
        let res = parse_color("rgb(500, 25, 15)");
        assert_eq!(res, Err(String::from("Unable to parse color: rgb(500, 25, 15)")));
    }

    #[test]
    fn test_parse_color_rgb_invalid() {
        let res = parse_color("rgb(50, 25, 15, 5)");
        assert_eq!(res, Err(String::from("Unable to parse color: rgb(50, 25, 15, 5)")));
    }

    #[test]
    fn test_parse_color_hex() {
        let color = parse_color("#4287f5").unwrap();
        assert_eq!(color, Some(Color::Rgb { r: 66, g: 135, b: 245 }));
    }

    #[test]
    fn test_parse_color_hex_out_of_range() {
        let res = parse_color("#4287fg");
        assert_eq!(res, Err(String::from("Unable to parse color: #4287fg")));
    }

    #[test]
    fn test_parse_color_hex_invalid() {
        let res = parse_color("#4287f50");
        assert_eq!(res, Err(String::from("Unable to parse color: #4287f50")));
    }

    #[test]
    fn test_parse_color_index() {
        let color = parse_color("6").unwrap();
        assert_eq!(color, Some(Color::AnsiValue(6)));
    }

    #[test]
    fn test_parse_color_fail() {
        let res = parse_color("1234");
        assert_eq!(res, Err(String::from("Unable to parse color: 1234")));
    }

    #[test]
    fn test_parse_style_empty() {
        let style = parse_style("").unwrap();
        assert_eq!(style, ContentStyle::new());
    }

    #[test]
    fn test_parse_style_default() {
        let style = parse_style("default").unwrap();
        assert_eq!(style, ContentStyle::new());
    }

    #[test]
    fn test_parse_style_simple() {
        let style = parse_style("red").unwrap();
        assert_eq!(style.foreground_color, Some(Color::Red));
        assert_eq!(style.attributes, Attributes::none());
    }

    #[test]
    fn test_parse_style_only_modifier() {
        let style = parse_style("bold").unwrap();
        assert_eq!(style.foreground_color, None);
        let mut expected_attributes = Attributes::none();
        expected_attributes.set(Attribute::Bold);
        assert_eq!(style.attributes, expected_attributes);
    }

    #[test]
    fn test_parse_style_with_modifier() {
        let style = parse_style("italic red").unwrap();
        assert_eq!(style.foreground_color, Some(Color::Red));
        let mut expected_attributes = Attributes::none();
        expected_attributes.set(Attribute::Italic);
        assert_eq!(style.attributes, expected_attributes);
    }

    #[test]
    fn test_parse_style_multiple_modifier() {
        let style = parse_style("underline dim dark red").unwrap();
        assert_eq!(style.foreground_color, Some(Color::DarkRed));
        let mut expected_attributes = Attributes::none();
        expected_attributes.set(Attribute::Underlined);
        expected_attributes.set(Attribute::Dim);
        assert_eq!(style.attributes, expected_attributes);
    }
}
