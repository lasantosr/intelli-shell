use std::sync::LazyLock;

use itertools::Itertools;
use regex::Regex;
use unidecode::unidecode;

/// Regex to match various newline sequences (`\r`, `\n`, `\r\n`)
static NEW_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\r\n|\r|\n"#).unwrap());

/// Converts all types of newline sequences (`\r`, `\n`, `\r\n`) in a string to a single newline character (`\n`).
///
/// This is useful for normalizing text input that might come from different operating systems or sources with
/// inconsistent line endings.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::unify_newlines;
/// let text = "Hello\r\nWorld\nAnother\rLine";
/// let unified = unify_newlines(text);
/// assert_eq!(unified, "Hello\nWorld\nAnother\nLine");
/// ```
pub fn unify_newlines(str: impl AsRef<str>) -> String {
    NEW_LINES.replace_all(str.as_ref(), "\n").to_string()
}

/// Regex to match newline sequences potentially surrounded by whitespace.
///
/// It also handles an optional backslash (`\`) preceding the newline, which might indicate an escaped newline in shell
/// contexts.
static NEW_LINE_AND_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\s*(\\)?(\r\n|\r|\n)\s*"#).unwrap());

/// Removes newline sequences and any surrounding whitespace, replacing them with a single space.
///
/// This function is useful for converting multi-line text into a single line while preserving word separation.
/// It collapses multiple lines and adjacent whitespace into one space.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::remove_newlines;
/// let text = "Line 1\n  Line 2 \r\n\tLine 3";
/// let single_line = remove_newlines(text);
/// assert_eq!(single_line, "Line 1 Line 2 Line 3");
///
/// // Example with potentially escaped newline
/// let text_escaped = "Line A \\\n Line B";
/// let single_line_escaped = remove_newlines(text_escaped);
/// assert_eq!(single_line_escaped, "Line A Line B");
/// ```
pub fn remove_newlines(str: impl AsRef<str>) -> String {
    NEW_LINE_AND_SPACES.replace_all(str.as_ref(), " ").to_string()
}

/// Regex to match any non-allowed character on the flattened version
static FLATTEN_KEEP_CHARS_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9\s-]").unwrap());
/// Regex to match consecutive whitespaces
static FLATTEN_COLLAPSE_WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Normalizes a string by performing ASCII transliteration and converting to lowercase.
///
/// This uses the [unidecode] crate to approximate non-ASCII characters with their closest ASCII equivalents, and then
/// converts the entire string to lowercase. Then, remove any non-alphanumeric character and consecutive whitespaces,
/// returning the trimmed string.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::flatten_str;
/// let text = "Héllö Wörld! (-123) ";
/// let flattened = flatten_str(text);
/// assert_eq!(flattened, "hello world -123");
/// ```
pub fn flatten_str(s: impl AsRef<str>) -> String {
    // Unidecode and lowercase
    let decoded = unidecode(s.as_ref()).to_lowercase();

    // Keep only alphanumeric characters and whitespace.
    let flattened = FLATTEN_KEEP_CHARS_REGEX.replace_all(&decoded, "");

    // Remove consecutive whitespaces
    FLATTEN_COLLAPSE_WHITESPACE_REGEX
        .replace_all(&flattened, " ")
        .trim()
        .to_string()
}

/// Normalizes a variable name string that may contain multiple segments separated by `|`.
///
/// Each segment is individually processed by [`flatten_str`].
///
/// After processing, any segments that become empty are removed. The remaining non-empty,
/// flattened segments are then joined back together with `|`.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::flatten_variable;
/// let variable = "  First Segment | SÉCOND Part |  | Last One! || ";
/// let flattened = flatten_variable(variable);
/// assert_eq!(flattened, "first segment|second part|last one");
/// ```
pub fn flatten_variable(variable: impl AsRef<str>) -> String {
    variable
        .as_ref()
        .split('|')
        .map(str::trim)
        .map(flatten_str)
        .filter(|s| !s.is_empty())
        .join("|")
}
