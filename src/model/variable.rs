use std::{
    collections::HashSet,
    convert::Infallible,
    fmt::{Display, Formatter},
    mem,
    str::FromStr,
};

use heck::ToShoutySnakeCase;
use itertools::Itertools;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

use crate::utils::{COMMAND_VARIABLE_REGEX, SplitCaptures, SplitItem, flatten_str, flatten_variable};

/// Suggestion for a variable value
pub enum VariableSuggestion {
    /// A new secret value, the user must input it and it won't be stored
    Secret,
    /// A new value, if the user enters it, it must be then stored
    New,
    /// Suggestion from the environment variables
    Environment {
        env_var_name: String,
        value: Option<String>,
    },
    /// Suggestion for an already-stored value
    Existing(VariableValue),
    /// Literal suggestion, derived from the variable name itself
    Derived(String),
}

/// Type to represent a variable value
#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct VariableValue {
    /// The unique identifier for the value (if stored)
    pub id: Option<i32>,
    /// The flattened root command (i.e., the first word)
    pub flat_root_cmd: String,
    /// The flattened variable name (or multiple, e.g., "var1|var2")
    pub flat_variable: String,
    /// The variable value
    pub value: String,
}

impl VariableValue {
    /// Creates a new variable value
    pub fn new(root_cmd: impl Into<String>, variable_name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: None,
            flat_root_cmd: flatten_str(root_cmd.into()),
            flat_variable: flatten_variable(variable_name.into()),
            value: value.into(),
        }
    }
}

/// A command containing variables
#[cfg_attr(debug_assertions, derive(Debug))]
#[derive(Clone)]
pub struct DynamicCommand {
    /// The root command (i.e., the first word)
    pub root: String,
    /// The different parts of the command, including variables or values
    pub parts: Vec<CommandPart>,
}
impl DynamicCommand {
    /// Parses the given command as a [DynamicCommand]
    pub fn parse(cmd: impl AsRef<str>) -> Self {
        let cmd = cmd.as_ref();
        let splitter = SplitCaptures::new(&COMMAND_VARIABLE_REGEX, cmd);
        let parts = splitter
            .map(|e| match e {
                SplitItem::Unmatched(t) => CommandPart::Text(t.to_owned()),
                SplitItem::Captured(v) => CommandPart::Variable(Variable::parse(v.get(1).unwrap().as_str())),
            })
            .collect::<Vec<_>>();

        DynamicCommand {
            root: cmd.split_whitespace().next().unwrap_or(cmd).to_owned(),
            parts,
        }
    }

    /// Checks if the command has any variables without value
    pub fn has_pending_variable(&self) -> bool {
        self.parts.iter().any(|part| matches!(part, CommandPart::Variable(_)))
    }

    /// Retrieves the first variable without value in the command
    pub fn current_variable(&self) -> Option<&Variable> {
        self.parts.iter().find_map(|part| {
            if let CommandPart::Variable(v) = part {
                Some(v)
            } else {
                None
            }
        })
    }

    /// Retrieves the context for the current variable in the command
    pub fn current_variable_context(&self) -> impl IntoIterator<Item = (String, String)> {
        self.parts
            .iter()
            .take_while(|part| !matches!(part, CommandPart::Variable(_)))
            .filter_map(|part| {
                if let CommandPart::VariableValue(v, value) = part
                    && !v.secret
                {
                    Some((v.name.clone(), value.clone()))
                } else {
                    None
                }
            })
    }

    /// Updates the first variable in the command for the given value
    pub fn set_next_variable(&mut self, value: impl Into<String>) {
        // Find the first part in the command that is an unfilled variable
        if let Some(part) = self.parts.iter_mut().find(|p| matches!(p, CommandPart::Variable(_))) {
            // Replace it with the filled variable including the value
            if let CommandPart::Variable(v) = mem::take(part) {
                *part = CommandPart::VariableValue(v, value.into());
            } else {
                unreachable!();
            }
        }
    }

