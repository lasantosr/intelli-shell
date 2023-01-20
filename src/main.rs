use std::{
    fs,
    io::{self, Write},
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
    storage::{SqliteStorage, USER_CATEGORY},
    theme::{self, Theme},
    widgets::{SaveCommandWidget, SearchWidget},
    Widget, WidgetOutput,
};
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
    /// Saves a new user command
    Save {
        /// Command to be stored
        command: String,

        #[arg(short, long)]
        /// Description of the command
        description: Option<String>,
    },
    /// Opens a new search interface
    Search {
        /// Filter to be applied
        filter: Option<String>,
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

fn main() -> Result<()> {
    // Parse arguments
    let cli = Args::parse();

    // Prepare storage
    let mut storage = SqliteStorage::new()?;

    // Execute command
    let res: WidgetOutput = match cli.action {
        Actions::Save { command, description } => exec(
            cli.inline,
            cli.inline_extra_line,
            SaveCommandWidget::new(&mut storage, command, description),
        ),
        Actions::Search { filter } => exec(
            cli.inline,
            cli.inline_extra_line,
            SearchWidget::new(&mut storage, filter.unwrap_or_default())?,
        ),
        Actions::Export { file } => {
            let file_path = file.as_deref().unwrap_or("user_commands.txt");
            let exported = storage.export(USER_CATEGORY, file_path)?;
            Ok(WidgetOutput::message(format!(
                "Successfully exported {exported} commands to '{file_path}'"
            )))
        }
        Actions::Import { file } => {
            let new = storage.import(USER_CATEGORY, file)?;
            Ok(WidgetOutput::message(format!("Imported {new} new commands")))
        }
        #[cfg(feature = "tldr")]
        Actions::Fetch { category } => exec(
            cli.inline,
            cli.inline_extra_line,
            intelli_shell::widgets::FetchWidget::new(category, &mut storage),
        ),
    }?;

    // Print any message received
    if let Some(msg) = res.message {
        println!("{msg}");
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

fn exec<W>(inline: bool, inline_extra_line: bool, widget: W) -> Result<WidgetOutput>
where
    W: Widget,
{
    let theme = theme::DARK;
    if inline {
        exec_inline(widget, theme, inline_extra_line)
    } else {
        exec_alt_screen(widget, theme)
    }
}

fn exec_alt_screen<W>(mut widget: W, theme: Theme) -> Result<WidgetOutput>
where
    W: Widget,
{
    // Check if we've got a straight result
    if let Some(result) = widget.peek()? {
        return Ok(result);
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Prepare terminal
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Show widget
    let res = widget.show(&mut terminal, false, theme, |f| f.size());

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    // Return
    res
}

fn exec_inline<W>(mut widget: W, theme: Theme, extra_line: bool) -> Result<WidgetOutput>
where
    W: Widget,
{
    // Check if we've got a straight result
    if let Some(result) = widget.peek()? {
        return Ok(result);
    }

    // Setup terminal
    let (orig_cursor_x, orig_cursor_y) = cursor::position()?;
    let min_height = widget.min_height() as u16;
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

    // Show widget
    let res = widget.show(&mut terminal, true, theme, |f| {
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
