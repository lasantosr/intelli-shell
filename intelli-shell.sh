# Default bindings
intelli_search_key="${INTELLI_SEARCH_HOTKEY:-\C-@}"
intelli_save_key="${INTELLI_SAVE_HOTKEY:-\C-b}"

if [[ -n "$ZSH_VERSION" ]]; then
    # zshell
    # https://zsh.sourceforge.io/Guide/zshguide04.html

    function _intelli_search {
        # Temp file for output
        tmp_file=$(mktemp -t intelli-shell.XXXXXXXX)
        # Exec command
        intelli-shell --inline --inline-extra-line --file-output="$tmp_file" search "$BUFFER"
        # Capture output
        INTELLI_OUTPUT=$(<$tmp_file)
        rm -f $tmp_file
        # Rewrite line
        BUFFER="$INTELLI_OUTPUT"
        zle end-of-line
    }

    function _intelli_save {
        # Temp file for output
        tmp_file=$(mktemp -t intelli-shell.XXXXXXXX)
        # Exec command
        intelli-shell --inline --inline-extra-line --file-output="$tmp_file" save "$BUFFER"
        # Capture output
        INTELLI_OUTPUT=$(<$tmp_file)
        rm -f $tmp_file
        # Rewrite line
        BUFFER="$INTELLI_OUTPUT"
        zle end-of-line
    }
    
    if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then bindkey "\e" kill-whole-line; fi
    zle -N _intelli_search
    zle -N _intelli_save
    bindkey "$intelli_search_key" _intelli_search 
    bindkey "$intelli_save_key" _intelli_save
    
elif [[ -n "$BASH" ]]; then
    # bash
    # https://www.gnu.org/software/bash/manual/html_node/Bash-Builtins.html#index-bind

    function intelli_search {
        # Temp file for output
        tmp_file=$(mktemp -t intelli-shell.XXXXXXXX)
        # Exec command
        intelli-shell --inline --file-output="$tmp_file" search "$READLINE_LINE"
        # Capture output
        INTELLI_OUTPUT=$(<$tmp_file)
        rm -f $tmp_file
        # Rewrite line
        READLINE_LINE="$INTELLI_OUTPUT"
        READLINE_POINT=${#READLINE_LINE}
    }

    function intelli_save {
        # Temp file for output
        tmp_file=$(mktemp -t intelli-shell.XXXXXXXX)
        # Exec command
        intelli-shell --inline --file-output="$tmp_file" save "$READLINE_LINE"
        # Capture output
        INTELLI_OUTPUT=$(<$tmp_file)
        rm -f $tmp_file
        # Rewrite line
        READLINE_LINE="$INTELLI_OUTPUT"
        READLINE_POINT=${#READLINE_LINE}
    }

    if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then bind '"\e": kill-whole-line'; fi
    bind -x '"'"$intelli_search_key"'":intelli_search'
    bind -x '"'"$intelli_save_key"'":intelli_save'
fi
