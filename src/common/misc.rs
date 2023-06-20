use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::{CaptureMatches, Captures, Regex};
use unicode_segmentation::UnicodeSegmentation;
use unidecode::unidecode;

/// Regex to match newlines
static NEW_LINES: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\r|\n|\r\n"#).unwrap());

/// Converts all newline kinds to `\n`
pub fn unify_newlines(str: impl AsRef<str>) -> String {
    NEW_LINES.replace_all(str.as_ref(), "\n").to_string()
}

/// Regex to match spaces
static NEW_LINE_AND_SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(\\)?(\r|\n|\r\n)\s*"#).unwrap());

/// Removes newlines
pub fn remove_newlines(str: impl AsRef<str>) -> String {
    NEW_LINE_AND_SPACES.replace_all(str.as_ref(), "").to_string()
}

/// Applies [unidecode] to the given string and then converts it to lower case
pub fn flatten_str(s: impl AsRef<str>) -> String {
    unidecode(s.as_ref()).to_lowercase()
}

/// Iterator to split a test by a regex and capture both unmatched and captured groups
pub struct SplitCaptures<'r, 't> {
    finder: CaptureMatches<'r, 't>,
    text: &'t str,
    last: usize,
    caps: Option<Captures<'t>>,
}

impl<'r, 't> SplitCaptures<'r, 't> {
    /// Builds a new [SplitCaptures]
    pub fn new(re: &'r Regex, text: &'t str) -> SplitCaptures<'r, 't> {
        SplitCaptures {
            finder: re.captures_iter(text),
            text,
            last: 0,
            caps: None,
        }
    }
}

/// Represents each item of a [SplitCaptures]
#[derive(Debug)]
pub enum SplitItem<'t> {
    Unmatched(&'t str),
    Captured(Captures<'t>),
}

impl<'r, 't> Iterator for SplitCaptures<'r, 't> {
    type Item = SplitItem<'t>;

    fn next(&mut self) -> Option<SplitItem<'t>> {
        if let Some(caps) = self.caps.take() {
            return Some(SplitItem::Captured(caps));
        }
        match self.finder.next() {
            None => {
                if self.last >= self.text.len() {
                    None
                } else {
                    let s = &self.text[self.last..];
                    self.last = self.text.len();
                    Some(SplitItem::Unmatched(s))
                }
            }
            Some(caps) => {
                let m = caps.get(0).unwrap();
                let unmatched = &self.text[self.last..m.start()];
                self.last = m.end();
                self.caps = Some(caps);
                Some(SplitItem::Unmatched(unmatched))
            }
        }
    }
}

/// String utilities to work with [grapheme clusters](https://doc.rust-lang.org/book/ch08-02-strings.html#bytes-and-scalar-values-and-grapheme-clusters-oh-my)
pub trait StringExt {
    /// Inserts a `char` at a given char index position.
    ///
    /// Unlike [`String::insert`](String::insert), the index is char-based, not byte-based.
    fn insert_safe(&mut self, char_index: usize, c: char);

    /// Inserts an `String` at a given char index position.
    ///
    /// Unlike [`String::insert`](String::insert), the index is char-based, not byte-based.
    fn insert_safe_str(&mut self, char_index: usize, str: impl Into<String>);

    /// Removes a `char` at a given char index position.
    ///
    /// Unlike [`String::remove`](String::remove), the index is char-based, not byte-based.
    fn remove_safe(&mut self, char_index: usize);
}
pub trait StrExt {
    /// Returns the number of characters.
    ///
    /// Unlike [`String::len`](String::len), the number is char-based, not byte-based.
    fn len_chars(&self) -> usize;
}

impl StringExt for String {
    fn insert_safe(&mut self, char_index: usize, new_char: char) {
        let mut v = self.graphemes(true).map(ToOwned::to_owned).collect_vec();
        v.insert(char_index, new_char.to_string());
        *self = v.join("");
    }

    fn insert_safe_str(&mut self, char_index: usize, str: impl Into<String>) {
        let mut v = self.graphemes(true).map(ToOwned::to_owned).collect_vec();
        v.insert(char_index, str.into());
        *self = v.join("");
    }

    fn remove_safe(&mut self, char_index: usize) {
        *self = self
            .graphemes(true)
            .enumerate()
            .filter_map(|(i, c)| if i != char_index { Some(c) } else { None })
            .collect_vec()
            .join("");
    }
}

impl StrExt for String {
    fn len_chars(&self) -> usize {
        self.graphemes(true).count()
    }
}

impl StrExt for str {
    fn len_chars(&self) -> usize {
        self.graphemes(true).count()
    }
}
