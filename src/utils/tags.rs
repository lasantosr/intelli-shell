use std::{collections::HashSet, sync::LazyLock};

use regex::Regex;
use unicode_width::UnicodeWidthChar;

/// Characters to be trimmed from the raw tag body
const TRIM_CHARS_PATTERN: &str = r#".,!?;:)[]{}'"`<>-_\/"#;

/// Extracts hashtags from a description string
pub fn extract_tags_from_description(description: Option<&str>) -> Option<Vec<String>> {
    let output = process_text_for_tags(description?, false, None)?;
    Some(output.all_tags)
}

/// Extracts hashtags from a string and returns them along with the cleaned text.
///
/// The cleaned text has all hashtag occurrences (including the preceding space, if any) removed.
pub fn extract_tags_and_cleaned_text(text: &str) -> Option<(Vec<String>, String)> {
    let output = process_text_for_tags(text, true, None)?;
    Some((output.all_tags, output.cleaned_text.unwrap()))
}

/// Extracts hashtags from a string and returns the tag where the cursor is placed, other tags found on the text and the
/// cleaned text.
///
/// The cleaned text has all hashtag occurrences removed.
pub fn extract_tags_with_editing_and_cleaned_text(
    text: &str,
    cursor_pos: usize,
) -> Option<(String, Vec<String>, String)> {
    let output = process_text_for_tags(text, true, Some(cursor_pos))?;
    let editing_tag = output.editing_tag?;
    Some((editing_tag, output.all_tags, output.cleaned_text.unwrap()))
}

