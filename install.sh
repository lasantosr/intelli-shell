set -e 

# Retrieve default shell
shell="${SHELL##*/}"

if [[ "$shell" != "bash" ]] && [[ "$shell" != "zsh" ]];
then
  echo "Terminal $shell is not compatible";
  exit 1;
fi

# Find target
arch=$(uname -m)
case "$OSTYPE" in
  linux*)   os="unknown-$OSTYPE" 
            INTELLI_HOME="$HOME/.local/share/intelli-shell"
            ;;
  darwin*)  os="apple-darwin" 
            INTELLI_HOME="$HOME/Library/Application Support/org.IntelliShell.Intelli-Shell"
            ;; 
  msys*)    os="pc-windows-msvc" 
            POSIX_APPDATA=$(echo "/$APPDATA" | sed 's/\\/\//g' | sed 's/://')
            INTELLI_HOME="$POSIX_APPDATA/IntelliShell/Intelli-Shell"
            ;;
  *)        echo "OS type not supported: $OSTYPE" 
            exit 1 
            ;;
esac
target="$arch-$os"

# Download latest release
mkdir -p $INTELLI_HOME/bin
curl -Lsf https://github.com/lasantosr/intelli-shell/releases/latest/download/intelli-shell-$target.tar.gz | tar zxf - -C $INTELLI_HOME/bin 

# Update rc
if [[ "$os" = "apple-darwin" ]] && [[ "$shell" = "bash" ]];
then
  rcfile=".bash_profile"
else
  rcfile=".${shell}rc"
fi
echo -e '\n# IntelliShell' >> ~/$rcfile
echo "INTELLI_HOME=$INTELLI_HOME" >> ~/$rcfile
echo '# export INTELLI_SEARCH_HOTKEY=\C-@' >> ~/$rcfile
echo '# export INTELLI_SAVE_HOTKEY=C-b' >> ~/$rcfile
echo '# export INTELLI_SKIP_ESC_BIND=0' >> ~/$rcfile
echo 'alias intelli-shell="$INTELLI_HOME/bin/intelli-shell"' >> ~/$rcfile
echo 'source $INTELLI_HOME/bin/intelli-shell.sh' >> ~/$rcfile

echo "Successfully installed IntelliShell at: $INTELLI_HOME"
echo "Please restart the terminal or re-source ~/$rcfile, where further customizations can be made"
