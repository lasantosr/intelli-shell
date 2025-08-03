#!/bin/sh

# Exit immediately if a command exits with a non-zero status.
set -e

# Get OS type
os_type=$(uname -s)

# --- Check for Required Commands ---
# Base commands required for all platforms
required_cmds="curl tar uname mkdir rm grep sed printf"

# Add unzip check only if on a Windows-like system
case "$os_type" in
  MSYS*|MINGW*|CYGWIN*)
    required_cmds="$required_cmds unzip powershell.exe"
    ;;
esac

# Perform the check
for cmd in $required_cmds; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Error: Required command '$cmd' not found in PATH." >&2
    exit 1
  fi
done

# --- Determine OS, Architecture, and Target Details ---

# Get architecture
arch=$(uname -m)
# Normalize arm64 to aarch64 for consistency with release naming
if [ "$arch" = "arm64" ]; then
  arch="aarch64"
fi

# Determine OS string, default home directory, and archive extension
case "$os_type" in
  Linux*)
    os_kernel="linux"
    os_libc="gnu"
    archive_ext="tar.gz"

    # --- Libc Detection ---
    # Method 1: Try using ldd on /bin/sh (if ldd exists)
    if command -v ldd >/dev/null 2>&1; then
      if ldd /bin/sh 2>/dev/null | grep -q 'musl'; then
        os_libc="musl"
      fi
    fi

    # Method 2: If still 'gnu', try checking for common musl linker file existence
    # This serves as a fallback or primary method if ldd isn't present/conclusive.
    if [ "$os_libc" = "gnu" ]; then
      # Use shell's path expansion to check common locations
      # Loop through potential matches. If a file exists, the loop runs.
      for potential_musl_linker in /lib/ld-musl-*.so.? /usr/lib/ld-musl-*.so.?; do
        # Check if the glob pattern actually expanded to an existing file
        if [ -e "$potential_musl_linker" ]; then
          os_libc="musl"
          break 
        fi
      done
    fi

    # Construct the os_slug based on detected kernel and libc
    os_slug="unknown-$os_kernel-$os_libc"
    if [ -z "$INTELLI_HOME" ]; then
      # Use XDG_DATA_HOME if set, otherwise default to ~/.local/share
      INTELLI_HOME="${XDG_DATA_HOME:-$HOME/.local/share}/intelli-shell"
    fi
    ;;

  Darwin*)
    os_slug="apple-darwin"
    archive_ext="tar.gz"
    if [ -z "$INTELLI_HOME" ]; then
      INTELLI_HOME="$HOME/Library/Application Support/org.IntelliShell.Intelli-Shell"
    fi
    ;;

  MSYS*|MINGW*|CYGWIN*)
    os_slug="pc-windows-msvc"
    archive_ext="zip"
    if [ -z "$INTELLI_HOME" ]; then
      if [ -n "$APPDATA" ]; then
        # Convert Windows %APPDATA% path to a POSIX-like path for use in sh
        POSIX_APPDATA=$(echo "/$APPDATA" | sed 's/\\/\//g' | sed 's/://')
        INTELLI_HOME="$POSIX_APPDATA/IntelliShell/Intelli-Shell/data"
      else
        echo "Error: Cannot determine default install location on Windows." >&2
        echo "Reason: Neither INTELLI_HOME nor APPDATA environment variables are set." >&2
        echo "Please set the INTELLI_HOME variable to your desired installation path before running the script." >&2
        exit 1
      fi
    fi
    ;;

  *)
    echo "Error: OS type not supported: $os_type" >&2
    exit 1
    ;;
esac

# Construct the final target string and artifact filename
target="$arch-$os_slug"
artifact_filename="intelli-shell-$target.$archive_ext"

# --- Download and Extract Latest Release ---

# Ensure the target directory exists
mkdir -p "$INTELLI_HOME/bin"

# Define the download URL and a temporary file path
DOWNLOAD_URL="https://github.com/lasantosr/intelli-shell/releases/latest/download/$artifact_filename"
TMP_ARCHIVE="$INTELLI_HOME/$artifact_filename.tmp"

echo "Downloading IntelliShell ($artifact_filename) ..."
if ! curl -Lsf "$DOWNLOAD_URL" -o "$TMP_ARCHIVE"; then
  echo "Error: Download failed from $DOWNLOAD_URL" >&2
  rm -f "$TMP_ARCHIVE"
  exit 1
fi

