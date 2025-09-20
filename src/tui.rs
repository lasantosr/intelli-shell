use std::{
    cmp,
    io::{self, Stdout, stdout},
    ops::{Deref, DerefMut},
    thread,
    time::Duration,
};

use color_eyre::Result;
use crossterm::{
    cursor,
    event::{
        self, Event as CrosstermEvent, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
        KeyboardEnhancementFlags, MouseEvent,
    },
    style,
    terminal::{self, ClearType, supports_keyboard_enhancement},
};
use futures_util::{FutureExt, StreamExt};
use ratatui::{CompletedFrame, Frame, Terminal, backend::CrosstermBackend as Backend, layout::Rect};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
    time::interval,
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

/// Events that can occur within the TUI application
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// A periodic tick event, useful for time-based updates or animations
    Tick,
    /// A periodic render event, suggesting the UI should be redrawn
    Render,
    /// The terminal window gained focus
    FocusGained,
    /// The terminal window lost focus
    FocusLost,
    /// Text was pasted into the terminal (requires paste mode)
    Paste(String),
    /// A key was pressed
    Key(KeyEvent),
    /// A mouse event occurred (requires mouse capture)
    Mouse(MouseEvent),
    /// The terminal window was resized (columns and rows)
    Resize(u16, u16),
}

/// Manages the terminal User Interface (TUI) lifecycle, event handling, and rendering
pub struct Tui {
    stdout: Stdout,
    terminal: Terminal<Backend<Stdout>>,
    task: JoinHandle<()>,
    loop_cancellation_token: CancellationToken,
    global_cancellation_token: CancellationToken,
    event_rx: UnboundedReceiver<Event>,
    event_tx: UnboundedSender<Event>,
    frame_rate: f64,
    tick_rate: f64,
    mouse: bool,
    paste: bool,
    state: Option<State>,
}

#[derive(Clone, Copy)]
enum State {
    FullScreen(bool),
    Inline(bool, InlineTuiContext),
}

#[derive(Clone, Copy)]
struct InlineTuiContext {
    min_height: u16,
    x: u16,
    y: u16,
    restore_cursor_x: u16,
    restore_cursor_y: u16,
}

