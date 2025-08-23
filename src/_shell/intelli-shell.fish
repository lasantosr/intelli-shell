# --- Fish Integration ---
# https://fishshell.com/docs/current/cmds/bind.html
# https://fishshell.com/docs/current/cmds/commandline.html

# Helper function to execute intelli-shell and update the command line buffer
function _intelli_exec --description "Executes intelli-shell and updates command line"
  set -l output ""
  set -l temp_result_file (mktemp)
  set -l execute_prefix "____execute____"

  # Clear the buffer
  set -l buffer_len (string length -- (commandline))
  if test $buffer_len -gt 0
    echo -ne "\e[$buffer_len"D'\e[K'
  end

  # Run intelli-shell
  intelli-shell --extra-line --skip-execution --file-output "$temp_result_file" $argv
  set -l exit_status $status

  # Read output from temp file if it exists and remove it
  if test -s "$temp_result_file"
    set output (cat "$temp_result_file")
  end
  rm -f "$temp_result_file" 2>/dev/null

  # Check if the command failed
  if test $exit_status -ne 0
    commandline -f bell
    return $exit_status
  end

  # Check if the output starts with the execution prefix
  if string match -q -- "$execute_prefix*" -- "$output"
    # If it does, strip the prefix from the output string
    set -l command_to_run (string sub --start=(math (string length -- "$execute_prefix") + 1) -- "$output")
    # Update the command line buffer with the command
    commandline -r -- $command_to_run
    # And execute it immediately
    commandline -f execute
  else
    # Otherwise, just update the command line with the output
    commandline -r -- $output
    commandline -f end-of-line
    commandline -f repaint
  end
end

# --- Action Functions ---

# Search function
function _intelli_search --description "IntelliShell Search"
  set -l current_line (commandline)
  _intelli_exec search -i "$current_line"
end

# Save/Bookmark function
function _intelli_save --description "IntelliShell Bookmark"
  set -l current_line (commandline)
  _intelli_exec new -i "$current_line"
end

# Variable replacement function
function _intelli_replace --description "IntelliShell Variable Replacement"
  set -l current_line (commandline)
  _intelli_exec replace -i "$current_line"
end

# Fix function
function _intelli_fix --description "IntelliShell Fix Command"
  set -l current_line (commandline)
  string join \n $history[5..1] | read -z history_str
  _intelli_exec fix --history "$history_str" "$current_line"
end

# --- Key Bindings ---
function fish_user_key_bindings
  # Use defaults if environment variables are not set
  set -l search_key '-k nul'
  set -l bookmark_key \cb
  set -l variable_key \cl
  set -l fix_key \cx

  # Override defaults if environment variables are set
  if set -q INTELLI_SEARCH_HOTKEY; and test -n "$INTELLI_SEARCH_HOTKEY"
    set search_key $INTELLI_SEARCH_HOTKEY
  end
  if set -q INTELLI_BOOKMARK_HOTKEY; and test -n "$INTELLI_BOOKMARK_HOTKEY"
    set bookmark_key $INTELLI_BOOKMARK_HOTKEY
  end
  if set -q INTELLI_VARIABLE_HOTKEY; and test -n "$INTELLI_VARIABLE_HOTKEY"
    set variable_key $INTELLI_VARIABLE_HOTKEY
  end
  if set -q INTELLI_FIX_HOTKEY; and test -n "$INTELLI_FIX_HOTKEY"
    set fix_key $INTELLI_FIX_HOTKEY
  end

  # Bind ESC to kill the whole line if not skipped
  if not set -q INTELLI_SKIP_ESC_BIND; or test "$INTELLI_SKIP_ESC_BIND" != "1"
    bind --preset \e kill-whole-line
  end

  # Bind the keys to the action functions
  if string match -q -- '\c@' $search_key; or string match -q -- '-k nul' $search_key
    bind -k nul _intelli_search
  else
    bind $search_key _intelli_search
  end
  bind $bookmark_key _intelli_save
  bind $variable_key _intelli_replace
  bind $fix_key _intelli_fix

end

# Export the execution prompt variable
if functions -q fish_prompt_second
    set -gx INTELLI_EXEC_PROMPT (fish_prompt_second)
else
    set -gx INTELLI_EXEC_PROMPT '> '
end
