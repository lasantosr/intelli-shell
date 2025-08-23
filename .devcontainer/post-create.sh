#!/bin/bash
set -e

# Get the workspace folder path from the first argument
# The devcontainer.json will pass ${containerWorkspaceFolder} here
WORKSPACE_FOLDER="${1:-/workspaces/intelli-shell}" # Default fallback, though it should always be passed

# Define paths based on the workspace folder
INTELLI_SHELL_DEBUG_PATH="$WORKSPACE_FOLDER/target/debug/intelli-shell"

echo "Running post-create script..."
echo "Workspace folder: $WORKSPACE_FOLDER"

BLOCK_MARKER="# IntelliShell debug"

# --- Bash Configuration ---
BASHRC_FILE="$HOME/.bashrc"
if ! grep -q "$BLOCK_MARKER" "$BASHRC_FILE"; then
  echo "Updating $BASHRC_FILE..."
  cat << EOF >> "$BASHRC_FILE"

$BLOCK_MARKER
alias intelli-shell="$INTELLI_SHELL_DEBUG_PATH"
alias is="$INTELLI_SHELL_DEBUG_PATH"
eval "\$(intelli-shell init bash)"
EOF
else
  echo "$BASHRC_FILE already configured for IntelliShell."
fi

# --- Zsh Configuration ---
ZSHRC_FILE="$HOME/.zshrc"
# Check if the block already exists
if ! grep -q "$BLOCK_MARKER" "$ZSHRC_FILE"; then
  echo "Updating $ZSHRC_FILE..."
  cat << EOF >> "$ZSHRC_FILE"

$BLOCK_MARKER
alias intelli-shell="$INTELLI_SHELL_DEBUG_PATH"
alias is="$INTELLI_SHELL_DEBUG_PATH"
eval "\$(intelli-shell init zsh)"
EOF
else
  echo "$ZSHRC_FILE already configured for IntelliShell."
fi

# --- Fish Configuration ---
FISH_CONFIG_DIR="$HOME/.config/fish"
FISH_CONFIG_FILE="$FISH_CONFIG_DIR/config.fish"
mkdir -p "$FISH_CONFIG_DIR"
if ! grep -q "$BLOCK_MARKER" "$FISH_CONFIG_FILE" 2>/dev/null; then
  echo "Updating $FISH_CONFIG_FILE..."
  cat << EOF >> "$FISH_CONFIG_FILE"

$BLOCK_MARKER
function intelli-shell --description 'IntelliShell'
  $INTELLI_SHELL_DEBUG_PATH \$argv;
end
function is --description 'IntelliShell'
  $INTELLI_SHELL_DEBUG_PATH \$argv;
end
intelli-shell init fish | source
EOF
else
  echo "$FISH_CONFIG_FILE already configured for IntelliShell."
fi

echo "Shell configuration complete."

# --- IntelliShell Configuration ---
INTELLI_SHELL_CONFIG_DIR="$HOME/.config/intelli-shell"
INTELLI_SHELL_CONFIG_FILE="$INTELLI_SHELL_CONFIG_DIR/config.toml"

mkdir -p "$INTELLI_SHELL_CONFIG_DIR"
if [ ! -f "$INTELLI_SHELL_CONFIG_FILE" ]; then
  echo "Creating IntelliShell configuration file..."
  cat << EOF > "$INTELLI_SHELL_CONFIG_FILE"
check_updates = false
inline = true

[search]
exec_on_alias_match = true

[logs]
enabled = true
filter = "info,intelli_shell=trace"

[theme]
accent = "136"
comment = "rgb(106, 153, 66)"
highlight = "none"
highlight_primary = "220"
highlight_secondary = "222"
highlight_accent = "208"
highlight_comment = "rgb(143, 221, 75)"

[ai]
enabled = true
EOF
else
  echo "IntelliShell configuration file already exists."
fi

# --- Run Initial Cargo Build ---
echo "Running initial cargo build..."
cargo build

echo "Post-create script finished."
