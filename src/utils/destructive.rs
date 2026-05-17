const DESTRUCTIVE_COMMANDS: &[&str] = &["rm", "rmdir", "del", "erase", "rd", "remove-item"];
const PRIVILEGE_WRAPPERS: &[&str] = &["sudo", "doas"];

/// Checks whether a command string contains a destructive shell action.
pub fn is_destructive_command(command: &str) -> bool {
    split_shell_segments(command).into_iter().any(is_destructive_segment)
}

fn is_destructive_segment(segment: &str) -> bool {
    let mut words = ShellWordIter::new(segment);

    for word in words.by_ref() {
        if is_env_assignment(word) || is_privilege_wrapper(word) {
            continue;
        }

        return is_destructive_verb(word) || is_destructive_subcommand(word, &mut words);
    }

    false
}

fn is_destructive_verb(word: &str) -> bool {
    DESTRUCTIVE_COMMANDS.iter().any(|verb| word.eq_ignore_ascii_case(verb))
}

fn is_privilege_wrapper(word: &str) -> bool {
    PRIVILEGE_WRAPPERS.iter().any(|wrapper| word.eq_ignore_ascii_case(wrapper))
}

fn is_destructive_subcommand(command: &str, remaining_words: &mut ShellWordIter<'_>) -> bool {
    if !command.eq_ignore_ascii_case("git") {
        return false;
    }

    remaining_words
        .next()
        .is_some_and(is_destructive_verb)
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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

struct ShellWordIter<'a> {
    segment: &'a str,
    cursor: usize,
}

impl<'a> ShellWordIter<'a> {
    fn new(segment: &'a str) -> Self {
        Self { segment, cursor: 0 }
    }
}

impl<'a> Iterator for ShellWordIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.segment.as_bytes();

        while let Some(byte) = bytes.get(self.cursor) {
            if byte.is_ascii_whitespace() {
                self.cursor += 1;
            } else {
                break;
            }
        }

        if self.cursor >= bytes.len() {
            return None;
        }

        let start = self.cursor;
        let mut index = self.cursor;
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
                _ if byte.is_ascii_whitespace() => break,
                _ => index += 1,
            }
        }

        self.cursor = index;
        Some(&self.segment[start..index])
    }
}

#[cfg(test)]
mod tests {
    use super::is_destructive_command;

    #[test]
    fn test_is_destructive_command_positive_cases() {
        for command in [
            "rm file",
            "sudo rm -rf /tmp/x",
            "VAR=1 rm file",
            "echo ok && rm file",
            "git rm file",
            "Remove-Item foo",
            "del foo",
        ] {
            assert!(is_destructive_command(command), "expected destructive: {command}");
        }
    }

    #[test]
    fn test_is_destructive_command_negative_cases() {
        for command in [
            "docker run --rm image",
            "echo rm file",
            "printf 'rm file'",
            "git status",
            "rmdir_backup",
            "trash-put foo",
        ] {
            assert!(
                !is_destructive_command(command),
                "expected non-destructive: {command}"
            );
        }
    }

    #[test]
    fn test_command_is_destructive_uses_command_text() {
        let command = "doas erase temp.txt";
        assert!(is_destructive_command(command));
    }
}
