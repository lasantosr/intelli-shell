function intelli-shell --description 'IntelliShell'
  $INTELLI_HOME/bin/intelli-shell $argv;
end

function _intelli_search 
    set LINE (commandline)
    # Temp file for output
    set TMP_FILE (mktemp -t intelli-shell.XXXXXXXX)
    # Exec command
    intelli-shell --inline --inline-extra-line --file-output="$TMP_FILE" search "$LINE"
    # Capture output
    set INTELLI_OUTPUT (cat "$TMP_FILE" | string collect)
    rm -f $TMP_FILE
    # Replace line
    commandline -f repaint
    commandline -r "$INTELLI_OUTPUT"
end

function _intelli_save
    set LINE (commandline)
    # Temp file for output
    set TMP_FILE (mktemp -t intelli-shell.XXXXXXXX)
    # Exec command
    intelli-shell --inline --inline-extra-line --file-output="$TMP_FILE" save "$LINE"
    # Capture output
    set INTELLI_OUTPUT (cat "$TMP_FILE" | string collect)
    rm -f $TMP_FILE
    # Replace line
    commandline -f repaint
    commandline -r "$INTELLI_OUTPUT"
end

function _intelli_label
    set LINE (commandline)
    # Temp file for output
    set TMP_FILE (mktemp -t intelli-shell.XXXXXXXX)
    # Exec command
    intelli-shell --inline --inline-extra-line --file-output="$TMP_FILE" label "$LINE"
    # Capture output
    set INTELLI_OUTPUT (cat "$TMP_FILE" | string collect)
    rm -f $TMP_FILE
    # Replace line
    commandline -f repaint
    commandline -r "$INTELLI_OUTPUT"
end

function fish_user_key_bindings
  if [ "$INTELLI_SKIP_ESC_BIND" != "1" ] 
    bind --preset \e 'kill-whole-line'
  end
  if test -n "$INTELLI_SEARCH_HOTKEY"
    bind $INTELLI_SEARCH_HOTKEY '_intelli_search'
  else
    bind -k nul '_intelli_search'
  end
  if test -n "$INTELLI_SAVE_HOTKEY"
    bind $INTELLI_SAVE_HOTKEY '_intelli_save'
  else
    bind \cb '_intelli_save'
  end
  if test -n "$INTELLI_LABEL_HOTKEY"
    bind $INTELLI_LABEL_HOTKEY '_intelli_label'
  else
    bind \cl '_intelli_label'
  end
end
