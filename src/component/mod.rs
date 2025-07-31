use async_trait::async_trait;
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::{Frame, layout::Rect};

use crate::{
    app::Action,
    config::{KeyBindingAction, KeyBindingsConfig},
    process::ProcessOutput,
};

pub mod edit;
pub mod search;
pub mod variable;

/// Defines the behavior for a UI component within the application.
///
/// Components are responsible for rendering themselves, handling user input, and managing their internal state. They
/// can also perform logic updates periodically via the `tick` method.
#[async_trait]
pub trait Component: Send {
    /// Retrieves the component name, for debugging purposes
    fn name(&self) -> &'static str;

    /// Calculates the minimum height required by this component to be rendered correctly when inline (in rows)
    fn min_inline_height(&self) -> u16;

    /// Allows the component to initialize any internal state or resources it needs before being used.
    ///
    /// It can be called multiple times, for example if a component is re-used after being switched out.
    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Peeks into the component before rendering, for examplle to give a straight result, switch component or continue
    /// with the TUI
    async fn peek(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Processes time-based logic, internal state updates, or background tasks for the component.
    ///
    /// This method is called periodically by the application's main loop and is not directly tied to rendering or user
    /// input events.
    /// It can be used for animations, polling asynchronous operations, or updating internal timers.
    fn tick(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Renders the component's UI within the given `area` of the `frame`
    fn render(&mut self, frame: &mut Frame, area: Rect);

    /// Finalizes the component's current operation and returns its output with the current state.
    ///
    /// This method is typically called when the user signals that they want to exit the command.
    fn exit(&mut self) -> Result<Option<ProcessOutput>> {
        Ok(Some(ProcessOutput::success()))
    }

    /// Processes a paste event, typically from clipboard paste into the terminal.
    ///
    /// This method is called when the application detects a paste action. The `content` parameter contains the string
    /// of text that was pasted.
    ///
    /// The default implementation will just call [`insert_text`](Component::insert_text).
    fn process_paste_event(&mut self, content: String) -> Result<Action> {
        self.insert_text(content)
    }

    /// Processes a key press event.
    ///
    /// This method is the primary handler for keyboard input. It receives a `KeyEvent` and is responsible for
    /// translating it into component-specific behaviors or application-level [`Action`]s by consulting the provided
    /// `keybindings` or using hardcoded defaults.
    ///
    /// Implementors can override this method to provide entirely custom key handling, optionally calling
    /// [`default_process_key_event`](Component::default_process_key_event) first and checking its result.
    async fn process_key_event(&mut self, keybindings: &KeyBindingsConfig, key: KeyEvent) -> Result<Action> {
        Ok(self
            .default_process_key_event(keybindings, key)
            .await?
            .unwrap_or_default())
    }

    /// The default behavior for [`process_key_event`](Component::process_key_event) with a baseline set of key mappings
    /// by matching against the event. It calls other granular methods of this trait (e.g.,
    /// [`move_up`](Component::move_up), [`insert_char`](Component::insert_char)).
    ///
    /// - If this default method returns `Ok(Some(action))`, it means the key was recognized and mapped to an `Action`.
    /// - If it returns `Ok(None)`, it means the specific key event was **not handled** by this default logic, allowing
    ///   an overriding implementation to then process it.
    async fn default_process_key_event(
        &mut self,
        keybindings: &KeyBindingsConfig,
        key: KeyEvent,
    ) -> Result<Option<Action>> {
        // Check customizable key bindings first
        if let Some(action) = keybindings.get_action_matching(&key) {
            return Ok(Some(match action {
                KeyBindingAction::Quit => self.exit()?.map(Action::Quit).unwrap_or_default(),
                KeyBindingAction::Update => self.selection_update().await?,
                KeyBindingAction::Delete => self.selection_delete().await?,
                KeyBindingAction::Confirm => self.selection_confirm().await?,
                KeyBindingAction::Execute => self.selection_execute().await?,
                KeyBindingAction::SearchMode => self.toggle_search_mode()?,
                KeyBindingAction::SearchUserOnly => self.toggle_search_user_only()?,
            }));
        }

        // If no configured binding matched, fall back to default handling
        Ok(match key.code {
            #[cfg(debug_assertions)]
            // Debug
            KeyCode::Char('p') if key.modifiers == KeyModifiers::ALT => panic!("Debug panic!"),
            // Selection / Movement
            KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_prev()?),
            KeyCode::Char('j') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_next()?),
            KeyCode::Home => Some(self.move_home(key.modifiers == KeyModifiers::CONTROL)?),
            KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_home(false)?),
            KeyCode::End => Some(self.move_end(key.modifiers == KeyModifiers::CONTROL)?),
            KeyCode::Char('e') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_end(false)?),
            KeyCode::Up => Some(self.move_up()?),
            KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_up()?),
            KeyCode::Down => Some(self.move_down()?),
            KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_down()?),
            KeyCode::Right => Some(self.move_right(key.modifiers == KeyModifiers::CONTROL)?),
            KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_right(false)?),
            KeyCode::Char('f') if key.modifiers == KeyModifiers::ALT => Some(self.move_right(true)?),
            KeyCode::Left => Some(self.move_left(key.modifiers == KeyModifiers::CONTROL)?),
            KeyCode::Char('b') if key.modifiers == KeyModifiers::CONTROL => Some(self.move_left(false)?),
            KeyCode::Char('b') if key.modifiers == KeyModifiers::ALT => Some(self.move_left(true)?),
            // Undo / redo
            KeyCode::Char('z') if key.modifiers == KeyModifiers::CONTROL => Some(self.undo()?),
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => Some(self.undo()?),
            KeyCode::Char('y') if key.modifiers == KeyModifiers::CONTROL => Some(self.redo()?),
            KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => Some(self.redo()?),
            // Text edit
            KeyCode::Backspace => Some(self.delete(true, key.modifiers == KeyModifiers::CONTROL)?),
            KeyCode::Char('h') if key.modifiers == KeyModifiers::CONTROL => Some(self.delete(true, false)?),
            KeyCode::Char('w') if key.modifiers == KeyModifiers::CONTROL => Some(self.delete(true, true)?),
            KeyCode::Delete => Some(self.delete(false, key.modifiers == KeyModifiers::CONTROL)?),
            KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => Some(self.delete(false, false)?),
            KeyCode::Char('d') if key.modifiers == KeyModifiers::ALT => Some(self.delete(false, true)?),
            KeyCode::Enter if key.modifiers == KeyModifiers::SHIFT => Some(self.insert_newline()?),
            KeyCode::Enter if key.modifiers == KeyModifiers::ALT => Some(self.insert_newline()?),
            KeyCode::Char('m') if key.modifiers == KeyModifiers::CONTROL => Some(self.insert_newline()?),
            KeyCode::Char(c) => Some(self.insert_char(c)?),
            // Don't process other events
            _ => None,
        })
    }

    /// Processes a mouse event.
    ///
    /// This method is called when a mouse action (like click, scroll, move) occurs within the terminal, provided mouse
    /// capture is enabled.
    ///
    /// Returns [Some] if the event was processed or [None] if not (the default).
    fn process_mouse_event(&mut self, mouse: MouseEvent) -> Result<Action> {
        let _ = mouse;
        Ok(Action::NoOp)
    }

    /// Called when the component gains focus within the application.
    ///
    /// Components can implement this method to change their appearance (e.g., show a highlight border), enable input
    /// handling, initialize internal state specific to being active, or perform other setup tasks.
    fn focus_gained(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Called when the component loses focus within the application.
    ///
    /// Components can implement this method to change their appearance (e.g., remove a highlight border), disable input
    /// handling, persist any temporary state, or perform other cleanup tasks.
    fn focus_lost(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a terminal resize event, informing the component of the new global terminal dimensions.
    ///
    /// This method is called when the overall terminal window size changes.
    /// Components can use this notification to adapt internal state, pre-calculate layout-dependent values, or
    /// invalidate caches before a subsequent `render` call, which will likely provide a new drawing area (`Rect`)
    /// based on these new terminal dimensions.
    ///
    /// **Note:** The `width` and `height` parameters typically represent the total new dimensions of the terminal in
    /// columns and rows, not necessarily the area allocated to this specific component.
    fn resize(&mut self, width: u16, height: u16) -> Result<Action> {
        _ = (width, height);
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection or focus upwards within the component.
    ///
    /// The exact behavior depends on the component's nature (e.g., moving up in a list, focusing an element above the
    /// current one).
    fn move_up(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection or focus downwards within the component.
    ///
    /// The exact behavior depends on the component's nature (e.g., moving down in a list, focusing an element below the
    /// current one).
    fn move_down(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection or focus to the left within the component.
    ///
    /// The `word` parameter indicates whether the movement should be applied to a whole word or just a single
    /// character.
    ///
    /// The exact behavior depends on the component's nature (e.g., moving left in a text input, focusing an element to
    /// the left of the current one).
    fn move_left(&mut self, word: bool) -> Result<Action> {
        let _ = word;
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection or focus to the right within the component.
    ///
    /// The `word` parameter indicates whether the movement should be applied to a whole word or just a single
    /// character.
    ///
    /// The exact behavior depends on the component's nature (e.g., moving right in a text input, focusing an element to
    /// the right of the current one).
    fn move_right(&mut self, word: bool) -> Result<Action> {
        let _ = word;
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection to the previous logical item or element.
    ///
    /// This is often used for navigating backwards in a sequence (e.g., previous tab,
    /// previous item in a wizard) that may not map directly to simple directional moves.
    fn move_prev(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection to the next logical item or element.
    ///
    /// This is often used for navigating forwards in a sequence (e.g., next tab, next item in a wizard) that may not
    /// map directly to simple directional moves.
    fn move_next(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection to the beginning (e.g., "Home" key).
    ///
    /// The `absolute` parameter indicates whether the movement should be absolute (to the very start of the component)
    /// or relative (to the start of the current logical section).
    ///
    /// This typically moves the selection to the first item in a list, the start of a text input, or the first element
    /// in a navigable group.
    fn move_home(&mut self, absolute: bool) -> Result<Action> {
        let _ = absolute;
        Ok(Action::NoOp)
    }

    /// Handles a request to move the selection to the end (e.g., "End" key).
    ///
    /// The `absolute` parameter indicates whether the movement should be absolute (to the very end of the component) or
    /// relative (to the end of the current logical section).
    ///
    /// This typically moves the selection to the last item in a list, the end of a text input, or the last element in a
    /// navigable group.
    fn move_end(&mut self, absolute: bool) -> Result<Action> {
        let _ = absolute;
        Ok(Action::NoOp)
    }

    /// Handles a request to undo the last action performed in the component.
    ///
    /// The specific behavior depends on the component's nature (e.g., undoing a text edit, reverting a selection
    /// change).
    fn undo(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to redo the last undone action in the component.
    ///
    /// The specific behavior depends on the component's nature (e.g., redoing a text edit, restoring a selection
    /// change).
    fn redo(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles the insertion of a block of text into the component.
    ///
    /// This is typically used for pasting text into a focused input field.
    /// If the component or its currently focused element does not support text input, this method may do nothing.
    fn insert_text(&mut self, text: String) -> Result<Action> {
        _ = text;
        Ok(Action::NoOp)
    }

    /// Handles the insertion of a single character into the component.
    ///
    /// This is typically used for typing into a focused input field.
    /// If the component or its currently focused element does not support text input, this method may do nothing.
    fn insert_char(&mut self, c: char) -> Result<Action> {
        _ = c;
        Ok(Action::NoOp)
    }

    /// Handles a request to insert a newline character into the component.
    ///
    /// This is typically used for multiline text inputs or text areas where pressing "Shift+Enter" should create a new
    /// line.
    fn insert_newline(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles the deletion key from the component, typically from a focused input field.
    ///
    /// The `backspace` parameter distinguishes between deleting the character before the cursor (backspace) and
    /// deleting the character at/after the cursor (delete).
    ///
    /// The `word` parameter indicates whether the deletion should be applied to a whole word or just a single
    /// character.
    fn delete(&mut self, backspace: bool, word: bool) -> Result<Action> {
        _ = backspace;
        _ = word;
        Ok(Action::NoOp)
    }

    /// Handles a request to delete the currently selected item or element within the component.
    ///
    /// The exact behavior depends on the component (e.g., deleting an item from a list, clearing a field).
    async fn selection_delete(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to update or modify the currently selected item or element.
    ///
    /// This could mean initiating an edit mode for an item, toggling a state, or triggering some other modification.
    async fn selection_update(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to confirm the currently selected item or element.
    ///
    /// This is often equivalent to an "Enter" key press on a selected item, triggering its primary action (e.g.,
    /// executing a command, submitting a form item, navigating into a sub-menu).
    async fn selection_confirm(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// Handles a request to execute the primary action associated with the currently selected item or element within
    /// the component.
    ///
    /// This method is typically invoked when the user wants to "run" or "activate"
    /// the selected item. For example, this could mean:
    /// - Executing a shell command that is currently selected in a list.
    /// - Starting a process associated with the selected item.
    /// - Triggering a significant, non-trivial operation.
    ///
    /// The specific behavior is determined by the component and the nature of its items.
    async fn selection_execute(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// For the search command only, toggle the search mode
    fn toggle_search_mode(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }

    /// For the search command only, toggle the user-only mode
    fn toggle_search_user_only(&mut self) -> Result<Action> {
        Ok(Action::NoOp)
    }
}

/// A placeholder component that provides no-op implementations for the [Component] trait.
///
/// This component is useful as a default or when no interactive component is currently active in the TUI.
pub struct EmptyComponent;
impl Component for EmptyComponent {
    fn name(&self) -> &'static str {
        "EmptyComponent"
    }

    fn min_inline_height(&self) -> u16 {
        0
    }

    fn render(&mut self, _frame: &mut Frame, _area: Rect) {}
}
