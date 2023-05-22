function intelli-shell --description 'IntelliShell'
  $INTELLI_HOME/bin/intelli-shell $argv;
end

function _intelli_exec
    # Temp file for output
    set TMP_FILE (mktemp -t intelli-shell.XXXXXXXX)
    set TMP_FILE_MSG (mktemp -t intelli-shell.XXXXXXXX)
    # Exec command
    intelli-shell --inline --inline-extra-line --file-output="$TMP_FILE" $argv 2> $TMP_FILE_MSG
    # Capture output
    set INTELLI_OUTPUT (cat "$TMP_FILE" | string collect)
    set INTELLI_MESSAGE (cat "$TMP_FILE_MSG" | string collect)
    rm -f $TMP_FILE
    rm -f $TMP_FILE_MSG
    if test -n "$INTELLI_MESSAGE"
      echo $INTELLI_MESSAGE
    end
    # Replace line
    commandline -f repaint
    commandline -r "$INTELLI_OUTPUT"
end

function _intelli_search 
    set LINE (commandline)
    _intelli_exec search "$LINE"
end

function _intelli_save
    set LINE (commandline)
    _intelli_exec new -c "$LINE"
end

function _intelli_label
    set LINE (commandline)
    _intelli_exec label "$LINE"
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
