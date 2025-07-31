use std::sync::LazyLock;

use regex::{CaptureMatches, Captures, Regex};

const VARIABLE_REGEX: &str = r"\{\{((?:\{[^}]+\}|[^}]+))\}\}";

/// Regex to match variables from a command, with a capturing group for the name
pub static COMMAND_VARIABLE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(VARIABLE_REGEX).unwrap());

/// Regex to match variables from a command, with a capturing group for the name.
///
/// This regex identifies if variables are unquoted, single-quoted, or double-quoted:
/// - Group 1: Will exist if a single-quoted placeholder like '{{name}}' is matched
/// - Group 2: Will exist if a double-quoted placeholder like "{{name}}" is matched
/// - Group 3: Will exist if an unquoted placeholder like {{name}} is matched
pub static COMMAND_VARIABLE_REGEX_QUOTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"'{VARIABLE_REGEX}'|"{VARIABLE_REGEX}"|{VARIABLE_REGEX}"#)).unwrap());

/// An iterator that splits a text string based on a regular expression, yielding both the substrings that _don't_ match
/// the regex and the `Captures` objects for the parts that _do_ match.
///
/// This is useful when you need to process parts of a string separated by delimiters defined by a regex, but you also
/// need access to the captured groups within those delimiters.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::{SplitCaptures, SplitItem};
/// # use regex::Regex;
/// let regex = Regex::new(r"\{(\w+)\}").unwrap();
/// let text = "Hello {name}, welcome to {place}!";
/// let mut parts = vec![];
/// for item in SplitCaptures::new(&regex, text) {
///     match item {
///         SplitItem::Unmatched(s) => parts.push(format!("Unmatched: '{}'", s)),
///         SplitItem::Captured(caps) => {
///             parts.push(format!("Captured: '{}', Group 1: '{}'", &caps[0], &caps[1]))
///         }
///     }
/// }
/// assert_eq!(
///     parts,
///     vec![
///         "Unmatched: 'Hello '",
///         "Captured: '{name}', Group 1: 'name'",
///         "Unmatched: ', welcome to '",
///         "Captured: '{place}', Group 1: 'place'",
///         "Unmatched: '!'",
///     ]
/// );
/// ```
pub struct SplitCaptures<'r, 't> {
    /// Iterator over regex captures
    finder: CaptureMatches<'r, 't>,
    /// The original text being split
    text: &'t str,
    /// The byte index marking the end of the last match/unmatched part
    last: usize,
    /// Holds the captures of the _next_ match to be returned
    caps: Option<Captures<'t>>,
}

impl<'r, 't> SplitCaptures<'r, 't> {
    /// Creates a new [SplitCaptures] iterator.
    pub fn new(regex: &'r Regex, text: &'t str) -> SplitCaptures<'r, 't> {
        SplitCaptures {
            finder: regex.captures_iter(text),
            text,
            last: 0,
            caps: None,
        }
    }
}

/// Represents an item yielded by the [SplitCaptures] iterator.
///
/// It can be either a part of the string that did not match the regex ([Unmatched](SplitItem::Unmatched)) or the
/// [Captures] object from a part that did match ([Captured](SplitItem::Captured)).
#[derive(Debug)]
pub enum SplitItem<'t> {
    /// A string slice that did not match the regex separator
    Unmatched(&'t str),
    /// The [Captures] object resulting from a regex match
    Captured(Captures<'t>),
}

impl<'t> Iterator for SplitCaptures<'_, 't> {
    type Item = SplitItem<'t>;

    /// Advances the iterator, returning the next unmatched slice or captured group.
    ///
    /// The iterator alternates between returning `SplitItem::Unmatched` and `SplitItem::Captured`,
    /// starting and ending with `Unmatched` (unless the string is empty or fully matched).
    fn next(&mut self) -> Option<SplitItem<'t>> {
        // If we have pending captures from the previous iteration, return them now
        if let Some(caps) = self.caps.take() {
            return Some(SplitItem::Captured(caps));
        }
        // Find the next match using the internal captures iterator
        match self.finder.next() {
            // No more matches found
            None => {
                if self.last >= self.text.len() {
                    None
                } else {
                    // If there's remaining text after the last match (or if the string was never matched), return it.
                    // Get the final unmatched slice
                    let s = &self.text[self.last..];
                    // Mark the end of the string as processed
                    self.last = self.text.len();
                    Some(SplitItem::Unmatched(s))
                }
            }
            // A match was found
            Some(caps) => {
                // Get the match bounds
                let m = caps.get(0).unwrap();
                // Extract the text between the end of the last item and the start of this match
                let unmatched = &self.text[self.last..m.start()];
                // Update the position to the end of the current match
                self.last = m.end();
                // Store the captures to be returned in the next iteration
                self.caps = Some(caps);
                // Return the unmatched part before the captures
                Some(SplitItem::Unmatched(unmatched))
            }
        }
    }
}
