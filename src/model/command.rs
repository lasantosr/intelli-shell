use std::fmt::Display;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct Command {
    pub id: i64,
    pub category: String,
    pub alias: Option<String>,
    pub cmd: String,
    pub description: String,
    pub usage: u64,
}

impl Command {
    pub fn new(category: impl Into<String>, command: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: 0,
            category: category.into(),
            alias: None,
            cmd: command.into(),
            description: description.into(),
            usage: 0,
        }
    }

    pub fn increment_usage(&mut self) {
        self.usage += 1;
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cmd)
    }
}

pub enum MaybeCommand {
    Persisted(Command),
    Unpersisted(String),
}
impl Display for MaybeCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaybeCommand::Persisted(p) => write!(f, "{p}"),
            MaybeCommand::Unpersisted(u) => write!(f, "{u}"),
        }
    }
}
impl From<Command> for MaybeCommand {
    fn from(value: Command) -> Self {
        Self::Persisted(value)
    }
}
impl<T: Into<String>> From<T> for MaybeCommand {
    fn from(value: T) -> Self {
        Self::Unpersisted(value.into())
    }
}
