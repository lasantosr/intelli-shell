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
    model::{AsLabeledCommand, MaybeCommand},
    storage::{SqliteStorage, USER_CATEGORY},
    theme::{self, Theme},
    widgets::{LabelWidget, SaveCommandWidget, SearchWidget},
    ResultExt, Widget, WidgetOutput,
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

fn main() -> Result<()> {
    // Parse arguments
    let cli = Args::parse();

    // Set panic hook to avoid printing while on raw mode
    panic::set_hook(Box::new(|info| {
        PANIC_INFO.get_or_init(|| info.to_string());
    }));

    // Run program
    if panic::catch_unwind(|| run(cli)).is_err() {
        disable_raw_mode()?;
        if let Some(panic_info) = PANIC_INFO.get() {
            println!("{panic_info}");
        }
    }

    Ok(())
}

fn run(cli: Args) -> Result<()> {
    // Prepare storage
    let mut storage = SqliteStorage::new()?;

    // Execute command
    let res = match cli.action {
        Actions::Save { command, description } => exec(
            cli.inline,
            cli.inline_extra_line,
            SaveCommandWidget::new(&mut storage, command, description),
        )
        .map_output_str(),
        Actions::Search { filter } => exec(
            cli.inline,
            cli.inline_extra_line,
            SearchWidget::new(&mut storage, filter.unwrap_or_default())?,
        )
        .and_then(|out| {
            if let Some(cmd) = &out.output {
                if let Some(cmd) = match &cmd {
                    MaybeCommand::Persisted(cmd) => cmd.as_labeled_command(),
                    MaybeCommand::Unpersisted(cmd) => cmd.as_labeled_command(),
                } {
                    return exec(cli.inline, cli.inline_extra_line, LabelWidget::new(&mut storage, cmd)?)
                        .map_output_str();
                }
            }
            Ok(out).map_output_str()
        }),
        Actions::Label { command } => match command.as_labeled_command() {
            Some(labeled_command) => exec(
                cli.inline,
                cli.inline_extra_line,
                LabelWidget::new(&mut storage, labeled_command)?,
            )
            .map_output_str(),
            None => Ok(WidgetOutput::new("The command contains no labels!", command)),
        },
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
        )
        .map_output_str(),
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

fn exec<W>(inline: bool, inline_extra_line: bool, widget: W) -> Result<WidgetOutput<W::Output>>
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

fn exec_alt_screen<W>(mut widget: W, theme: Theme) -> Result<WidgetOutput<W::Output>>
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

fn exec_inline<W>(mut widget: W, theme: Theme, extra_line: bool) -> Result<WidgetOutput<W::Output>>
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