#[allow(dead_code, reason = "provide a useful interface, even if not required yet")]
impl Tui {
    /// Constructs a new terminal ui with default settings
    pub fn new(cancellation_token: CancellationToken) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            stdout: stdout(),
            terminal: Terminal::new(Backend::new(stdout()))?,
            task: tokio::spawn(async {}),
            loop_cancellation_token: CancellationToken::new(),
            global_cancellation_token: cancellation_token,
            event_rx,
            event_tx,
            frame_rate: 60.0,
            tick_rate: 10.0,
            mouse: false,
            paste: false,
            state: None,
        })
    }

    /// Sets the tick rate for the TUI.
    ///
    /// The tick rate determines how frequently `Event::Tick` is generated.
    pub fn tick_rate(mut self, tick_rate: f64) -> Self {
        self.state.is_some().then(|| panic!("Can't updated an entered TUI"));
        self.tick_rate = tick_rate;
        self
    }

    /// Sets the frame rate for the TUI.
    ///
    /// The frame rate determines how often `Event::Render` is emitted.
    pub fn frame_rate(mut self, frame_rate: f64) -> Self {
        self.state.is_some().then(|| panic!("Can't updated an entered TUI"));
        self.frame_rate = frame_rate;
        self
    }

    /// Enables or disables mouse event capture.
    ///
    /// If true, `Event::Mouse` events will be emitted.
    pub fn mouse(mut self, mouse: bool) -> Self {
        self.state.is_some().then(|| panic!("Can't updated an entered TUI"));
        self.mouse = mouse;
        self
    }

    /// Enables or disables bracketed paste mode.
    ///
    /// If true, `Event::Paste` events will be emitted.
    pub fn paste(mut self, paste: bool) -> Self {
        self.state.is_some().then(|| panic!("Can't updated an entered TUI"));
        self.paste = paste;
        self
    }

    /// Asynchronously retrieves the next event from the event queue.
    ///
    /// Returns `None` if the event channel has been closed (e.g., the event loop has stopped).
    pub async fn next_event(&mut self) -> Option<Event> {
        self.event_rx.recv().await
    }

    /// Prepares the terminal for full-screen TUI interaction and starts the event loop
    pub fn enter(&mut self) -> Result<()> {
        self.state.is_some().then(|| panic!("Can't re-enter on a TUI"));

        tracing::trace!(mouse = self.mouse, paste = self.paste, "Entering a full-screen TUI");

        // Enter raw mode and set up the terminal
        let keyboard_enhancement_supported = self.enter_raw_mode(true)?;

        // Store the state and start the event loop
        self.state = Some(State::FullScreen(keyboard_enhancement_supported));
        self.start();

        Ok(())
    }

    /// Prepares the terminal for inline TUI interaction and starts the event loop
    pub fn enter_inline(&mut self, extra_line: bool, min_height: u16) -> Result<()> {
        self.state.is_some().then(|| panic!("Can't re-enter on a TUI"));
        let extra_line = extra_line as u16;

        tracing::trace!(
            mouse = self.mouse,
            paste = self.paste,
            extra_line,
            min_height,
            "Entering an inline TUI"
        );

        // Save the original cursor position
        let (orig_cursor_x, orig_cursor_y) = cursor::position()?;
        tracing::trace!("Initial cursor position: ({orig_cursor_x},{orig_cursor_y})");
        // Prepare the area for the inline content
        crossterm::execute!(
            self.stdout,
            // Fill in the minimum height (plus the extra line), the cursor will end up at the end
            style::Print("\n".repeat((min_height + extra_line) as usize)),
            // Move the cursor back the min height (without the extra lines)
            cursor::MoveToPreviousLine(min_height),
            // And clear the lines below
            terminal::Clear(ClearType::FromCursorDown)
        )?;
        // Retrieve the new cursor position, which defines the starting coords for the area to render in
        let (cursor_x, cursor_y) = cursor::position()?;
        // Calculate where the cursor should be restored to
        let restore_cursor_x = orig_cursor_x;
        let restore_cursor_y = cmp::min(orig_cursor_y, cmp::max(cursor_y, extra_line) - extra_line);
        tracing::trace!("Cursor shall be restored at: ({restore_cursor_x},{restore_cursor_y})");

        // Enter raw mode and set up the terminal
        let keyboard_enhancement_supported = self.enter_raw_mode(false)?;

        // Store the state and start the event loop
        self.state = Some(State::Inline(
            keyboard_enhancement_supported,
            InlineTuiContext {
                min_height,
                x: cursor_x,
                y: cursor_y,
                restore_cursor_x,
                restore_cursor_y,
            },
        ));
        self.start();

        Ok(())
    }

    /// Renders the TUI using the provided callback function.
    ///
    /// The callback receives a mutable reference to the `Frame` and the area to render in, which might not be the same
    /// as the frame area for inline TUIs.
    pub fn render<F>(&mut self, render_callback: F) -> io::Result<CompletedFrame<'_>>
    where
        F: FnOnce(&mut Frame, Rect),
    {
        let Some(state) = self.state else {
            return Err(io::Error::other("Cannot render on a non-entered TUI"));
        };

        self.terminal.draw(|frame| {
            let area = match state {
                State::FullScreen(_) => frame.area(),
                State::Inline(_, inline) => {
                    let frame = frame.area();
                    let min_height = cmp::min(frame.height, inline.min_height);
                    let available_height = frame.height - inline.y;
                    let height = cmp::max(min_height, available_height);
                    let width = frame.width - inline.x;
                    Rect::new(inline.x, inline.y, width, height)
                }
            };

            render_callback(frame, area);
        })
    }

    /// Restores the terminal to its original state and stops the event loop
    pub fn exit(mut self) -> Result<()> {
        self.state.is_none().then(|| panic!("Cannot exit a non-entered TUI"));
        self.stop();
        self.restore_terminal()
    }

    fn restore_terminal(&mut self) -> Result<()> {
        match self.state.take() {
            None => (),
            Some(State::FullScreen(keyboard_enhancement_supported)) => {
                tracing::trace!("Leaving the full-screen TUI");
                self.flush()?;
                self.exit_raw_mode(true, keyboard_enhancement_supported)?;
            }
            Some(State::Inline(keyboard_enhancement_supported, ctx)) => {
                tracing::trace!("Leaving the inline TUI");
                self.flush()?;
                self.exit_raw_mode(false, keyboard_enhancement_supported)?;
                crossterm::execute!(
                    self.stdout,
                    cursor::MoveTo(ctx.restore_cursor_x, ctx.restore_cursor_y),
                    terminal::Clear(ClearType::FromCursorDown)
                )?;
            }
        }

        Ok(())
    }

    fn enter_raw_mode(&mut self, alt_screen: bool) -> Result<bool> {
        terminal::enable_raw_mode()?;
        crossterm::execute!(self.stdout, cursor::Hide)?;
        if alt_screen {
            crossterm::execute!(self.stdout, terminal::EnterAlternateScreen)?;
        }
        if self.mouse {
            crossterm::execute!(self.stdout, event::EnableMouseCapture)?;
        }
        if self.paste {
            crossterm::execute!(self.stdout, event::EnableBracketedPaste)?;
        }

        tracing::trace!("Checking keyboard enhancement support");
        let keyboard_enhancement_supported = supports_keyboard_enhancement()
            .inspect_err(|err| tracing::error!("{err}"))
            .unwrap_or(false);

        if keyboard_enhancement_supported {
            tracing::trace!("Keyboard enhancement flags enabled");
            crossterm::execute!(
                self.stdout,
                event::PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                ),
            )?;
        } else {
            tracing::trace!("Keyboard enhancement flags not enabled");
        }

        Ok(keyboard_enhancement_supported)
    }

    fn exit_raw_mode(&mut self, alt_screen: bool, keyboard_enhancement_supported: bool) -> Result<()> {
        if keyboard_enhancement_supported {
            crossterm::execute!(self.stdout, event::PopKeyboardEnhancementFlags)?;
        }

        if self.paste {
            crossterm::execute!(self.stdout, event::DisableBracketedPaste)?;
        }
        if self.mouse {
            crossterm::execute!(self.stdout, event::DisableMouseCapture)?;
        }
        if alt_screen {
            crossterm::execute!(self.stdout, terminal::LeaveAlternateScreen)?;
        }
        crossterm::execute!(self.stdout, cursor::Show)?;
        terminal::disable_raw_mode()?;

        Ok(())
    }

    fn start(&mut self) {
        self.cancel();
        self.loop_cancellation_token = CancellationToken::new();

        tracing::trace!(
            tick_rate = self.tick_rate,
            frame_rate = self.frame_rate,
            "Starting the event loop"
        );

        self.task = tokio::spawn(Self::event_loop(
            self.event_tx.clone(),
            self.loop_cancellation_token.clone(),
            self.global_cancellation_token.clone(),
            self.tick_rate,
            self.frame_rate,
        ));
    }

    #[instrument(skip_all)]
    async fn event_loop(
        event_tx: UnboundedSender<Event>,
        loop_cancellation_token: CancellationToken,
        global_cancellation_token: CancellationToken,
        tick_rate: f64,
        frame_rate: f64,
    ) {
        let mut event_stream = EventStream::new();
        let mut tick_interval = interval(Duration::from_secs_f64(1.0 / tick_rate));
        let mut render_interval = interval(Duration::from_secs_f64(1.0 / frame_rate));

        loop {
            let event = tokio::select! {
                // Ensure signals are checked in order (cancellation first)
                biased;

                // Exit the loop if any cancellation is requested
                _ = loop_cancellation_token.cancelled() => {
                    break;
                }
                _ = global_cancellation_token.cancelled() => {
                    break;
                }

                // Crossterm events
                crossterm_event = event_stream.next().fuse() => match crossterm_event {
                    Some(Ok(event)) => match event {
                        // On raw mode, SIGINT is no longer received and we should handle it manually
                        CrosstermEvent::Key(KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        }) => {
                            tracing::debug!("Ctrl+C key event received in TUI, cancelling token");
                            global_cancellation_token.cancel();
                            continue;
                        }
                        // Process only key press events to avoid duplicate events for release/repeat
                        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => Event::Key(key),
                        CrosstermEvent::Mouse(mouse) => Event::Mouse(mouse),
                        CrosstermEvent::Resize(cols, rows) => Event::Resize(cols, rows),
                        CrosstermEvent::FocusLost => Event::FocusLost,
                        CrosstermEvent::FocusGained => Event::FocusGained,
                        CrosstermEvent::Paste(s) => Event::Paste(s),
                        _ => continue, // Ignore other crossterm event types
                    }
                    Some(Err(err)) =>  {
                        tracing::error!("Error retrieving next crossterm event: {err}");
                        break;
                    },
                    None => break, // Event stream ended, exit the loop
                },

                // Intervals
                _ = tick_interval.tick() => Event::Tick,
                _ = render_interval.tick() => Event::Render,
            };

            // Try to send the processed event
            if event_tx.send(event).is_err() {
                // If sending fails, the receiver is likely dropped. Exit the loop
                break;
            }
        }

        // Ensure the token is cancelled if the loop exits for reasons other than direct cancellation
        // (e.g. event_stream ending or send error).
        loop_cancellation_token.cancel();
    }

    fn stop(&self) {
        if !self.task.is_finished() {
            tracing::trace!("Stopping the event loop");
            self.cancel();
            let mut counter = 0;
            while !self.task.is_finished() {
                thread::sleep(Duration::from_millis(1));
                counter += 1;
                // Attempt to abort the task if it hasn't finished in a short period
                if counter > 50 {
                    tracing::debug!("Task hasn't finished in 50 milliseconds, attempting to abort");
                    self.task.abort();
                }
                // Log an error and give up waiting if the task hasn't finished after the abort
                if counter > 100 {
                    tracing::error!("Failed to abort task in 100 milliseconds for unknown reason");
                    break;
                }
            }
        }
    }

    fn cancel(&self) {
        self.loop_cancellation_token.cancel();
    }
}

impl Deref for Tui {
    type Target = Terminal<Backend<Stdout>>;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl DerefMut for Tui {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        self.stop();
        if let Err(err) = self.restore_terminal() {
            tracing::error!("Failed to restore terminal state: {err:?}");
        }
    }
}
