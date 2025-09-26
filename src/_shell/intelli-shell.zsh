# --- Zsh Integration (using ZLE - Zsh Line Editor) ---
# https://zsh.sourceforge.io/Guide/zshguide04.html
# https://zsh.sourceforge.io/Doc/Release/Zsh-Line-Editor.html

# Define key bindings, using defaults if environment variables are not set
intelli_search_key="${INTELLI_SEARCH_HOTKEY:-^@}"
intelli_bookmark_key="${INTELLI_BOOKMARK_HOTKEY:-^b}"
intelli_variable_key="${INTELLI_VARIABLE_HOTKEY:-^l}"
intelli_fix_key="${INTELLI_FIX_HOTKEY:-^x}"

# Helper function to execute intelli-shell and update the ZLE buffer
function _intelli_exec {
  local temp_result_file=$(mktemp)

  # Clear the buffer and invalidate to force a redraw of the line
  BUFFER=""
  zle -I

  # Run intelli-shell
  intelli-shell --skip-execution --file-output "$temp_result_file" "$@"
  local exit_status=$?
  
  # If the output file is missing or empty, there's nothing to process (likely a crash)
  if [[ ! -s "$temp_result_file" ]]; then
    rm -f "$temp_result_file" 2>/dev/null
    return $exit_status
  fi

  # Read the file content and parse it
  local -a lines
  lines=("${(f)$(<"$temp_result_file")}")
  rm -f "$temp_result_file" 2>/dev/null
  local out_status="${lines[1]}"
  local action=""
  local command=""
  if ((${#lines[@]} > 1)); then
    action="${lines[2]}"
  fi
  if ((${#lines[@]} > 2)); then
    command="${(F)lines[3,-1]}"
  fi

  # Determine whether to start a new prompt line
  if [[ "$out_status" == "DIRTY" || $exit_status -ne 0 ]]; then
    # Nothing to do, ZLE will redraw the prompt on the next line (because of `zle -I` above)
  else 
    # For any clean action, stay on the same line.
    zle .redisplay
  fi

  # Determine the content of the buffer
  if [[ "$action" == "REPLACE" ]]; then
    BUFFER="$command"
    zle .end-of-line
  elif [[ "$action" == "EXECUTE" ]]; then
    BUFFER="$command"
    zle .accept-line
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

# ZLE widget function for fixing commands
function _intelli_fix {
  local hist
  hist=$(fc -l -n -5)
  _intelli_exec fix --history "$hist" "$BUFFER"
}

# Bind ESC to kill the whole line if not skipped
if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then
  bindkey '\e' kill-whole-line
fi

# Register the functions as ZLE widgets
zle -N _intelli_search
zle -N _intelli_save
zle -N _intelli_variable
zle -N _intelli_fix

# Bind the keys to the widgets
bindkey "$intelli_search_key" _intelli_search
bindkey "$intelli_bookmark_key" _intelli_save
bindkey "$intelli_variable_key" _intelli_variable
bindkey "$intelli_fix_key" _intelli_fix

# Export the execution prompt variable
export INTELLI_EXEC_PROMPT=$(print -r -- "$PS2" | sed 's/%{//g; s/%}//g')
