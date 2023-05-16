# Default bindings
intelli_search_key="${INTELLI_SEARCH_HOTKEY:-\C-@}"
intelli_bookmark_key="${INTELLI_BOOKMARK_HOTKEY:-\C-b}"
intelli_label_key="${INTELLI_LABEL_HOTKEY:-\C-l}"

if [[ -n "$ZSH_VERSION" ]]; then
    # zshell
    # https://zsh.sourceforge.io/Guide/zshguide04.html

    function _intelli_exec {
        ps1_lines=$(echo "$PS1" | wc -l)
        # Temp file for output
        tmp_file=$(mktemp -t intelli-shell.XXXXXXXX)
        tmp_file_msg=$(mktemp -t intelli-shell.XXXXXXXX)
        # Exec command
        intelli-shell --inline --inline-extra-line --file-output="$tmp_file" "$@" 2> $tmp_file_msg
        # Capture output
        INTELLI_OUTPUT=$(<$tmp_file)
        INTELLI_MESSAGE=$(<$tmp_file_msg)
        rm -f $tmp_file
        rm -f $tmp_file_msg
        if [[ -n "$INTELLI_MESSAGE" ]]; then
            echo "$INTELLI_MESSAGE"
            [[ "$ps1_lines" -gt 1 ]] && echo ""
        fi
        # Rewrite line
        zle reset-prompt
        BUFFER="$INTELLI_OUTPUT"
        zle end-of-line
    }

    function _intelli_search {
        _intelli_exec search "$BUFFER"
    }

    function _intelli_save {
        _intelli_exec save "$BUFFER"
    }

    function _intelli_label {
        _intelli_exec label "$BUFFER"
    }
    
    if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then bindkey "\e" kill-whole-line; fi
    zle -N _intelli_search
    zle -N _intelli_save
    zle -N _intelli_label
    bindkey "$intelli_search_key" _intelli_search 
    bindkey "$intelli_bookmark_key" _intelli_save
    bindkey "$intelli_label_key" _intelli_label
    
elif [[ -n "$BASH" ]]; then
    # bash
    # https://www.gnu.org/software/bash/manual/html_node/Bash-Builtins.html#index-bind

    function _intelli_exec {
        ps1_lines=$(echo "$PS1" | wc -l)
        # Temp file for output
        tmp_file=$(mktemp -t intelli-shell.XXXXXXXX)
        tmp_file_msg=$(mktemp -t intelli-shell.XXXXXXXX)
        # Exec command
        intelli-shell --inline --file-output="$tmp_file" "$@" 2> $tmp_file_msg
        # Capture output
        INTELLI_OUTPUT=$(<$tmp_file)
        INTELLI_MESSAGE=$(<$tmp_file_msg)
        rm -f $tmp_file
        rm -f $tmp_file_msg
        if [[ -n "$INTELLI_MESSAGE" ]]; then
            echo "$INTELLI_MESSAGE"
        fi
        # Rewrite line
        READLINE_LINE="$INTELLI_OUTPUT"
        READLINE_POINT=${#READLINE_LINE}
    }

    function _intelli_search {
        _intelli_exec search "$READLINE_LINE"
    }

    function _intelli_save {
        _intelli_exec save "$READLINE_LINE"
    }

    function _intelli_label {
        _intelli_exec label "$READLINE_LINE"
    }

    if [[ "${INTELLI_SKIP_ESC_BIND:-0}" == "0" ]]; then bind '"\e": kill-whole-line'; fi
    bind -x '"'"$intelli_search_key"'":_intelli_search'
    bind -x '"'"$intelli_bookmark_key"'":_intelli_save'
    bind -x '"'"$intelli_label_key"'":_intelli_label'
fi
