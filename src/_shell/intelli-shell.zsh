# --- Zsh Integration (using ZLE - Zsh Line Editor) ---
# https://zsh.sourceforge.io/Guide/zshguide04.html
# https://zsh.sourceforge.io/Doc/Release/Zsh-Line-Editor.html

# Define key bindings, using defaults if environment variables are not set
intelli_search_key="${INTELLI_SEARCH_HOTKEY:-^@}"
intelli_bookmark_key="${INTELLI_BOOKMARK_HOTKEY:-^b}"
intelli_variable_key="${INTELLI_VARIABLE_HOTKEY:-^l}"

# Helper function to execute intelli-shell and update the ZLE buffer
function _intelli_exec {
  local output
  local exit_status
  local temp_result_file=$(mktemp)
  local execute_prefix="____execute____"

  intelli-shell --extra-line --skip-execution --file-output "$temp_result_file" "$@"
  exit_status=$?

  # Read output from temp file if it exists and remove it
  if [[ -f "$temp_result_file" ]]; then
    output=$(cat "$temp_result_file")
  fi
  rm -f "$temp_result_file" 2>/dev/null

  # Check if the command failed
  if [[ $exit_status -ne 0 ]]; then
    zle redisplay
    return $exit_status
  fi

  # Check if the output starts with the special execution prefix
  if [[ "$output" == "${execute_prefix}"* ]]; then
    # If it does, strip the prefix from the output
    BUFFER="${output#$execute_prefix}"
    # Execute it
    zle accept-line
  else
    # Otherwise, perform the original action: just update the line
    BUFFER=$output
    zle end-of-line
    zle redisplay
  fi

}

# ZLE widget function for searching
function _intelli_search {
  _intelli_exec search -i "$BUFFER"
}

# ZLE widget function for saving/bookmarking
function _intelli_save {
  _intelli_exec new -i "$BUFFER"
}

# ZLE widget function for variable replacement
function _intelli_variable {
  _intelli_exec replace -i "$BUFFER"
}

# Bind ESC to kill the whole line if not skipped
if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then
  bindkey '\e' kill-whole-line
fi

# Register the functions as ZLE widgets
zle -N _intelli_search
zle -N _intelli_save
zle -N _intelli_variable

# Bind the keys to the widgets
bindkey "$intelli_search_key" _intelli_search
bindkey "$intelli_bookmark_key" _intelli_save
bindkey "$intelli_variable_key" _intelli_variable

# Export the execution prompt variable
export INTELLI_EXEC_PROMPT=$(print -r -- "$PS2" | sed 's/%{//g; s/%}//g')
