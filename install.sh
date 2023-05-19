#!/bin/bash

set -eo pipefail

# Find target
arch=$(uname -m)
if [ "$arch" = "arm64" ]; then
  arch=aarch64
fi
case "$OSTYPE" in
  linux*)   os="unknown-$OSTYPE" 
            INTELLI_HOME="${INTELLI_HOME:-$HOME/.local/share/intelli-shell}"
            ;;
  darwin*)  os="apple-darwin" 
            INTELLI_HOME="${INTELLI_HOME:-$HOME/Library/Application Support/org.IntelliShell.Intelli-Shell}"
            ;; 
  msys*)    os="pc-windows-msvc" 
            POSIX_APPDATA=$(echo "/$APPDATA" | sed 's/\\/\//g' | sed 's/://')
            INTELLI_HOME="${INTELLI_HOME:-$POSIX_APPDATA/IntelliShell/Intelli-Shell/data}"
            ;;
  *)        echo "OS type not supported: $OSTYPE" 
            exit 1 
            ;;
esac
target="$arch-$os"

# Download latest release
mkdir -p "$INTELLI_HOME/bin"
curl -Lsf https://github.com/lasantosr/intelli-shell/releases/latest/download/intelli-shell-$target.tar.gz | tar zxf - -C "$INTELLI_HOME/bin" 

echo "Successfully installed IntelliShell at: $INTELLI_HOME"

if [[ "${INTELLI_SKIP_PROFILE:-0}" == "0" ]]; then

  # Update rc
  files=()
  function update_rc () {
    if [ -f "$1" ]; then
      sourced=$(cat $1 | { grep -E '.*intelli-shell.*' || test $? = 1; })
    else
      sourced=
    fi
    if [[ -z "$sourced" ]];
    then
      files+=("$1")
      echo -e '\n# IntelliShell' >> "$1"
      printf "export INTELLI_HOME=%q\n" "$INTELLI_HOME" >> "$1"
      echo '# export INTELLI_SEARCH_HOTKEY=\\C-@' >> "$1"
      echo '# export INTELLI_LABEL_HOTKEY=\\C-l' >> "$1"
      echo '# export INTELLI_BOOKMARK_HOTKEY=\\C-b' >> "$1"
      echo '# export INTELLI_SKIP_ESC_BIND=0' >> "$1"
      echo 'alias intelli-shell="$INTELLI_HOME/bin/intelli-shell"' >> "$1"
      echo 'source "$INTELLI_HOME/bin/intelli-shell.sh"' >> "$1"
    fi
  }

  update_rc "$HOME/.bashrc"
  if [[ -f "$HOME/.bash_profile" ]]; then
    update_rc "$HOME/.bash_profile"
  fi
  if [[ -f "/bin/zsh" ]]; then
    update_rc "$HOME/.zshrc"
  fi
  if [[ -f "/usr/bin/fish" ]]; then
    config="$HOME/.config/fish/config.fish"
    if [ -f "$config" ]; then
      sourced=$(cat $config | { grep -E '.*intelli-shell.*' || test $? = 1; })
    else
      mkdir -p "$HOME/.config/fish"
      sourced=
    fi
    if [[ -z "$sourced" ]];
    then
      files+=("$config")
      echo -e '\n# IntelliShell' >> "$config"
      printf "set -gx INTELLI_HOME %q\n" "$INTELLI_HOME" >> "$config"
      echo '# set -gx INTELLI_SEARCH_HOTKEY \cr' >> "$config"
      echo '# set -gx INTELLI_LABEL_HOTKEY \cl' >> "$config"
      echo '# set -gx INTELLI_BOOKMARK_HOTKEY \cb' >> "$config"
      echo '# set -gx INTELLI_SKIP_ESC_BIND 0' >> "$config"
      echo 'source "$INTELLI_HOME/bin/intelli-shell.fish"' >> "$config"
    fi
  fi

  if [ ${#files[@]} -ne 0 ]; then
    echo "The following files were updated: ${files[@]}"
    echo "You might have to re-source your profile or restart your terminal."
  fi

else

  echo "You might want to update your profile files!"
  printf "export INTELLI_HOME=%q\n" "$INTELLI_HOME"
  echo 'source "$INTELLI_HOME/bin/intelli-shell.sh"'

fi
