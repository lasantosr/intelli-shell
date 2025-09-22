# --- Bash Integration (using Readline) ---
# https://www.gnu.org/software/bash/manual/html_node/Bash-Builtins.html#index-bind

# Define key bindings, using defaults if environment variables are not set
intelli_search_key="${INTELLI_SEARCH_HOTKEY:-\C-@}"
intelli_bookmark_key="${INTELLI_BOOKMARK_HOTKEY:-\C-b}"
intelli_variable_key="${INTELLI_VARIABLE_HOTKEY:-\C-l}"
intelli_fix_key="${INTELLI_FIX_HOTKEY:-\C-x}"

# Helper function to execute intelli-shell and update the Readline buffer
function _intelli_exec {
  local temp_result_file=$(mktemp)
  
  # In Bash, readline clears the prompt when an external command is run from a binding.
  # We print the last line of the prompt here so the user sees it while the TUI is active.
  # This printed line will be cleared later before readline redraws the real prompt.
  echo -n "${PS1@P}" | tail -n 1

  # Run intelli-shell
  intelli-shell --extra-line --file-output "$temp_result_file" "$@"
  local exit_status=$?

  # If the output file is missing or empty, there's nothing to process (likely a crash)
  if [[ ! -s "$temp_result_file" ]]; then
    # Panic report was likely printed, we must start a new prompt line
    echo -n "${PS1@P}" | head -n -1
    rm -f "$temp_result_file" 2>/dev/null
    return $exit_status
  fi

  # Read the file content and parse it
  local -a lines
  mapfile -t lines < "$temp_result_file"
  rm -f "$temp_result_file" 2>/dev/null
  local status="${lines[0]}"
  local action=""
  local command=""
  if ((${#lines[@]} > 1)); then
    action="${lines[1]}"
  fi
  if ((${#lines[@]} > 2)); then
    printf -v command '%s\n' "${lines[@]:2}"
    command="${command%$'\n'}"
  fi

  # Determine the content of the readline buffer
  if [[ "$action" == "REPLACE" ]]; then
    READLINE_LINE="$command"
    READLINE_POINT=${#command}
  else
    # For EXECUTED, DIRTY, or just CLEAN, the buffer should be empty
    READLINE_LINE=""
    READLINE_POINT=0
  fi

  # Determine whether to start a new prompt line
  if [[ "$status" == "DIRTY" || "$action" == "EXECUTED" || $exit_status -ne 0 ]]; then
    # If a new prompt is needed but the tool didn't output anything (e.g., Ctrl+C),
    # we must print a newline ourselves to advance the cursor
    if [[ "$status" == "CLEAN" && "$action" != "EXECUTED" ]]; then
      echo
    fi
    # Print the multi-line part of the prompt that readline won't draw
    echo -n "${PS1@P}" | head -n -1
  fi

  # Finally, clear the temporary prompt line we echoed at the start
  printf '\r\033[2K'
}

# Readline callable function for searching
function _intelli_search {
  _intelli_exec search -i "$READLINE_LINE"
}

# Readline callable function for saving/bookmarking
function _intelli_save {
  _intelli_exec new -i "$READLINE_LINE"
}

# Readline callable function for variable replacement
function _intelli_variable {
  _intelli_exec replace -i "$READLINE_LINE"
}

# Readline callable function for fixing commands
function _intelli_fix {
  local hist
  hist=$(fc -l -n -5)
  _intelli_exec fix --history "$hist" "$READLINE_LINE"
}

# Bind ESC to kill the whole line if not skipped
if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then
  bind '"\e": kill-whole-line'
fi

# Bind the keys to execute the shell functions
bind -x '"'"$intelli_search_key"'":"_intelli_search"'
bind -x '"'"$intelli_bookmark_key"'":"_intelli_save"'
bind -x '"'"$intelli_variable_key"'":"_intelli_variable"'
bind -x '"'"$intelli_fix_key"'":"_intelli_fix"'

# Export the execution prompt variable
export INTELLI_EXEC_PROMPT=$(printf '%s' "$PS2" | sed 's/\\\[//g; s/\\\]//g')
