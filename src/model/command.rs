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

    pub fn is_persisted(&self) -> bool {
        self.id > 0
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cmd)
    }
}
