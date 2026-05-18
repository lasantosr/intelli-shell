use std::sync::LazyLock;

use regex::Regex;
use unidecode::unidecode;

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
    /// Regex to match various newline sequences (`\r`, `\n`, `\r\n`)
    static NEW_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\r\n|\r|\n"#).unwrap());

    NEW_LINES.replace_all(str.as_ref(), "\n").to_string()
}

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
    /// Regex to match newline sequences potentially surrounded by whitespace.
    ///
    /// It also handles an optional backslash (`\`) preceding the newline, which might indicate an escaped newline in
    /// shell contexts.
    static NEW_LINE_AND_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\s*(\\)?(\r\n|\r|\n)\s*"#).unwrap());

    NEW_LINE_AND_SPACES.replace_all(str.as_ref(), " ").to_string()
}

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
    /// Regex to match any non-allowed character on the flattened version
    static FLAT_STRING_FORBIDDEN_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9\s-]").unwrap());

    flatten(s, &FLAT_STRING_FORBIDDEN_CHARS)
}

/// Normalizes a variable name string by performing ASCII transliteration and converting to lowercase.
///
/// This uses the [unidecode] crate to approximate non-ASCII characters with their closest ASCII equivalents, and then
/// converts the entire string to lowercase. Then, remove any non-allowed character and consecutive whitespaces,
/// returning the trimmed string.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::flatten_variable_name;
/// let variable = "  SÉCOND Part ";
/// let flattened = flatten_variable_name(variable);
/// assert_eq!(flattened, "second part");
/// ```
pub fn flatten_variable_name(variable_name: impl AsRef<str>) -> String {
    /// Regex to match any non-allowed character on the flattened version of a variable
    static VARIABLE_FORBIDDEN_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9\s_:-]").unwrap());

    flatten(variable_name, &VARIABLE_FORBIDDEN_CHARS)
}

fn flatten(s: impl AsRef<str>, forbidden_chars: &Regex) -> String {
    // Unidecode and lowercase
    let decoded = unidecode(s.as_ref()).to_lowercase();

    // Keep only allowed characters
    let flattened = forbidden_chars.replace_all(&decoded, "");

    /// Regex to match consecutive whitespaces
    static FLATTEN_COLLAPSE_WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    // Remove consecutive whitespaces
    FLATTEN_COLLAPSE_WHITESPACE_REGEX
        .replace_all(&flattened, " ")
        .trim()
        .to_string()
}

/// Extracts the root command from a shell command string, skipping environment variables and common prefixes
/// like `sudo`, `time`, etc., as well as shell operators like `&&` or `;`.
pub fn extract_root_cmd(command: &str) -> Option<String> {
    fn is_env_var(s: &str) -> bool {
        // Handle PowerShell: `$env:VAR=val`, Nushell `$env.VAR=val`
        let s = s.trim_start_matches("$env:").trim_start_matches("$env.");

        let mut parts = s.splitn(2, '=');
        let name = parts.next().unwrap_or("");

        if name.is_empty() || parts.next().is_none() {
            return false;
        }

        // Allow basic alphanumeric, underscore, and dots/colons from some shells
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':')
    }

    let parts = match shell_words::split(command) {
        Ok(p) => p,
        Err(_) => command.split_whitespace().map(|s| s.to_string()).collect(),
    };

    let mut skip_next_n = 0;

    for (i, part) in parts.iter().enumerate() {
        if skip_next_n > 0 {
            skip_next_n -= 1;
            continue;
        }

        let p = part.as_str();

        // Strip trailing semicolons or other separators that might have attached to the word in fallback parsing
        let p = p.strip_suffix(';').unwrap_or(p);

        if is_env_var(p) {
            continue;
        }

        match p {
            "&&" | "||" | ";" | "|" | "sudo" | "doas" | "time" | "env" | "function" | "def" | "def-env" | "export" => {
                continue;
            }
            // Nushell assignment like: `let-env VAR = "val"` or `$env.VAR = "val"`
            "let-env" | "let" | "mut" => {
                // Skips variable name, `=`, and value (e.g. `let-env VAR = val`)
                // In some cases it's just `let VAR = val`
                if parts.get(i + 2).map(|s| s.as_str()) == Some("=") {
                    skip_next_n = 3;
                } else if parts.get(i + 1).map(|s| s.as_str()) == Some("=") {
                    // maybe `let-env = val` ?
                    skip_next_n = 2;
                } else {
                    // let VAR val (unlikely in Nushell, but just to be safe)
                    skip_next_n = 2;
                }
                continue;
            }
            // Fish assignment like `set -x VAR val` or `set VAR val`
            "set" => {
                // Skip everything in `parts` until we hit a delimiter, then let the loop resume from there.
                let mut skipped = 0;
                for next_part in parts.iter().skip(i + 1) {
                    let next_part_stripped = next_part.as_str().strip_suffix(';').unwrap_or(next_part.as_str());
                    if matches!(next_part_stripped, ";" | "&&" | "||" | "|") {
                        break;
                    }
                    skipped += 1;
                    if next_part.as_str().ends_with(';') {
                        // The token has `;` attached to it (e.g. `val;`).
                        break;
                    }
                }
                skip_next_n = skipped;
                continue;
            }
            _ => {}
        }

        // if the part itself is `$env.VAR` and the next part is `=`
        if p.starts_with("$env.") || p.starts_with("$env:") {
            if parts.get(i + 1).map(|s| s.as_str()) == Some("=") {
                skip_next_n = 2; // skip `=` and `val`
                continue;
            }
        }

        if p.starts_with('-') {
            continue;
        }

        let trimmed = p.strip_suffix("()").unwrap_or(p).to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_root_cmd() {
        assert_eq!(extract_root_cmd("VAR1=val1 VAR2=\"val2\" root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("VAR4='value 4' && root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("VAR5=val\\ 5 ; root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("sudo root arg1 arg2").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("time sudo root arg1 arg2").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("env VAR=1 root arg1").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("root arg1").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd(""), None);
        assert_eq!(extract_root_cmd("VAR=val"), None);
        assert_eq!(extract_root_cmd("my_fn() { echo a; }").as_deref(), Some("my_fn"));
        assert_eq!(extract_root_cmd("function my_fn() { echo a; }").as_deref(), Some("my_fn"));
        assert_eq!(extract_root_cmd("function my_fn { echo a; }").as_deref(), Some("my_fn"));
        assert_eq!(extract_root_cmd("ENV={{variable-name:kebab}} function my_fn() { echo a; }").as_deref(), Some("my_fn"));

        // PowerShell
        assert_eq!(extract_root_cmd("$env:VAR=\"val\"; root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("$env:VAR=val; root argument").as_deref(), Some("root"));

        // Nushell
        assert_eq!(extract_root_cmd("let-env VAR = \"val\"; root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("let VAR = \"val\"; root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("$env.VAR = \"val\"; root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("def my_fn [] { echo a }").as_deref(), Some("my_fn"));
        assert_eq!(extract_root_cmd("def-env my_fn [] { echo a }").as_deref(), Some("my_fn"));

        // Fish
        assert_eq!(extract_root_cmd("env VAR=val root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("function my_fn; echo a; end").as_deref(), Some("my_fn"));
        assert_eq!(extract_root_cmd("export VAR=val; root argument").as_deref(), Some("root"));
        assert_eq!(extract_root_cmd("set -x VAR val; root argument").as_deref(), Some("root"));
    }
}
