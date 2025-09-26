# --- Fish Integration ---
# https://fishshell.com/docs/current/cmds/bind.html
# https://fishshell.com/docs/current/cmds/commandline.html

# Helper function to execute intelli-shell and update the command line buffer
function _intelli_exec --description "Executes intelli-shell and updates command line"
  set -l temp_result_file (mktemp)

  # --- Helper function to handle multi-line prompts ---
  function _make_room_for_prompt
    # To avoid repainting over existing output with multi-line prompts,
    # we first print N-1 newlines to create the required vertical space.
    set -l prompt_lines (string split \n -- (fish_prompt) | count)
    if test $prompt_lines -gt 1
      for i in (seq (math $prompt_lines - 1))
        echo ""
      end
    end
  end

  # Clear the buffer with ANSI escapes to be rendered immediately
  set -l buffer_len (string length -- (commandline))
  if test $buffer_len -gt 0
    echo -ne "\e[$buffer_len"D'\e[K'
    commandline -r -- ""
  end

  # Run intelli-shell
  intelli-shell --extra-line --skip-execution --file-output "$temp_result_file" $argv
  set -l exit_status $status

  # If the output file is missing or empty, there's nothing to process (likely a crash)
  if not test -s "$temp_result_file"
    # Panic report was likely printed, we must start a new prompt line
    _make_room_for_prompt
    commandline -f repaint
    rm -f "$temp_result_file" 2>/dev/null
    return $exit_status
  end

  # Read the file content and parse it
  set -l lines (string split \n -- (cat "$temp_result_file"))
  rm -f "$temp_result_file" 2>/dev/null
  set -l out_status $lines[1]
  set -l action ""
  set -l command ""
  if test (count $lines) -gt 1
    set action $lines[2]
  end
  if test (count $lines) -gt 2
    set command (string join \n $lines[3..-1])
  end
  
  # Determine whether to start a new prompt line
  if test "$out_status" = "DIRTY" -o $exit_status -ne 0
    # If a new prompt is needed but the tool didn't output anything (e.g., Ctrl+C),
    # we must print a newline ourselves to advance the cursor
    if test "$out_status" = "CLEAN"
      echo ""
    end
    _make_room_for_prompt
  end

  # Determine the content of the buffer
  if test "$action" = "REPLACE"
    commandline -r -- "$command"
    commandline -f end-of-line
  else if test "$action" = "EXECUTE"
    commandline -r -- "$command"
    commandline -f execute
  end

  # Always, repaint the prompt to ensure it's correctly drawn after those ANSI chars
  commandline -f repaint

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