    /// Creates a [VariableValue] for this command with the given variable name and value
    pub fn new_variable_value_for(&self, variable_name: impl Into<String>, value: impl Into<String>) -> VariableValue {
        VariableValue::new(&self.root, variable_name, value)
    }
}
impl FromStr for DynamicCommand {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}
impl Display for DynamicCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for part in self.parts.iter() {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

/// Represents a part of a command, which can be either text, a variable, or a variable value
#[cfg_attr(debug_assertions, derive(Debug, PartialEq, Eq))]
#[derive(Clone)]
pub enum CommandPart {
    Text(String),
    Variable(Variable),
    VariableValue(Variable, String),
}
impl Default for CommandPart {
    fn default() -> Self {
        Self::Text(String::new())
    }
}
impl Display for CommandPart {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandPart::Text(t) => write!(f, "{t}"),
            CommandPart::Variable(v) => write!(f, "{{{{{}}}}}", v.name),
            CommandPart::VariableValue(_, value) => write!(f, "{value}"),
        }
    }
}

/// Represents a variable from a command
#[cfg_attr(debug_assertions, derive(Debug, PartialEq, Eq))]
#[derive(Clone)]
pub struct Variable {
    /// The variable name, as it was displayed on the command
    pub name: String,
    /// Parsed variable values derived from the name
    pub options: Vec<String>,
    /// Parsed varable functions to apply
    pub functions: Vec<VariableFunction>,
    /// Whether the variable is secret
    pub secret: bool,
}
impl Variable {
    /// Parses a variable into its components
    pub fn parse(text: impl Into<String>) -> Self {
        let name: String = text.into();

        // Determine if the variable is secret or not
        let (variable_name, secret) = match is_secret_variable(&name) {
            Some(inner) => (inner, true),
            None => (name.as_str(), false),
        };

        // Split the variable name in parts
        let parts: Vec<&str> = variable_name.split(':').collect();
        let mut functions = Vec::new();
        let mut boundary_index = parts.len();

        // Iterate from right-to-left to find the boundary between options and functions
        if parts.len() > 1 {
            for (i, part) in parts.iter().enumerate().rev() {
                if let Ok(func) = VariableFunction::from_str(part) {
                    functions.push(func);
                    boundary_index = i;
                } else {
                    break;
                }
            }
        }

        // The collected functions are in reverse order
        functions.reverse();

        // Join the option parts back together, then split them by the pipe character
        let options_str = &parts[..boundary_index].join(":");
        let options = if options_str.is_empty() {
            vec![]
        } else {
            options_str
                .split('|')
                .map(|o| o.trim())
                .filter(|o| !o.is_empty())
                .map(String::from)
                .collect()
        };

        Self {
            name,
            options,
            functions,
            secret,
        }
    }

    /// Retrieves the env var names where the value for this variable might reside (in order of preference)
    pub fn env_var_names(&self, include_options: bool) -> HashSet<String> {
        let mut names = HashSet::new();
        let env_var_name = self.name.to_shouty_snake_case();
        if !env_var_name.trim().is_empty() {
            names.insert(env_var_name.trim().to_owned());
        }
        let env_var_name_no_fn = self.options.iter().join("|").to_shouty_snake_case();
        if !env_var_name_no_fn.trim().is_empty() {
            names.insert(env_var_name_no_fn.trim().to_owned());
        }
        if include_options {
            names.extend(
                self.options
                    .iter()
                    .map(|o| o.to_shouty_snake_case())
                    .filter(|o| !o.trim().is_empty())
                    .map(|o| o.trim().to_owned()),
            );
        }
        names
    }

    /// Applies variable functions to the given text
    pub fn apply_functions_to(&self, text: impl Into<String>) -> String {
        let text = text.into();
        let mut result = text;
        for func in self.functions.iter() {
            result = func.apply_to(result);
        }
        result
    }

    /// Iterates every function to check if a char has to be replaced
    pub fn check_functions_char(&self, ch: char) -> Option<String> {
        let mut out: Option<String> = None;
        for func in self.functions.iter() {
            if let Some(ref mut out) = out {
                let mut new_out = String::from("");
                for ch in out.chars() {
                    if let Some(replacement) = func.check_char(ch) {
                        new_out.push_str(&replacement);
                    } else {
                        new_out.push(ch);
                    }
                }
                *out = new_out;
            } else if let Some(replacement) = func.check_char(ch) {
                out = Some(replacement);
            }
        }
        out
    }
}
impl FromStr for Variable {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

/// Functions that can be applied to variable values
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString)]
pub enum VariableFunction {
    #[strum(serialize = "kebab")]
    KebabCase,
    #[strum(serialize = "snake")]
    SnakeCase,
    #[strum(serialize = "upper")]
    UpperCase,
    #[strum(serialize = "lower")]
    LowerCase,
    #[strum(serialize = "url")]
    Urlencode,
}
impl VariableFunction {
    /// Applies this function to the given text
    pub fn apply_to(&self, input: impl AsRef<str>) -> String {
        let input = input.as_ref();
        match self {
            Self::KebabCase => replace_separators(input, '-'),
            Self::SnakeCase => replace_separators(input, '_'),
            Self::UpperCase => input.to_uppercase(),
            Self::LowerCase => input.to_lowercase(),
            Self::Urlencode => idempotent_percent_encode(input, NON_ALPHANUMERIC),
        }
    }

