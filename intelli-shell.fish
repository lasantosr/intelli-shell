function intelli-shell --description 'IntelliShell'
  $INTELLI_HOME/bin/intelli-shell $argv;
end

function _intelli_exec
    set p_lines (fish_prompt | string split0 | wc -l)
    # Swap stderr and stdout
    if test (math $p_lines + 0) -gt "1"
      set INTELLI_OUTPUT (intelli-shell --inline --inline-extra-line  $argv 3>&1 1>&2 2>&3)
    else 
      set INTELLI_OUTPUT (intelli-shell --inline  $argv 3>&1 1>&2 2>&3)
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
