# --- Bash Integration (using Readline) ---
# https://www.gnu.org/software/bash/manual/html_node/Bash-Builtins.html#index-bind

# Define key bindings, using defaults if environment variables are not set
intelli_search_key="${INTELLI_SEARCH_HOTKEY:-\C-@}"
intelli_bookmark_key="${INTELLI_BOOKMARK_HOTKEY:-\C-b}"
intelli_variable_key="${INTELLI_VARIABLE_HOTKEY:-\C-l}"

# Helper function to execute intelli-shell and update the Readline buffer
function _intelli_exec {
  local output
  local exit_status
  local temp_result_file=$(mktemp)
  local executed_output="####EXECUTED####"
  
  # Print the last line of PS1 (readline clears it)
  echo -n "${PS1@P}" | tail -n 1

  intelli-shell --extra-line --file-output "$temp_result_file" "$@" 
  exit_status=$?

  # Read output from temp file if it exists and remove it
  if [[ -f "$temp_result_file" ]]; then
    output=$(cat "$temp_result_file")
  fi
  rm -f "$temp_result_file" 2>/dev/null

  # Check if the command failed
  if [[ $exit_status -ne 0 ]]; then
    # Print every line of PS1 except the last one (readline will draw it)
    echo -n "${PS1@P}" | head -n -1
    return $exit_status
  fi

  # If a command was executed, print the prompt as well
  if [[ "$output" = "$executed_output" ]] ; then
    echo -n "${PS1@P}" | head -n -1
    output=""
  fi
  # Clear the line we previously printed, to avoid readline from duplicating it
  printf '\r\033[2K'
  
  # Rewrite the Readline buffer variables
  READLINE_LINE=${output}
  READLINE_POINT=${#READLINE_LINE}
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

# Bind ESC to kill the whole line if not skipped
if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then
  bind '"\e": kill-whole-line'
fi

# Bind the keys to execute the shell functions
bind -x '"'"$intelli_search_key"'":"_intelli_search"'
bind -x '"'"$intelli_bookmark_key"'":"_intelli_save"'
bind -x '"'"$intelli_variable_key"'":"_intelli_variable"'

# Export the execution prompt variable
export INTELLI_EXEC_PROMPT=$(printf '%s' "$PS2" | sed 's/\\\[//g; s/\\\]//g')