    /// Checks if this char would be transformed by this function
    pub fn check_char(&self, ch: char) -> Option<String> {
        match self {
            Self::KebabCase | Self::SnakeCase => {
                let separator = if *self == Self::KebabCase { '-' } else { '_' };
                if ch != separator && is_separator(ch) {
                    Some(separator.to_string())
                } else {
                    None
                }
            }
            Self::UpperCase => {
                if ch.is_lowercase() {
                    Some(ch.to_uppercase().to_string())
                } else {
                    None
                }
            }
            Self::LowerCase => {
                if ch.is_uppercase() {
                    Some(ch.to_lowercase().to_string())
                } else {
                    None
                }
            }
            Self::Urlencode => {
                if ch.is_ascii_alphanumeric() {
                    None
                } else {
                    Some(idempotent_percent_encode(&ch.to_string(), NON_ALPHANUMERIC))
                }
            }
        }
    }
}

/// Checks if a given variable is a secret (must not be stored), returning the inner variable name if it is
fn is_secret_variable(variable_name: &str) -> Option<&str> {
    if (variable_name.starts_with('*') && variable_name.ends_with('*') && variable_name.len() > 1)
        || (variable_name.starts_with('{') && variable_name.ends_with('}'))
    {
        Some(&variable_name[1..variable_name.len() - 1])
    } else {
        None
    }
}

/// Checks if a character is a separator
fn is_separator(c: char) -> bool {
    c == '-' || c == '_' || c.is_whitespace()
}

// This function replaces any sequence of separators with a single one
fn replace_separators(s: &str, separator: char) -> String {
    let mut result = String::with_capacity(s.len());

    // Split the string using the custom separator logic and filter out empty results
    let mut words = s.split(is_separator).filter(|word| !word.is_empty());

    // Join the first word without a separator
    if let Some(first_word) = words.next() {
        result.push_str(first_word);
    }
    // Append the separator and the rest of the words
    for word in words {
        result.push(separator);
        result.push_str(word);
    }

    result
}