echo "Extracting ..."
if [ "$archive_ext" = "zip" ]; then
  # Use unzip for Windows zip files
  if ! unzip -oq "$TMP_ARCHIVE" -d "$INTELLI_HOME/bin"; then
    echo "Error: Extraction failed for $TMP_ARCHIVE using unzip" >&2
    rm -f "$TMP_ARCHIVE"
    exit 1
  fi
else
  # Use tar for tar.gz files
  if ! tar zxf "$TMP_ARCHIVE" -C "$INTELLI_HOME/bin"; then
    echo "Error: Extraction failed for $TMP_ARCHIVE using tar" >&2
    rm -f "$TMP_ARCHIVE"
    exit 1
  fi
fi

# Clean up the temporary archive
rm -f "$TMP_ARCHIVE"

# Ensure the main binary is executable
if [ -f "$INTELLI_HOME/bin/intelli-shell" ]; then
    chmod +x "$INTELLI_HOME/bin/intelli-shell"
fi

echo "Successfully installed IntelliShell at: $INTELLI_HOME"

# --- Update Shell Profiles (Optional) ---

if [ "${INTELLI_SKIP_PROFILE:-0}" = "0" ]; then

  # Use a string to keep track of modified files
  updated_files=""

  # Function to add IntelliShell config to a given profile file if not already present
  update_rc() {
    profile_file="$1"
    shell_type="$2" # 'bash', 'zsh', 'fish', or 'powershell'

    # Check if the file exists
    if [ ! -f "$profile_file" ]; then
      # If it's a fish config and doesn't exist, create the directory
      if [ "$shell_type" = "fish" ] && [ ! -d "$HOME/.config/fish" ]; then
        mkdir -p "$HOME/.config/fish"
      fi
      # Create the file if it doesn't exist
      touch "$profile_file"
    fi

    # Check if IntelliShell is already mentioned
    is_sourced=""
    if [ -f "$profile_file" ] && grep -q 'intelli-shell' "$profile_file"; then
      is_sourced="yes"
    fi

    # If not already sourced, add the necessary lines
    if [ -z "$is_sourced" ]; then
      echo "Updating $profile_file ..."
      updated_files="$updated_files $profile_file"

      if [ "$shell_type" = "fish" ]; then
        printf '\n# IntelliShell\n' >> "$profile_file"
        printf 'set -gx INTELLI_HOME "%s"\n' "$INTELLI_HOME" >> "$profile_file"
        printf '# set -gx INTELLI_SEARCH_HOTKEY \\c@\n' >> "$profile_file"
        printf '# set -gx INTELLI_VARIABLE_HOTKEY \\cl\n' >> "$profile_file"
        printf '# set -gx INTELLI_BOOKMARK_HOTKEY \\cb\n' >> "$profile_file"
        printf '# set -gx INTELLI_SKIP_ESC_BIND 0\n' >> "$profile_file"
        printf '# alias is="intelli-shell"\n' >> "$profile_file"
        printf 'fish_add_path "$INTELLI_HOME/bin"\n' >> "$profile_file"
        printf 'intelli-shell init fish | source\n' >> "$profile_file"
      elif [ "$shell_type" = "powershell" ]; then
        printf '\r\n# IntelliShell\r\n' >> "$profile_file"
        printf '$env:INTELLI_HOME = "%s"\r\n' "$INTELLI_HOME" >> "$profile_file"
        printf '# $env:INTELLI_SEARCH_HOTKEY = "Ctrl+Spacebar"\r\n' >> "$profile_file"
        printf '# $env:INTELLI_VARIABLE_HOTKEY = "Ctrl+l"\r\n' >> "$profile_file"
        printf '# $env:INTELLI_BOOKMARK_HOTKEY = "Ctrl+b"\r\n' >> "$profile_file"
        printf '# Set-Alias -Name "is" -Value "intelli-shell"\r\n' >> "$profile_file"
        printf 'iex ((intelli-shell.exe init powershell) -join "`n")\r\n' >> "$profile_file"
      else # bash, zsh
        printf '\n# IntelliShell\n' >> "$profile_file"
        printf 'export INTELLI_HOME="%s"\n' "$INTELLI_HOME" >> "$profile_file"
        printf '# export INTELLI_SEARCH_HOTKEY=\\\\C-@\n' >> "$profile_file"
        printf '# export INTELLI_VARIABLE_HOTKEY=\\\\C-l\n' >> "$profile_file"
        printf '# export INTELLI_BOOKMARK_HOTKEY=\\\\C-b\n' >> "$profile_file"
        printf '# export INTELLI_SKIP_ESC_BIND=0\n' >> "$profile_file"
        printf '# alias is="intelli-shell"\n' >> "$profile_file"
        printf 'export PATH="$INTELLI_HOME/bin:$PATH"\n' >> "$profile_file"
        printf 'eval "$(intelli-shell init %s)"\n' "$shell_type" >> "$profile_file"
      fi
    fi
  }

  # Check for PowerShell Profile (Windows-only)
  case "$os_type" in
    MSYS*|MINGW*|CYGWIN*)
      # Convert the POSIX path in INTELLI_HOME to a native Windows path (e.g., C:\Users\...)
      drive_letter=$(echo "$INTELLI_HOME" | sed -n 's,^/\(.\)/.*,\1,p')
      drive_letter_upper=$(echo "$drive_letter" | tr 'a-z' 'A-Z')
      path_without_drive=$(echo "$INTELLI_HOME" | sed 's,^/./,,')
      path_win_slashes=$(echo "$path_without_drive" | sed 's|/|\\|g')
      INTELLI_HOME_WIN="$drive_letter_upper:\\$path_win_slashes"
      pwsh_command=$(printf '
        $binPath = "%s\\bin";
        $currentUserPath = [System.Environment]::GetEnvironmentVariable("PATH", [System.EnvironmentVariableTarget]::User)
        $pathItems = $currentUserPath -split [System.IO.Path]::PathSeparator
        if ($binPath -notin $pathItems) {
          $newPath = ($pathItems + $binPath) -join [System.IO.Path]::PathSeparator
          [System.Environment]::SetEnvironmentVariable("PATH", $newPath, [System.EnvironmentVariableTarget]::User)
        }
      ' "$INTELLI_HOME_WIN")
      if ! powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$pwsh_command"; then
          echo "Warning: You may need to add $INTELLI_HOME_WIN\bin to your Windows PATH manually" >&2
      fi
      pwsh_profile_win_path=$(powershell.exe -NoProfile -Command '$PROFILE.CurrentUserCurrentHost')
      if [ -n "$pwsh_profile_win_path" ]; then
        # Convert Windows path (C:\Users\...) to a POSIX path (/c/Users/...) that sh can use
        pwsh_profile_posix_path=$(echo "$pwsh_profile_win_path" | sed -e 's|\\|/|g' -e 's|^ *\([A-Za-z]\):|/\L\1|' | tr -d '\r')
        update_rc "$pwsh_profile_posix_path" "powershell"
      else
        echo "Warning: Could not determine PowerShell profile path"
      fi
      ;;
  esac
  # Check for .bash_profile or default to .bashrc
  if [ -f "$HOME/.bash_profile" ]; then
    update_rc "$HOME/.bash_profile" "bash"
  else
    update_rc "$HOME/.bashrc" "bash"
  fi
  # Check if zsh likely exists before updating .zshrc
  if [ -x "/bin/zsh" ] || [ -x "/usr/bin/zsh" ]; then
    update_rc "$HOME/.zshrc" "zsh"
  fi
  # Check if fish likely exists before updating fish config
  if [ -x "/bin/fish" ] || [ -x "/usr/bin/fish" ] || [ -x "/usr/local/bin/fish" ]; then
    update_rc "$HOME/.config/fish/config.fish" "fish"
  fi

  # Check if the updated_files string is non-empty
  if [ -n "$updated_files" ]; then
    echo "The following files were updated, you can customize them:$updated_files"
    echo "Please restart your terminal or source the updated files (e.g., 'source ~/.bashrc')."
  fi

else

  # If profile update was skipped, show manual instructions
  echo "Skipped automatic profile updates."
  echo "You may need to add the following lines to your shell profile (e.g., ~/.bashrc):"
  printf '\nexport INTELLI_HOME="%s"\n' "$INTELLI_HOME"
  printf 'export PATH="$INTELLI_HOME/bin:$PATH"\n'
  printf 'eval "$(intelli-shell init bash)"\n\n'
  echo "For Fish shell (e.g., ~/.config/fish/config.fish):"
  printf '\nset -gx INTELLI_HOME "%s"\n' "$INTELLI_HOME"
  printf 'fish_add_path "$INTELLI_HOME/bin"\n'
  printf 'intelli-shell init fish | source\n\n'
  echo "And then restart your terminal or source the updated files"

fi

exit 0