/// Holds all possible outputs from the core tag processing logic
struct ExtractionOutput {
    /// A set of all unique, processed tags found in the text
    all_tags: Vec<String>,
    /// The specific tag the cursor is on, if a cursor position was provided
    editing_tag: Option<String>,
    /// The text with all tag occurrences removed, if requested
    cleaned_text: Option<String>,
}
/// Inner helper function to extract tags and optionally clean the description
fn process_text_for_tags(text: &str, clean_text: bool, cursor_pos: Option<usize>) -> Option<ExtractionOutput> {
    if text.is_empty() {
        return None;
    }

    /// Regex to match hashtags in the description.
    ///
    /// The regex captures:
    /// - The start of the string or a whitespace character before the hashtag
    /// - The hashtag itself, which is defined as a '#' followed by one or more non-whitespace characters
    ///
    /// The regex uses a named capture group `raw_body` to extract the content of the hashtag.
    static HASHTAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)#(?P<raw_body>[^\s]*)").unwrap());

    let mut all_tags = HashSet::new();
    let mut editing_tag: Option<String> = None;
    let mut ranges_to_remove = if clean_text { Some(Vec::new()) } else { None };

    // Convert width-based cursor_pos to byte-based cursor_idx if provided
    let byte_cursor_idx = cursor_pos.map(|width_idx| width_to_byte_offset(text, width_idx));

    for captures in HASHTAG_REGEX.captures_iter(text) {
        let full_match = captures.get(0).unwrap();
        let raw_body_capture = captures.name("raw_body").unwrap();
        let raw_body_str = raw_body_capture.as_str();

        let mut editing = false;
        if let Some(idx) = byte_cursor_idx
            && editing_tag.is_none()
            && idx >= raw_body_capture.start()
            && idx <= full_match.end()
        {
            editing_tag = Some(format!("#{raw_body_str}"));
            editing = true;
        }

        if !editing {
            let trimmed_body = raw_body_str.trim_matches(|c: char| TRIM_CHARS_PATTERN.contains(c));
            if !trimmed_body.is_empty() {
                let processed_tag = format!("#{trimmed_body}").to_lowercase();
                all_tags.insert(processed_tag);
            }
        }

        // If cleaning text, record the span of the full match to be removed later.
        if let Some(ref mut ranges) = ranges_to_remove {
            ranges.push(full_match.range());
        }
    }

    // If no tags were found when not editing, or no editing tag was found
    if (cursor_pos.is_none() && all_tags.is_empty()) || (cursor_pos.is_some() && editing_tag.is_none()) {
        return None;
    }

    // Generate the cleaned text by building a new string from the parts not in the removal ranges
    let cleaned_text = ranges_to_remove.map(|ranges| {
        let mut new_text = String::with_capacity(text.len());
        let mut last_end = 0;
        for range in ranges {
            new_text.push_str(&text[last_end..range.start]);
            last_end = range.end;
        }
        new_text.push_str(&text[last_end..]);
        new_text
    });

    let mut sorted_all_tags: Vec<String> = all_tags.into_iter().collect();
    sorted_all_tags.sort_unstable();

    Some(ExtractionOutput {
        all_tags: sorted_all_tags,
        editing_tag,
        cleaned_text,
    })
}
fn width_to_byte_offset(text: &str, target_width: usize) -> usize {
    let mut current_width = 0;
    let mut last_byte_offset = 0;
    for (byte_offset, c) in text.char_indices() {
        if current_width >= target_width {
            return byte_offset;
        }
        last_byte_offset = byte_offset;
        current_width += c.width().unwrap_or(0);
    }
    if current_width >= target_width {
        return last_byte_offset + text[last_byte_offset..].chars().next().map_or(0, |c| c.len_utf8());
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_hashtags() {
        // Test with text that contains no hashtags
        let text = "This is a simple text without any tags.";
        assert_eq!(extract_tags_and_cleaned_text(text), None);
    }

    #[test]
    fn test_simple_hashtag_at_start() {
        let text = "#hello world";
        let expected_tags = vec!["#hello".to_string()];
        if let Some((tags, cleaned_text)) = extract_tags_and_cleaned_text(text) {
            assert_eq!(tags, expected_tags);
            assert_eq!(cleaned_text, " world");
        } else {
            panic!("Expected Some, got None");
        }
    }

    #[test]
    fn test_multiple_hashtags() {
        let text = "Great #weather for #coding today!";
        let expected_tags = vec!["#coding".to_string(), "#weather".to_string()];
        if let Some((tags, cleaned_text)) = extract_tags_and_cleaned_text(text) {
            assert_eq!(tags, expected_tags);
            assert_eq!(cleaned_text, "Great for today!");
        } else {
            panic!("Expected Some, got None");
        }
    }

    #[test]
    fn test_duplicate_hashtags() {
        let text = "#Tag1 is good, #tag1 is better, #TAG1 is best.";
        let expected_tags = vec!["#tag1".to_string()];
        if let Some((tags, cleaned_text)) = extract_tags_and_cleaned_text(text) {
            assert_eq!(tags, expected_tags);
            assert_eq!(cleaned_text, " is good, is better, is best.");
        } else {
            panic!("Expected Some, got None");
        }
    }

    #[test]
    fn test_hashtags_with_trim_chars() {
        let text = "Check out #cool-stuff. and #another_Tag!";
        let expected_tags = vec!["#another_tag".to_string(), "#cool-stuff".to_string()];
        if let Some((tags, cleaned_text)) = extract_tags_and_cleaned_text(text) {
            assert_eq!(tags, expected_tags);
            assert_eq!(cleaned_text, "Check out and");
        } else {
            panic!("Expected Some, got None");
        }
    }

    #[test]
    fn test_url_anchor_tag_not_extracted() {
        let text = "Visit example.com/#section1 for more info.";
        assert_eq!(extract_tags_and_cleaned_text(text), None);
    }

    #[test]
    fn test_text_is_only_hashtags() {
        let text = "#only #tags #here";
        let expected_tags = vec!["#here".to_string(), "#only".to_string(), "#tags".to_string()];
        if let Some((tags, cleaned_text)) = extract_tags_and_cleaned_text(text) {
            assert_eq!(tags, expected_tags);
            assert_eq!(cleaned_text, "");
        } else {
            panic!("Expected Some, got None");
        }
    }

    #[test]
    fn test_extract_editing_tag_no_tags() {
        let text = "No hashtags here";
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 5), None);
    }

    #[test]
    fn test_extract_editing_tag_only() {
        let text = "#";
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 1).unwrap().0, "#");
    }

    #[test]
    fn test_extract_editing_tag() {
        let text = "This has #one tag";

        // Outside of the tag
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 3), None);
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 18), None);

        // Before hastag
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 9), None);

        // After hasthag
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 10).unwrap().0, "#one");

        // Right after the tag
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 13).unwrap().0, "#one");

        // After the tag
        assert_eq!(extract_tags_with_editing_and_cleaned_text(text, 14), None);
    }

    #[test]
    fn test_extract_editing_tag_multiple() {
        let text = "This has #one tag after #another";
        let (editing_tag, all_tags, cleaned_text) = extract_tags_with_editing_and_cleaned_text(text, 11).unwrap();
        assert_eq!(editing_tag, "#one");
        assert_eq!(all_tags, vec!["#another"]);
        assert_eq!(cleaned_text, "This has tag after");
    }

    #[test]
    fn test_extract_editing_tag_empty() {
        let text = "This has # tag after #another";
        let (editing_tag, all_tags, cleaned_text) = extract_tags_with_editing_and_cleaned_text(text, 10).unwrap();
        assert_eq!(editing_tag, "#");
        assert_eq!(all_tags, vec!["#another"]);
        assert_eq!(cleaned_text, "This has tag after");
    }

    #[test]
    fn test_extract_editing_tag_cursor_after_tag_unicode() {
        let text = "This has 日本語 and emoji 🚀, including #tags and other stuff";
        let (editing_tag, all_tags, cleaned_text) = extract_tags_with_editing_and_cleaned_text(text, 41).unwrap();
        assert_eq!(editing_tag, "#tags");
        assert!(all_tags.is_empty());
        assert_eq!(cleaned_text, "This has 日本語 and emoji 🚀, including and other stuff");
    }
}
