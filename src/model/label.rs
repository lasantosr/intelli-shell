use std::fmt::{Display, Formatter};

use once_cell::sync::Lazy;
use regex::Regex;

use super::Command;
use crate::common::{flatten_str, SplitCaptures, SplitItem};

/// Type to represent label suggestions.
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct LabelSuggestion {
    pub flat_root_cmd: String,
    pub flat_label: String,
    pub suggestion: String,
    pub usage: u64,
}

impl LabelSuggestion {
    pub fn increment_usage(&mut self) {
        self.usage += 1;
    }
}

/// A [Command] containing labels
#[cfg_attr(debug_assertions, derive(Debug))]
#[derive(Clone)]
pub struct LabeledCommand {
    pub root: String,
    pub parts: Vec<CommandPart>,
}

impl LabeledCommand {
    pub fn next_label(&self) -> Option<(usize, &str)> {
        let mut ix = 0;
        for part in self.parts.iter() {
            match part {
                CommandPart::Text(t) => ix += t.len(),
                CommandPart::LabelValue(v) => ix += v.len(),
                CommandPart::Label(l) => return Some((ix, l)),
            }
        }
        None
    }

    pub fn set_next_label(&mut self, value: impl Into<String>) {
        for part in self.parts.iter_mut() {
            if let CommandPart::Label(_) = part {
                *part = CommandPart::LabelValue(value.into());
                break;
            }
        }
    }

    pub fn new_suggestion_for(&self, label: impl AsRef<str>, suggestion: impl Into<String>) -> LabelSuggestion {
        LabelSuggestion {
            flat_root_cmd: flatten_str(&self.root),
            flat_label: flatten_str(label),
            suggestion: suggestion.into(),
            usage: 1,
        }
    }
}

impl Display for LabeledCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for part in self.parts.iter() {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
#[derive(Clone)]
pub enum CommandPart {
    Text(String),
    Label(String),
    LabelValue(String),
}

impl Display for CommandPart {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandPart::Text(t) => write!(f, "{t}"),
            CommandPart::Label(l) => write!(f, "{{{{{}}}}}", l),
            CommandPart::LabelValue(v) => write!(f, "{v}"),
        }
    }
}

/// Regex to parse commands with labels
static COMMAND_LABEL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{\{([^}]+)}}"#).unwrap());

/// Trait to build a [LabeledCommand] from other types
pub trait AsLabeledCommand {
    /// Represents this type as a labeled command, when labels exist. Otherwise [None] shall be returned.
    fn as_labeled_command(&self) -> Option<LabeledCommand>;
}
impl AsLabeledCommand for str {
    fn as_labeled_command(&self) -> Option<LabeledCommand> {
        let cmd = self;
        let root = cmd.split_whitespace().next().unwrap_or(cmd);
        let splitter = SplitCaptures::new(&COMMAND_LABEL_REGEX, cmd);
        let parts = splitter
            .map(|e| match e {
                SplitItem::Unmatched(t) => CommandPart::Text(t.to_owned()),
                SplitItem::Captured(l) => CommandPart::Label(l.get(1).unwrap().as_str().to_owned()),
            })
            .collect::<Vec<_>>();

        if parts.len() <= 1 {
            None
        } else {
            Some(LabeledCommand {
                root: root.to_owned(),
                parts,
            })
        }
    }
}
impl AsLabeledCommand for Command {
    fn as_labeled_command(&self) -> Option<LabeledCommand> {
        self.cmd.as_labeled_command()
    }
}
