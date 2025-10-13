use std::{
    collections::{BTreeMap, HashSet},
    convert::Infallible,
    fmt::{Display, Formatter},
    mem,
    str::FromStr,
};

use heck::ToShoutySnakeCase;
use itertools::Itertools;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

use super::VariableValue;
use crate::utils::{
    COMMAND_VARIABLE_REGEX, COMMAND_VARIABLE_REGEX_ALT, SplitCaptures, SplitItem, flatten_str, flatten_variable_name,
};

/// A command containing variables
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct CommandTemplate {
    /// The flattened root command (i.e., the first word)
    pub flat_root_cmd: String,
    /// The different parts of the command template, including variables or values
    pub parts: Vec<TemplatePart>,
}
impl CommandTemplate {
    /// Parses the given command as a [`CommandTemplate`]
    pub fn parse(cmd: impl AsRef<str>, alt: bool) -> Self {
        let cmd = cmd.as_ref();
        let regex = if alt {
            &COMMAND_VARIABLE_REGEX_ALT
        } else {
            &COMMAND_VARIABLE_REGEX
        };
        let splitter = SplitCaptures::new(regex, cmd);
        let parts = splitter
            .map(|e| match e {
                SplitItem::Unmatched(t) => TemplatePart::Text(t.to_owned()),
                SplitItem::Captured(c) => {
                    TemplatePart::Variable(Variable::parse(c.get(1).or(c.get(2)).unwrap().as_str()))
                }
            })
            .collect::<Vec<_>>();

        CommandTemplate {
            flat_root_cmd: flatten_str(cmd.split_whitespace().next().unwrap_or(cmd)),
            parts,
        }
    }

    /// Checks if the command has any variables without value
    pub fn has_pending_variable(&self) -> bool {
        self.parts.iter().any(|part| matches!(part, TemplatePart::Variable(_)))
    }

