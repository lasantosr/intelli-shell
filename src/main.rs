use std::{
    fs,
    io::{self, Write},
    panic,
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::Print,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    QueueableCommand,
};
use intelli_shell::{
    model::{AsLabeledCommand, Command},
    process::{EditCommandProcess, LabelProcess, SearchProcess},
    remove_newlines,
    storage::{SqliteStorage, USER_CATEGORY},
    theme, ExecutionContext, Process, ProcessOutput,
};
use once_cell::sync::OnceCell;
use tui::{backend::CrosstermBackend, layout::Rect, Terminal};

/// Command line arguments
#[derive(Parser)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Whether the UI should be rendered inline instead of taking full terminal
    #[arg(short, long)]
    inline: bool,

    /// Whether an extra line should be rendered when inline
    #[arg(long)]
    inline_extra_line: bool,

    /// Path of an existing file to write the output to (defaults to stdout)
    #[arg(short, long)]
    file_output: Option<String>,

    /// Action to be executed
    #[command(subcommand)]
    action: Actions,
}

#[derive(Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
enum Actions {
    /// Stores a new user command
    New {
        /// Command to be stored
        #[arg(short, long)]
        command: Option<String>,

        #[arg(short, long)]
        /// Description of the command
        description: Option<String>,
    },
    /// Opens a new search interface
    Search {
        /// Filter to be applied
        filter: Option<String>,
    },
    /// Opens a new label interface
    Label {
        /// Command to replace labels
        command: String,
    },
    /// Exports stored user commands
    Export {
        /// File path to be exported
        #[arg(short, long)]
        file: Option<String>,
    },
    /// Imports user commands
    Import {
        /// File path to be imported
        file: String,
    },
    #[cfg(feature = "tldr")]
    /// Fetches new commands from tldr
    Fetch {
        /// Category to fetch, skip to fetch for current platform (common, android, osx, linux, windows)
        category: Option<String>,
    },
}

static PANIC_INFO: OnceCell<String> = OnceCell::new();

fn main() {
    // Parse arguments
    let cli = Args::parse();

    // Set panic hook to avoid printing while on raw mode
    panic::set_hook(Box::new(|info| {
        PANIC_INFO.get_or_init(|| info.to_string());
    }));

    // Run program
    match panic::catch_unwind(|| run(cli)) {
        Ok(Ok(_)) => (),
        Ok(Err(err)) => eprintln!(" -> Error: {err}"),
        Err(_) => {
            disable_raw_mode().unwrap();
            if let Some(panic_info) = PANIC_INFO.get() {
                eprintln!("{panic_info}");
            }
        }
    }
}

fn run(cli: Args) -> Result<()> {
    // Prepare storage
    let storage = SqliteStorage::new()?;

    // Execution context
    let context = ExecutionContext {
        inline: cli.inline,
        theme: theme::DARK,
    };

    // Execute command
    let res = match cli.action {
        Actions::New { command, description } => {
            let cmd = command.map(remove_newlines);
            let description = description.map(remove_newlines);
            let command = Command::new(USER_CATEGORY, cmd.unwrap_or_default(), description.unwrap_or_default());
            exec(
                cli.inline,
                cli.inline_extra_line,
                EditCommandProcess::new(&storage, command, context)?,
            )
        }
        Actions::Search { filter } => exec(
            cli.inline,
            cli.inline_extra_line,
            SearchProcess::new(&storage, remove_newlines(filter.unwrap_or_default()), context)?,
        ),
        Actions::Label { command } => match remove_newlines(&command).as_labeled_command() {
            Some(labeled_command) => exec(
                cli.inline,
                cli.inline_extra_line,
                LabelProcess::new(&storage, labeled_command, context)?,
            ),
            None => Ok(ProcessOutput::new(" -> The command contains no labels!", command)),
        },
        Actions::Export { file } => {
            let file_path = file.as_deref().unwrap_or("user_commands.txt");
            let exported = storage.export(USER_CATEGORY, file_path)?;
            Ok(ProcessOutput::message(format!(
                " -> Successfully exported {exported} commands to '{file_path}'"
            )))
        }
        Actions::Import { file } => {
            let new = storage.import(USER_CATEGORY, file)?;
            Ok(ProcessOutput::message(format!(" -> Imported {new} new commands")))
        }
        #[cfg(feature = "tldr")]
        Actions::Fetch { category } => exec(
            cli.inline,
            cli.inline_extra_line,
            intelli_shell::process::FetchProcess::new(category, &storage),
        ),
    }?;

    // Print any message received
    if let Some(msg) = res.message {
        eprintln!("{msg}");
    }

    // Write out the result
    match res.output {
        None => (),
        Some(output) => match cli.file_output {
            None => execute!(io::stdout(), Print(format!("{output}\n")))?,
            Some(path) => fs::write(path, output)?,
        },
    }

    // Exit
    Ok(())
}

fn exec<P>(inline: bool, inline_extra_line: bool, process: P) -> Result<ProcessOutput>
where
    P: Process,
{
    if inline {
        exec_inline(process, inline_extra_line)
    } else {
        exec_alt_screen(process)
    }
}

fn exec_alt_screen<P>(mut process: P) -> Result<ProcessOutput>
where
    P: Process,
{
    // Check if we've got a straight result
    if let Some(result) = process.peek()? {
        return Ok(result);
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Prepare terminal
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Show process
    let res = process.show(&mut terminal, |f| f.size());

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    // Return
    res
}

fn exec_inline<P>(mut process: P, extra_line: bool) -> Result<ProcessOutput>
where
    P: Process,
{
    // Check if we've got a straight result
    if let Some(result) = process.peek()? {
        return Ok(result);
    }

    // Setup terminal
    let (orig_cursor_x, orig_cursor_y) = cursor::position()?;
    let min_height = process.min_height() as u16;
    let mut stdout = io::stdout();
    for _ in 0..min_height {
        stdout.queue(Print("\n"))?;
    }
    if extra_line {
        stdout.queue(Print("\n"))?;
    }
    stdout
        .queue(cursor::MoveToPreviousLine(min_height))?
        .queue(Clear(ClearType::FromCursorDown))?
        .flush()?;

    let (cursor_x, cursor_y) = cursor::position()?;

    enable_raw_mode()?;

    // Prepare terminal
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Show process
    let res = process.show(&mut terminal, |f| {
        let Rect {
            x: _,
            y: _,
            width,
            height,
        } = f.size();
        let min_height = std::cmp::min(height, min_height);
        let available_height = height - cursor_y;
        let height = std::cmp::max(min_height, available_height);
        let width = width - cursor_x;
        Rect::new(cursor_x, cursor_y, width, height)
    });

    // Restore terminal
    disable_raw_mode()?;
    terminal
        .backend_mut()
        .queue(cursor::MoveTo(
            orig_cursor_x,
            std::cmp::min(orig_cursor_y, cursor_y - extra_line as u16),
        ))?
        .queue(Clear(ClearType::FromCursorDown))?
        .flush()?;
    terminal.show_cursor()?;

    // Return
    res
}
