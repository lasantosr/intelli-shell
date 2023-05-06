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
pub struct LabeledCommand<'c> {
    pub id: Option<i64>,
    pub root: &'c str,
    pub parts: Vec<CommandPart<'c>>,
}

impl<'c> LabeledCommand<'c> {
    pub fn next_label(&'c self) -> Option<(usize, &'c str)> {
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

    pub fn new_suggestion_for(&'c self, label: &'c str, suggestion: impl Into<String>) -> LabelSuggestion {
        LabelSuggestion {
            flat_root_cmd: flatten_str(self.root),
            flat_label: flatten_str(label),
            suggestion: suggestion.into(),
            usage: 1,
        }
    }
}

impl<'c> Display for LabeledCommand<'c> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for part in self.parts.iter() {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
#[derive(Clone)]
pub enum CommandPart<'c> {
    Text(&'c str),
    Label(&'c str),
    LabelValue(String),
}

impl<'c> Display for CommandPart<'c> {
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
pub trait AsLabeledCommand<'c> {
    /// Represents this type as a labeled command, when labels exist. Otherwise [None] shall be returned.
    fn as_labeled_command(&'c self) -> Option<LabeledCommand<'c>>;
}
impl<'c> AsLabeledCommand<'c> for str {
    fn as_labeled_command(&'c self) -> Option<LabeledCommand<'c>> {
        let cmd = self;
        let root = cmd.split_whitespace().next().unwrap_or(cmd);
        let splitter = SplitCaptures::new(&COMMAND_LABEL_REGEX, cmd);
        let parts = splitter
            .map(|e| match e {
                SplitItem::Unmatched(t) => CommandPart::Text(t),
                SplitItem::Captured(l) => CommandPart::Label(l.get(1).unwrap().as_str()),
            })
            .collect::<Vec<_>>();

        if parts.len() <= 1 {
            None
        } else {
            Some(LabeledCommand { id: None, root, parts })
        }
    }
}
impl<'c> AsLabeledCommand<'c> for Command {
    fn as_labeled_command(&'c self) -> Option<LabeledCommand<'c>> {
        self.cmd.as_labeled_command().map(|mut l| {
            l.id = Some(self.id);
            l
        })
    }
}
