use crate::config::RegexWrapper;

/// Checks whether a command string is destructive based on its tags and regex patterns.
pub fn is_destructive(command: &str, tags: &[String], patterns: &[RegexWrapper]) -> bool {
    // 1. Tag-Based Detection (Always-On)
    if tags.iter().any(|t| t == "#destructive") {
        return true;
    }

    // 2. Config-Based Regex Detection
    if patterns.is_empty() {
        return false;
    }

    let segments = split_shell_segments(command);
    for pattern in patterns {
        for segment in &segments {
            let trimmed = segment.trim();
            if !trimmed.is_empty() && pattern.is_match(trimmed) {
                return true;
            }
        }
    }

    false
}

fn split_shell_segments(command: &str) -> Vec<&str> {
    let bytes = command.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut quote: Option<u8> = None;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if escaped {
            escaped = false;
            index += 1;
            continue;
        }

        if let Some(active_quote) = quote {
            if byte == b'\\' && active_quote == b'"' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }

        match byte {
            b'\\' => {
                escaped = true;
                index += 1;
            }
            b'\'' | b'"' => {
                quote = Some(byte);
                index += 1;
            }
            b';' | b'\n' => {
                segments.push(&command[start..index]);
                start = index + 1;
                index += 1;
            }
            b'&' if bytes.get(index + 1) == Some(&b'&') => {
                segments.push(&command[start..index]);
                start = index + 2;
                index += 2;
            }
            b'|' if bytes.get(index + 1) == Some(&b'|') => {
                segments.push(&command[start..index]);
                start = index + 2;
                index += 2;
            }
            b'|' => {
                segments.push(&command[start..index]);
                start = index + 1;
                index += 1;
            }
            _ => index += 1,
        }
    }

    segments.push(&command[start..]);
    segments
}

#[cfg(test)]
mod tests {
    use super::is_destructive;
    use crate::config::RegexWrapper;
    use regex::Regex;

    fn make_patterns(pats: &[&str]) -> Vec<RegexWrapper> {
        pats.iter()
            .map(|p| RegexWrapper::new(Regex::new(p).unwrap()))
            .collect()
    }

    #[test]
    fn test_tag_based_detection() {
        // Tag-based check is always-on and triggers if '#destructive' tag is present
        assert!(is_destructive("echo safe", &["#destructive".to_string()], &[]));
        assert!(is_destructive("rm -rf /", &["#destructive".to_string()], &[]));
        assert!(is_destructive(
            "some-command",
            &["#other".to_string(), "#destructive".to_string()],
            &[]
        ));

        // If '#destructive' is not present, it should not trigger without patterns
        assert!(!is_destructive("rm -rf /", &["#safe".to_string()], &[]));
    }

    #[test]
    fn test_regex_patterns_detection() {
        let patterns = make_patterns(&["^rm\\b", "^del\\b"]);

        // Matches segment starting with rm or del
        assert!(is_destructive("rm -rf /", &[], &patterns));
        assert!(is_destructive("del file.txt", &[], &patterns));
        assert!(is_destructive("echo ok && rm -rf /", &[], &patterns));
        assert!(is_destructive("rm -rf / | echo", &[], &patterns));

        // Negative cases that should not match
        assert!(!is_destructive("echo rm file", &[], &patterns));
        assert!(!is_destructive("docker run --rm image", &[], &patterns));
        assert!(!is_destructive("rmdir_backup", &[], &patterns));
    }
}