    /// Retrieves the previously selected values for the given flat variable name
    pub fn previous_values_for(&self, flat_variable_name: &str) -> Option<Vec<String>> {
        // Find all filled variables that match the flat name, collecting their unique values
        let values = self
            .parts
            .iter()
            .filter_map(|part| {
                if let TemplatePart::VariableValue(v, value) = part
                    && v.flat_name == flat_variable_name
                {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .unique()
            .collect::<Vec<_>>();

        if values.is_empty() { None } else { Some(values) }
    }

    /// Retrieves the first variable without value in the command
    pub fn current_variable(&self) -> Option<&Variable> {
        self.parts.iter().find_map(|part| {
            if let TemplatePart::Variable(v) = part {
                Some(v)
            } else {
                None
            }
        })
    }

    /// Retrieves the context for the current variable in the command
    pub fn current_variable_context(&self) -> BTreeMap<String, String> {
        self.parts
            .iter()
            .take_while(|part| !matches!(part, TemplatePart::Variable(_)))
            .filter_map(|part| {
                if let TemplatePart::VariableValue(v, value) = part
                    && !v.secret
                {
                    Some((v.flat_name.clone(), value.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Updates the first variable in the command for the given value
    pub fn set_next_variable(&mut self, value: impl Into<String>) {
        // Find the first part in the command that is an unfilled variable
        if let Some(part) = self.parts.iter_mut().find(|p| matches!(p, TemplatePart::Variable(_))) {
            // Replace it with the filled variable including the value
            if let TemplatePart::Variable(v) = mem::take(part) {
                *part = TemplatePart::VariableValue(v, value.into());
            } else {
                unreachable!();
            }
        }
    }

    /// Reverts the last set variable back to its pending state, returning the unset value
    pub fn unset_last_variable(&mut self) -> Option<String> {
        // Find the last part in the command that is a filled variable
        if let Some(part) = self
            .parts
            .iter_mut()
            .rfind(|p| matches!(p, TemplatePart::VariableValue(_, _)))
        {
            // Replace it with the unfilled variable, returning its value
            if let TemplatePart::VariableValue(v, value) = mem::take(part) {
                *part = TemplatePart::Variable(v);
                Some(value)
            } else {
                unreachable!();
            }
        } else {
            None
        }
    }

    /// Counts the total number of variables in the template (both filled and unfilled)
    pub fn count_variables(&self) -> usize {
        self.parts
            .iter()
            .filter(|part| matches!(part, TemplatePart::Variable(_) | TemplatePart::VariableValue(_, _)))
            .count()
    }

    /// Returns the variable at a specific index (0-based)
    pub fn variable_at_index(&self, index: usize) -> Option<&Variable> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                TemplatePart::Variable(v) | TemplatePart::VariableValue(v, _) => Some(v),
                _ => None,
            })
            .nth(index)
    }

    /// Sets the value for the variable at a specific index
    fn set_value_at_index(&mut self, index: usize, value: Option<String>) {
        let mut variable_count = 0;

        for part in self.parts.iter_mut() {
            if matches!(part, TemplatePart::Variable(_) | TemplatePart::VariableValue(_, _)) {
                if variable_count == index {
                    // Found the variable at the target index
                    let new_part = match (&part, &value) {
                        (TemplatePart::Variable(v), Some(val)) => {
                            // Convert Variable to VariableValue
                            Some(TemplatePart::VariableValue(v.clone(), val.clone()))
                        }
                        (TemplatePart::VariableValue(v, _), Some(val)) => {
                            // Update existing VariableValue
                            Some(TemplatePart::VariableValue(v.clone(), val.clone()))
                        }
                        (TemplatePart::VariableValue(v, _), None) => {
                            // Convert VariableValue back to Variable
                            Some(TemplatePart::Variable(v.clone()))
                        }
                        _ => None,
                    };

                    if let Some(new_part) = new_part {
                        *part = new_part;
                    }
                    return;
                }
                variable_count += 1;
            }
        }
    }

    /// Syncs the template parts with the given variable values array
    pub fn sync_with_values(&mut self, values: &[Option<String>]) {
        for (index, value) in values.iter().enumerate() {
            self.set_value_at_index(index, value.clone());
        }
    }

    /// Creates a [VariableValue] for this command with the given flat variable name and value
    pub fn new_variable_value_for(
        &self,
        flat_variable_name: impl Into<String>,
        value: impl Into<String>,
    ) -> VariableValue {
        VariableValue::new(&self.flat_root_cmd, flat_variable_name, value)
    }
}
impl Display for CommandTemplate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for part in self.parts.iter() {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

/// Represents a part of a command, which can be either text, a variable, or a variable value
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub enum TemplatePart {
    Text(String),
    Variable(Variable),
    VariableValue(Variable, String),
}
impl Default for TemplatePart {
    fn default() -> Self {
        Self::Text(String::new())
    }
}
impl Display for TemplatePart {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplatePart::Text(t) => write!(f, "{t}"),
            TemplatePart::Variable(v) => write!(f, "{{{{{}}}}}", v.display),
            TemplatePart::VariableValue(_, value) => write!(f, "{value}"),
        }
    }
}

/// Represents a variable from a command template
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct Variable {
    /// The variable as it was displayed on the command (e.g., `"Opt1|Opt2:lower:kebab"`)
    pub display: String,
    /// Parsed variable values derived from `display` (e.g., `["Opt1","Opt2"]`)
    pub options: Vec<String>,
    /// Flattened variable names derived from `options` (e.g., `["opt1","opt2"]`)
    pub flat_names: Vec<String>,
    /// Flattened variable name derived from `flat_names` (e.g., `"opt1|opt2"`)
    pub flat_name: String,
    /// Parsed variable functions to apply (e.g., `["lower","kebab"]`)
    pub functions: Vec<VariableFunction>,
    /// Whether the variable is secret
    pub secret: bool,
}
impl Variable {
    /// Parses a variable text into its model
    pub fn parse(text: impl Into<String>) -> Self {
        let display: String = text.into();

        // Determine if the variable is secret or not
        let (content, secret) = match is_secret_variable(&display) {
            Some(inner) => (inner, true),
            None => (display.as_str(), false),
        };

        // Split the variable content in parts
        let parts: Vec<&str> = content.split(':').collect();
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
        let (options, flat_names) = if options_str.is_empty() {
            (vec![], vec![])
        } else {
            let mut options = Vec::new();
            let mut flat_names = Vec::new();
            let mut seen_options = HashSet::new();
            let mut seen_flat_names = HashSet::new();

            for option in options_str
                .split('|')
                .map(|o| o.trim())
                .filter(|o| !o.is_empty())
                .map(String::from)
            {
                if seen_options.insert(option.clone()) {
                    let flat_name = flatten_variable_name(&option);
                    if seen_flat_names.insert(flat_name.clone()) {
                        flat_names.push(flat_name);
                    }
                    options.push(option);
                }
            }

            (options, flat_names)
        };

        // Build back the flat name for this variable
        let flat_name = flat_names.iter().join("|");

        Self {
            display,
            options,
            flat_names,
            flat_name,
            functions,
            secret,
        }
    }

    /// Retrieves the env var names where the value for this variable might reside (in order of preference)
    pub fn env_var_names(&self, include_individual: bool) -> HashSet<String> {
        let mut names = HashSet::new();
        let env_var_name = self.display.to_shouty_snake_case();
        if !env_var_name.trim().is_empty() && env_var_name.trim() != "PATH" {
            names.insert(env_var_name.trim().to_owned());
        }
        let env_var_name_no_fn = self.flat_name.to_shouty_snake_case();
        if !env_var_name_no_fn.trim().is_empty() && env_var_name_no_fn.trim() != "PATH" {
            names.insert(env_var_name_no_fn.trim().to_owned());
        }
        if include_individual {
            names.extend(
                self.flat_names
                    .iter()
                    .map(|o| o.to_shouty_snake_case())
                    .filter(|o| !o.trim().is_empty())
                    .filter(|o| o.trim() != "PATH")
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
    UrlEncode,
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
            Self::UrlEncode => idempotent_percent_encode(input, NON_ALPHANUMERIC),
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
            Self::UrlEncode => {
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

/// Idempotent percent-encodes a string.
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
        let cmd = CommandTemplate::parse("git commit -m {{{message}}} --author {{author:kebab}}", false);
        assert_eq!(cmd.flat_root_cmd, "git");
        assert_eq!(cmd.parts.len(), 4);
        assert_eq!(cmd.parts[0], TemplatePart::Text("git commit -m ".into()));
        assert!(matches!(cmd.parts[1], TemplatePart::Variable(_)));
        assert_eq!(cmd.parts[2], TemplatePart::Text(" --author ".into()));
        assert!(matches!(cmd.parts[3], TemplatePart::Variable(_)));
    }

    #[test]
    fn test_parse_command_no_variables() {
        let cmd = CommandTemplate::parse("echo 'hello world'", false);
        assert_eq!(cmd.flat_root_cmd, "echo");
        assert_eq!(cmd.parts.len(), 1);
        assert_eq!(cmd.parts[0], TemplatePart::Text("echo 'hello world'".into()));
    }

    #[test]
    fn test_set_next_variable() {
        let mut cmd = CommandTemplate::parse("cmd {{var1}} {{var2}}", false);
        cmd.set_next_variable("value1");
        let var1 = Variable::parse("var1");
        assert_eq!(cmd.parts[1], TemplatePart::VariableValue(var1, "value1".into()));
        cmd.set_next_variable("value2");
        let var2 = Variable::parse("var2");
        assert_eq!(cmd.parts[3], TemplatePart::VariableValue(var2, "value2".into()));
    }

    #[test]
    fn test_unset_last_variable() {
        let mut cmd = CommandTemplate::parse("cmd {{var1}} {{var2}}", false);

        // Set both variables to check the initial state
        cmd.set_next_variable("value1");
        cmd.set_next_variable("value2");
        assert!(!cmd.has_pending_variable());
        let var1 = Variable::parse("var1");
        let var2 = Variable::parse("var2");
        assert_eq!(cmd.parts[1], TemplatePart::VariableValue(var1.clone(), "value1".into()));
        assert_eq!(cmd.parts[3], TemplatePart::VariableValue(var2.clone(), "value2".into()));

        // Unset the last variable (var2) and check the returned value
        let unset_value2 = cmd.unset_last_variable();
        assert_eq!(unset_value2, Some("value2".to_string()));
        assert!(cmd.has_pending_variable());
        assert_eq!(cmd.current_variable().unwrap(), &var2);
        assert_eq!(cmd.parts[1], TemplatePart::VariableValue(var1.clone(), "value1".into()));
        assert_eq!(cmd.parts[3], TemplatePart::Variable(var2));

        // Unset the last variable again (var1) and check the returned value
        let unset_value1 = cmd.unset_last_variable();
        assert_eq!(unset_value1, Some("value1".to_string()));
        assert!(cmd.has_pending_variable());
        assert_eq!(cmd.current_variable().unwrap(), &var1);
        assert_eq!(cmd.parts[1], TemplatePart::Variable(var1));

        // Unset again when no variables are set, should return None
        let no_unset_value = cmd.unset_last_variable();
        assert_eq!(no_unset_value, None);
    }

    #[test]
    fn test_has_pending_variable() {
        let mut cmd = CommandTemplate::parse("cmd {{var1}} {{var2}}", false);
        assert!(cmd.has_pending_variable());
        cmd.set_next_variable("value1");
        assert!(cmd.has_pending_variable());
        cmd.set_next_variable("value2");
        assert!(!cmd.has_pending_variable());
    }

    #[test]
    fn test_current_variable() {
        let mut cmd = CommandTemplate::parse("cmd {{var1}} {{var2}}", false);
        assert_eq!(cmd.current_variable().map(|l| l.flat_name.as_str()), Some("var1"));
        cmd.set_next_variable("value1");
        assert_eq!(cmd.current_variable().map(|l| l.flat_name.as_str()), Some("var2"));
        cmd.set_next_variable("value2");
        assert_eq!(cmd.current_variable(), None);
    }

    #[test]
    fn test_current_variable_context() {
        let mut cmd = CommandTemplate::parse("cmd {{var1}} {{{secret_var}}} {{var2}}", false);

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
        let cmd = CommandTemplate::parse("cmd {{var1}}", false);
        let context: Vec<_> = cmd.current_variable_context().into_iter().collect();
        assert!(context.is_empty());
    }

    #[test]
    fn test_parse_simple_variable() {
        let variable = Variable::parse("my_variable");
        assert_eq!(
            variable,
            Variable {
                display: "my_variable".into(),
                options: vec!["my_variable".into()],
                flat_names: vec!["my_variable".into()],
                flat_name: "my_variable".into(),
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
                display: "{my_secret}".into(),
                options: vec!["my_secret".into()],
                flat_names: vec!["my_secret".into()],
                flat_name: "my_secret".into(),
                functions: vec![],
                secret: true,
            }
        );
    }

    #[test]
    fn test_parse_variable_with_multiple_options() {
        let variable = Variable::parse("Option 1 | option 1 | Option 2 | Option 2 | Option 3");
        assert_eq!(
            variable,
            Variable {
                display: "Option 1 | option 1 | Option 2 | Option 2 | Option 3".into(),
                options: vec![
                    "Option 1".into(),
                    "option 1".into(),
                    "Option 2".into(),
                    "Option 3".into()
                ],
                flat_names: vec!["option 1".into(), "option 2".into(), "option 3".into()],
                flat_name: "option 1|option 2|option 3".into(),
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
                display: "my_variable:kebab".into(),
                options: vec!["my_variable".into()],
                flat_names: vec!["my_variable".into()],
                flat_name: "my_variable".into(),
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
                display: "my_variable:snake:upper".into(),
                options: vec!["my_variable".into()],
                flat_names: vec!["my_variable".into()],
                flat_name: "my_variable".into(),
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
                display: "opt1|opt2:lower:kebab".into(),
                options: vec!["opt1".into(), "opt2".into()],
                flat_names: vec!["opt1".into(), "opt2".into()],
                flat_name: "opt1|opt2".into(),
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
                display: "key:value:kebab".into(),
                options: vec!["key:value".into()],
                flat_names: vec!["key:value".into()],
                flat_name: "key:value".into(),
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
                display: ":snake".into(),
                options: vec![],
                flat_names: vec![],
                flat_name: "".into(),
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
                display: "kebab".into(),
                options: vec!["kebab".into()],
                flat_names: vec!["kebab".into()],
                flat_name: "kebab".into(),
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
        assert_eq!(VariableFunction::UrlEncode.apply_to("some text"), "some%20text");
        assert_eq!(VariableFunction::UrlEncode.apply_to("Some Text"), "Some%20Text");
        assert_eq!(VariableFunction::UrlEncode.apply_to("some-text"), "some%2Dtext");
        assert_eq!(VariableFunction::UrlEncode.apply_to("some_text"), "some%5Ftext");
        assert_eq!(
            VariableFunction::UrlEncode.apply_to("!@#$%^&*()"),
            "%21%40%23%24%25%5E%26%2A%28%29"
        );
        assert_eq!(VariableFunction::UrlEncode.apply_to("some%20text"), "some%20text");
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

        // UrlEncode
        assert_eq!(VariableFunction::UrlEncode.check_char(' '), Some("%20".to_string()));
        assert_eq!(VariableFunction::UrlEncode.check_char('!'), Some("%21".to_string()));
        assert_eq!(VariableFunction::UrlEncode.check_char('A'), None);
        assert_eq!(VariableFunction::UrlEncode.check_char('1'), None);
        assert_eq!(VariableFunction::UrlEncode.check_char('-'), Some("%2D".to_string()));
        assert_eq!(VariableFunction::UrlEncode.check_char('_'), Some("%5F".to_string()));
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