/// Idempotently percent-encodes a string.
///
/// This function first checks if the input is already a correctly percent-encoded string
/// according to the provided `encode_set`.
/// - If it is, the input is returned
/// - If it is not (i.e., it's unencoded, partially encoded, or incorrectly encoded), the function encodes the entire
///   input string and returns a new `String`
pub fn idempotent_percent_encode(input: &str, encode_set: &'static AsciiSet) -> String {
    // Attempt to decode the input
    if let Ok(decoded) = percent_decode_str(input).decode_utf8() {
        // If successful, re-encode the decoded string using the same character set
        let re_encoded = utf8_percent_encode(&decoded, encode_set).to_string();

        // If the re-encoded string matches the original input, it means the input was already perfectly encoded
        if re_encoded == input {
            return re_encoded;
        }
    }

    // In all other cases (decoding failed, or the re-encoded string is different), encode it fully
    utf8_percent_encode(input, encode_set).to_string().to_string()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    #[test]
    fn test_parse_command_with_variables() {
        let cmd = DynamicCommand::parse("git commit -m {{{message}}} --author {{author:kebab}}");
        assert_eq!(cmd.root, "git");
        assert_eq!(cmd.parts.len(), 4);
        assert_eq!(cmd.parts[0], CommandPart::Text("git commit -m ".into()));
        assert!(matches!(cmd.parts[1], CommandPart::Variable(_)));
        assert_eq!(cmd.parts[2], CommandPart::Text(" --author ".into()));
        assert!(matches!(cmd.parts[3], CommandPart::Variable(_)));
    }

    #[test]
    fn test_parse_command_no_variables() {
        let cmd = DynamicCommand::parse("echo 'hello world'");
        assert_eq!(cmd.root, "echo");
        assert_eq!(cmd.parts.len(), 1);
        assert_eq!(cmd.parts[0], CommandPart::Text("echo 'hello world'".into()));
    }

    #[test]
    fn test_set_next_variable() {
        let mut cmd = DynamicCommand::parse("cmd {{var1}} {{var2}}");
        cmd.set_next_variable("value1");
        let var1 = Variable::parse("var1");
        assert_eq!(cmd.parts[1], CommandPart::VariableValue(var1, "value1".into()));
        cmd.set_next_variable("value2");
        let var2 = Variable::parse("var2");
        assert_eq!(cmd.parts[3], CommandPart::VariableValue(var2, "value2".into()));
    }

    #[test]
    fn test_has_pending_variable() {
        let mut cmd = DynamicCommand::parse("cmd {{var1}} {{var2}}");
        assert!(cmd.has_pending_variable());
        cmd.set_next_variable("value1");
        assert!(cmd.has_pending_variable());
        cmd.set_next_variable("value2");
        assert!(!cmd.has_pending_variable());
    }

    #[test]
    fn test_current_variable() {
        let mut cmd = DynamicCommand::parse("cmd {{var1}} {{var2}}");
        assert_eq!(cmd.current_variable().map(|l| l.name.as_str()), Some("var1"));
        cmd.set_next_variable("value1");
        assert_eq!(cmd.current_variable().map(|l| l.name.as_str()), Some("var2"));
        cmd.set_next_variable("value2");
        assert_eq!(cmd.current_variable(), None);
    }

    #[test]
    fn test_current_variable_context() {
        let mut cmd = DynamicCommand::parse("cmd {{var1}} {{{secret_var}}} {{var2}}");

        // Set value for the first variable
        cmd.set_next_variable("value1");
        let context_before_secret: Vec<_> = cmd.current_variable_context().into_iter().collect();
        assert_eq!(context_before_secret, vec![("var1".to_string(), "value1".to_string())]);

        // Set value for the secret variable
        cmd.set_next_variable("secret_value");
        let context_after_secret: Vec<_> = cmd.current_variable_context().into_iter().collect();
        // The secret variable value should not be in the context
        assert_eq!(context_after_secret, context_before_secret);
    }

    #[test]
    fn test_current_variable_context_is_empty() {
        let cmd = DynamicCommand::parse("cmd {{var1}}");
        let context: Vec<_> = cmd.current_variable_context().into_iter().collect();
        assert!(context.is_empty());
    }

    #[test]
    fn test_parse_simple_variable() {
        let variable = Variable::parse("my_variable");
        assert_eq!(
            variable,
            Variable {
                name: "my_variable".into(),
                options: vec!["my_variable".into()],
                functions: vec![],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_secret_variable() {
        let variable = Variable::parse("{my_secret}");
        assert_eq!(
            variable,
            Variable {
                name: "{my_secret}".into(),
                options: vec!["my_secret".into()],
                functions: vec![],
                secret: true,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_multiple_options() {
        let variable = Variable::parse("option1|option2|option3");
        assert_eq!(
            variable,
            Variable {
                name: "option1|option2|option3".into(),
                options: vec!["option1".into(), "option2".into(), "option3".into()],
                functions: vec![],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_single_function() {
        let variable = Variable::parse("my_variable:kebab");
        assert_eq!(
            variable,
            Variable {
                name: "my_variable:kebab".into(),
                options: vec!["my_variable".into()],
                functions: vec![VariableFunction::KebabCase],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_multiple_functions() {
        let variable = Variable::parse("my_variable:snake:upper");
        assert_eq!(
            variable,
            Variable {
                name: "my_variable:snake:upper".into(),
                options: vec!["my_variable".into()],
                functions: vec![VariableFunction::SnakeCase, VariableFunction::UpperCase],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_options_and_functions() {
        let variable = Variable::parse("opt1|opt2:lower:kebab");
        assert_eq!(
            variable,
            Variable {
                name: "opt1|opt2:lower:kebab".into(),
                options: vec!["opt1".into(), "opt2".into()],
                functions: vec![VariableFunction::LowerCase, VariableFunction::KebabCase],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_colon_in_options() {
        let variable = Variable::parse("key:value:kebab");
        assert_eq!(
            variable,
            Variable {
                name: "key:value:kebab".into(),
                options: vec!["key:value".into()],
                functions: vec![VariableFunction::KebabCase],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_only_functions() {
        let variable = Variable::parse(":snake");
        assert_eq!(
            variable,
            Variable {
                name: ":snake".into(),
                options: vec![],
                functions: vec![VariableFunction::SnakeCase],
                secret: false,
            }
        );
    }

    #[test]
    fn test_parse_variable_that_is_a_function_name() {
        let variable = Variable::parse("kebab");
        assert_eq!(
            variable,
            Variable {
                name: "kebab".into(),
                options: vec!["kebab".into()],
                functions: vec![],
                secret: false,
            }
        );
    }

    #[test]
    fn test_variable_env_var_names() {
        // Simple variable
        let var1 = Variable::parse("my-variable");
        assert_eq!(var1.env_var_names(true), HashSet::from(["MY_VARIABLE".into()]));

        // Variable with options
        let var2 = Variable::parse("option1|option2");
        assert_eq!(
            var2.env_var_names(true),
            HashSet::from(["OPTION1_OPTION2".into(), "OPTION1".into(), "OPTION2".into()])
        );
        assert_eq!(var2.env_var_names(false), HashSet::from(["OPTION1_OPTION2".into()]));

        // Variable with functions
        let var3 = Variable::parse("my-variable:kebab:upper");
        assert_eq!(
            var3.env_var_names(true),
            HashSet::from(["MY_VARIABLE_KEBAB_UPPER".into(), "MY_VARIABLE".into()])
        );

        // Secret variable with asterisks
        let var4 = Variable::parse("*my-secret*");
        assert_eq!(var4.env_var_names(true), HashSet::from(["MY_SECRET".into()]));

        // Secret variable with braces
        let var5 = Variable::parse("{my-secret}");
        assert_eq!(var5.env_var_names(true), HashSet::from(["MY_SECRET".into()]));
    }

    #[test]
    fn test_variable_apply_functions_to() {
        // No functions, should not change the input
        let var_none = Variable::parse("text");
        assert_eq!(var_none.apply_functions_to("Hello World"), "Hello World");

        // Single function
        let var_upper = Variable::parse("text:upper");
        assert_eq!(var_upper.apply_functions_to("Hello World"), "HELLO WORLD");

        // Chained functions: kebab then upper
        let var_kebab_upper = Variable::parse("text:kebab:upper");
        assert_eq!(var_kebab_upper.apply_functions_to("Hello World"), "HELLO-WORLD");

        // Chained functions: snake then lower
        let var_snake_lower = Variable::parse("text:snake:lower");
        assert_eq!(var_snake_lower.apply_functions_to("Hello World"), "hello_world");
    }

    #[test]
    fn test_variable_check_functions_char() {
        // No functions, should always be None
        let var_none = Variable::parse("text");
        assert_eq!(var_none.check_functions_char('a'), None);
        assert_eq!(var_none.check_functions_char(' '), None);

        // Single function with a change
        let var_upper = Variable::parse("text:upper");
        assert_eq!(var_upper.check_functions_char('a'), Some("A".to_string()));

        // Single function with no change
        let var_lower = Variable::parse("text:lower");
        assert_eq!(var_lower.check_functions_char('a'), None);

        // Chained functions
        let var_upper_kebab = Variable::parse("text:upper:kebab");
        assert_eq!(var_upper_kebab.check_functions_char('a'), Some("A".to_string()));
        assert_eq!(var_upper_kebab.check_functions_char(' '), Some("-".to_string()));
        assert_eq!(var_upper_kebab.check_functions_char('-'), None);
    }

    #[test]
    fn test_variable_function_apply_to() {
        // KebabCase
        assert_eq!(VariableFunction::KebabCase.apply_to("some text"), "some-text");
        assert_eq!(VariableFunction::KebabCase.apply_to("Some Text"), "Some-Text");
        assert_eq!(VariableFunction::KebabCase.apply_to("some_text"), "some-text");
        assert_eq!(VariableFunction::KebabCase.apply_to("-"), "");
        assert_eq!(VariableFunction::KebabCase.apply_to("_"), "");

        // SnakeCase
        assert_eq!(VariableFunction::SnakeCase.apply_to("some text"), "some_text");
        assert_eq!(VariableFunction::SnakeCase.apply_to("Some Text"), "Some_Text");
        assert_eq!(VariableFunction::SnakeCase.apply_to("some-text"), "some_text");
        assert_eq!(VariableFunction::SnakeCase.apply_to("-"), "");
        assert_eq!(VariableFunction::SnakeCase.apply_to("_"), "");

        // UpperCase
        assert_eq!(VariableFunction::UpperCase.apply_to("some text"), "SOME TEXT");
        assert_eq!(VariableFunction::UpperCase.apply_to("SomeText"), "SOMETEXT");

        // LowerCase
        assert_eq!(VariableFunction::LowerCase.apply_to("SOME TEXT"), "some text");
        assert_eq!(VariableFunction::LowerCase.apply_to("SomeText"), "sometext");

        // Urlencode
        assert_eq!(VariableFunction::Urlencode.apply_to("some text"), "some%20text");
        assert_eq!(VariableFunction::Urlencode.apply_to("Some Text"), "Some%20Text");
        assert_eq!(VariableFunction::Urlencode.apply_to("some-text"), "some%2Dtext");
        assert_eq!(VariableFunction::Urlencode.apply_to("some_text"), "some%5Ftext");
        assert_eq!(
            VariableFunction::Urlencode.apply_to("!@#$%^&*()"),
            "%21%40%23%24%25%5E%26%2A%28%29"
        );
        assert_eq!(VariableFunction::Urlencode.apply_to("some%20text"), "some%20text");
    }

    #[test]
    fn test_variable_function_check_char() {
        // KebabCase
        assert_eq!(VariableFunction::KebabCase.check_char(' '), Some("-".to_string()));
        assert_eq!(VariableFunction::KebabCase.check_char('_'), Some("-".to_string()));
        assert_eq!(VariableFunction::KebabCase.check_char('-'), None);
        assert_eq!(VariableFunction::KebabCase.check_char('A'), None);

        // SnakeCase
        assert_eq!(VariableFunction::SnakeCase.check_char(' '), Some("_".to_string()));
        assert_eq!(VariableFunction::SnakeCase.check_char('-'), Some("_".to_string()));
        assert_eq!(VariableFunction::SnakeCase.check_char('_'), None);
        assert_eq!(VariableFunction::SnakeCase.check_char('A'), None);

        // UpperCase
        assert_eq!(VariableFunction::UpperCase.check_char('a'), Some("A".to_string()));
        assert_eq!(VariableFunction::UpperCase.check_char('A'), None);
        assert_eq!(VariableFunction::UpperCase.check_char(' '), None);

        // LowerCase
        assert_eq!(VariableFunction::LowerCase.check_char('A'), Some("a".to_string()));
        assert_eq!(VariableFunction::LowerCase.check_char('a'), None);
        assert_eq!(VariableFunction::LowerCase.check_char(' '), None);

        // Urlencode
        assert_eq!(VariableFunction::Urlencode.check_char(' '), Some("%20".to_string()));
        assert_eq!(VariableFunction::Urlencode.check_char('!'), Some("%21".to_string()));
        assert_eq!(VariableFunction::Urlencode.check_char('A'), None);
        assert_eq!(VariableFunction::Urlencode.check_char('1'), None);
        assert_eq!(VariableFunction::Urlencode.check_char('-'), Some("%2D".to_string()));
        assert_eq!(VariableFunction::Urlencode.check_char('_'), Some("%5F".to_string()));
    }

    #[test]
    fn test_is_secret_variable() {
        // Test with asterisks
        assert_eq!(is_secret_variable("*secret*"), Some("secret"));
        assert_eq!(is_secret_variable("* another secret *"), Some(" another secret "));
        assert_eq!(is_secret_variable("**"), Some(""));

        // Test with braces
        assert_eq!(is_secret_variable("{secret}"), Some("secret"));
        assert_eq!(is_secret_variable("{ another secret }"), Some(" another secret "));
        assert_eq!(is_secret_variable("{}"), Some(""));

        // Test non-secret variables
        assert_eq!(is_secret_variable("not-secret"), None);
        assert_eq!(is_secret_variable("*not-secret"), None);
        assert_eq!(is_secret_variable("not-secret*"), None);
        assert_eq!(is_secret_variable("{not-secret"), None);
        assert_eq!(is_secret_variable("not-secret}"), None);
        assert_eq!(is_secret_variable(""), None);
        assert_eq!(is_secret_variable("*"), None);
        assert_eq!(is_secret_variable("{"), None);
        assert_eq!(is_secret_variable("}*"), None);
        assert_eq!(is_secret_variable("*{"), None);
    }
}
