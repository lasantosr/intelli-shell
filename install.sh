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
    # Convert the POSIX-style path in INTELLI_HOME to a native Windows path (e.g., C:\Users\...)
    # This is needed for shells like PowerShell and Nushell that run natively on Windows
    drive_letter=$(echo "$INTELLI_HOME" | sed -n 's,^/\(.\)/.*,\1,p')
    drive_letter_upper=$(echo "$drive_letter" | tr 'a-z' 'A-Z')
    path_without_drive=$(echo "$INTELLI_HOME" | sed 's,^/./,,')
    path_win_slashes=$(echo "$path_without_drive" | sed 's|/|\\|g')
    INTELLI_HOME_WIN="$drive_letter_upper:\\$path_win_slashes"
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

# Determine the version to install
target_version=""
if [ -n "$1" ]; then
  target_version="$1"
elif [ -n "$INTELLI_VERSION" ]; then
  target_version="$INTELLI_VERSION"
fi

# Determine the release path part of the URL based on the version
if [ -n "$target_version" ]; then
  # If a version is provided, use it
  VERSION_TAG=$(echo "$target_version" | sed 's/^v//')
  RELEASE_PATH="releases/download/v$VERSION_TAG"
  echo "Downloading IntelliShell v$VERSION_TAG ($artifact_filename) ..."
else
  # Otherwise, download the latest release
  RELEASE_PATH="releases/latest/download"
  echo "No version specified, installing the latest release."
fi

# Define the download URL and a temporary file path
DOWNLOAD_URL="https://github.com/lasantosr/intelli-shell/$RELEASE_PATH/$artifact_filename"
TMP_ARCHIVE="$INTELLI_HOME/$artifact_filename.tmp"

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

  # Use strings to keep track of modified and skipped files
  updated_files=""
  skipped_files=""

  # Function to add IntelliShell config to a given profile file if not already present
  update_rc() {
    profile_file="$1"
    shell_type="$2" # 'bash', 'zsh', 'fish', 'nushell' or 'powershell'

    # Check if the file's parent directory exists, and create it if not
    profile_dir=$(dirname "$profile_file")
    if [ ! -d "$profile_dir" ]; then
      mkdir -p "$profile_dir"
    fi
    # Create the file if it doesn't exist
    if [ ! -f "$profile_file" ]; then
      touch "$profile_file"
    fi

    # Check if IntelliShell is already mentioned
    is_sourced=""
    if [ -f "$profile_file" ] && grep -q 'intelli-shell' "$profile_file"; then
      is_sourced="yes"
    fi

    # If not already sourced, add the necessary lines
    if [ -z "$is_sourced" ]; then
      updated_files=$(printf '%s\n%s' "$updated_files" "$profile_file")

      if [ "$shell_type" = "fish" ]; then
        printf '\n# IntelliShell\n' >> "$profile_file"
        printf 'set -gx INTELLI_HOME "%s"\n' "$INTELLI_HOME" >> "$profile_file"
        printf '# set -gx INTELLI_SEARCH_HOTKEY ctrl-space\n' >> "$profile_file"
        printf '# set -gx INTELLI_VARIABLE_HOTKEY ctrl-l\n' >> "$profile_file"
        printf '# set -gx INTELLI_BOOKMARK_HOTKEY ctrl-b\n' >> "$profile_file"
        printf '# set -gx INTELLI_FIX_HOTKEY ctrl-x\n' >> "$profile_file"
        printf '# set -gx INTELLI_SKIP_ESC_BIND 0\n' >> "$profile_file"
        printf '# alias is="intelli-shell"\n' >> "$profile_file"
        printf 'fish_add_path "$INTELLI_HOME/bin"\n' >> "$profile_file"
        printf 'intelli-shell init fish | source\n' >> "$profile_file"
      elif [ "$shell_type" = "nushell" ]; then
        printf '\n# IntelliShell\n' >> "$profile_file"
        case "$os_type" in
          MSYS*|MINGW*|CYGWIN*)
            printf "\$env.INTELLI_HOME = '%s'\n" "$INTELLI_HOME_WIN" >> "$profile_file"
            ;;
          *)
            printf '$env.INTELLI_HOME = "%s"\n' "$INTELLI_HOME" >> "$profile_file"
            printf '$env.PATH = ($env.PATH | prepend "%s/bin")\n' "$INTELLI_HOME" >> "$profile_file"
            ;;
        esac
        printf '# $env.INTELLI_SEARCH_HOTKEY = "control space"\n' >> "$profile_file"
        printf '# $env.INTELLI_VARIABLE_HOTKEY = "control char_l"\n' >> "$profile_file"
        printf '# $env.INTELLI_BOOKMARK_HOTKEY = "control char_b"\n' >> "$profile_file"
        printf '# $env.INTELLI_FIX_HOTKEY = "control char_x"\n' >> "$profile_file"
        printf '# alias is = intelli-shell\n' >> "$profile_file"
        printf 'mkdir ($nu.data-dir | path join "vendor/autoload")\n' >> "$profile_file"
        printf 'intelli-shell init nushell | save -f ($nu.data-dir | path join "vendor/autoload/intelli-shell.nu")\n' >> "$profile_file"
      elif [ "$shell_type" = "powershell" ]; then
        case "$os_type" in
          MSYS*|MINGW*|CYGWIN*)
            printf '\r\n# IntelliShell\r\n' >> "$profile_file"
            printf '$env:INTELLI_HOME = "%s"\r\n' "$INTELLI_HOME_WIN" >> "$profile_file"
            printf '# $env:INTELLI_SEARCH_HOTKEY = "Ctrl+Spacebar"\r\n' >> "$profile_file"
            printf '# $env:INTELLI_VARIABLE_HOTKEY = "Ctrl+l"\r\n' >> "$profile_file"
            printf '# $env:INTELLI_BOOKMARK_HOTKEY = "Ctrl+b"\r\n' >> "$profile_file"
            printf '# $env:INTELLI_FIX_HOTKEY = "Ctrl+x"\r\n' >> "$profile_file"
            printf '# Set-Alias -Name "is" -Value "intelli-shell"\r\n' >> "$profile_file"
            printf 'intelli-shell.exe init powershell | Out-String | Invoke-Expression\r\n' >> "$profile_file"
            ;;
          *)
            printf '\n# IntelliShell\n' >> "$profile_file"
            printf '$env:INTELLI_HOME = "%s"\n' "$INTELLI_HOME" >> "$profile_file"
            printf '$env:PATH = "%s/bin:" + $env:PATH\n' "$INTELLI_HOME" >> "$profile_file"
            printf '# $env:INTELLI_SEARCH_HOTKEY = "Ctrl+Spacebar"\n' >> "$profile_file"
            printf '# $env:INTELLI_VARIABLE_HOTKEY = "Ctrl+l"\n' >> "$profile_file"
            printf '# $env:INTELLI_BOOKMARK_HOTKEY = "Ctrl+b"\n' >> "$profile_file"
            printf '# $env:INTELLI_FIX_HOTKEY = "Ctrl+x"\n' >> "$profile_file"
            printf '# Set-Alias -Name "is" -Value "intelli-shell"\n' >> "$profile_file"
            printf 'intelli-shell init powershell | Out-String | Invoke-Expression\n' >> "$profile_file"
            ;;
        esac
      elif [ "$shell_type" = "zsh" ]; then
        printf '\n# IntelliShell\n' >> "$profile_file"
        printf 'export INTELLI_HOME="%s"\n' "$INTELLI_HOME" >> "$profile_file"
        printf "# export INTELLI_SEARCH_HOTKEY='^@'\n" >> "$profile_file"
        printf "# export INTELLI_VARIABLE_HOTKEY='^l'\n" >> "$profile_file"
        printf "# export INTELLI_BOOKMARK_HOTKEY='^b'\n" >> "$profile_file"
        printf "# export INTELLI_FIX_HOTKEY='^x'\n" >> "$profile_file"
        printf '# export INTELLI_SKIP_ESC_BIND=0\n' >> "$profile_file"
        printf '# alias is="intelli-shell"\n' >> "$profile_file"
        printf 'export PATH="$INTELLI_HOME/bin:$PATH"\n' >> "$profile_file"
        printf 'eval "$(intelli-shell init zsh)"\n' >> "$profile_file"
      else # bash
        printf '\n# IntelliShell\n' >> "$profile_file"
        printf 'export INTELLI_HOME="%s"\n' "$INTELLI_HOME" >> "$profile_file"
        printf '# export INTELLI_SEARCH_HOTKEY=\\\\C-@\n' >> "$profile_file"
        printf '# export INTELLI_VARIABLE_HOTKEY=\\\\C-l\n' >> "$profile_file"
        printf '# export INTELLI_BOOKMARK_HOTKEY=\\\\C-b\n' >> "$profile_file"
        printf '# export INTELLI_FIX_HOTKEY=\\\\C-x\n' >> "$profile_file"
        printf '# export INTELLI_SKIP_ESC_BIND=0\n' >> "$profile_file"
        printf '# alias is="intelli-shell"\n' >> "$profile_file"
        printf 'export PATH="$INTELLI_HOME/bin:$PATH"\n' >> "$profile_file"
        printf 'eval "$(intelli-shell init %s)"\n' "$shell_type" >> "$profile_file"
      fi
    else
      # If the file is already configured, add it to the skipped list
      skipped_files=$(printf '%s\n%s' "$skipped_files" "$profile_file")
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
  if command -v zsh >/dev/null 2>&1; then
    update_rc "$HOME/.zshrc" "zsh"
  fi
  # Check if fish likely exists before updating fish config
  if command -v fish >/dev/null 2>&1; then
    update_rc "$HOME/.config/fish/config.fish" "fish"
  fi
  # Check if nu likely exists before updating nu config
  if command -v nu >/dev/null 2>&1; then
    # Query nushell for its config file path to be robust across versions and platforms
    nu_config_path_native=$(nu -c '$nu.config-path' 2>/dev/null | tr -d '\r')
    if [ -n "$nu_config_path_native" ]; then
      nu_config_path_posix="$nu_config_path_native"
      # If on Windows, convert the native path (C:\...) to a POSIX path (/c/...) for the sh script to use
      case "$os_type" in
        MSYS*|MINGW*|CYGWIN*)
          nu_config_path_posix=$(echo "$nu_config_path_native" | sed -e 's|\\|/|g' -e 's|^ *\([A-Za-z]\):|/\L\1|')
          ;;
      esac
      update_rc "$nu_config_path_posix" "nushell"
    fi
    # Check for PowerShell (pwsh) on Linux and macOS
    if [ "$os_type" != "MSYS*" ] && [ "$os_type" != "MINGW*" ] && [ "$os_type" != "CYGWIN*" ]; then
      if command -v pwsh >/dev/null 2>&1; then
        # Query pwsh for its profile path
        pwsh_profile_path=$(pwsh -NoProfile -Command '$PROFILE.CurrentUserCurrentHost' 2>/dev/null | tr -d '\r')
        if [ -n "$pwsh_profile_path" ]; then
          update_rc "$pwsh_profile_path" "powershell"
        fi
      fi
    fi
  fi

  # Trim leading newlines that might exist from the printf construction
  updated_files=$(echo "$updated_files" | sed '/^$/d')
  skipped_files=$(echo "$skipped_files" | sed '/^$/d')

  # Check if any action was taken (either update or skip)
  if [ -n "$updated_files" ] || [ -n "$skipped_files" ]; then
    echo ""

    # Report skipped files, if any
    if [ -n "$skipped_files" ]; then
      echo "The following files already contain IntelliShell configuration and were skipped:"
      old_ifs="$IFS"
      IFS='
'
      for file in $skipped_files; do
        if [ -n "$file" ]; then
          echo "  - $file"
        fi
      done
      IFS="$old_ifs"
      
      echo ""
    fi

    # Report successfully updated files, if any
    if [ -n "$updated_files" ]; then
      echo "Configuration successfully added to the following files:"
      old_ifs="$IFS"
      IFS='
'
      for file in $updated_files; do
        if [ -n "$file" ]; then
          echo "  - $file"
        fi
      done
      IFS="$old_ifs"

      echo ""
      echo "Please restart your terminal for the changes to take effect."
    fi

    echo "If you use a shell that wasn't listed above, you will need to configure it manually."
  else
    # This block now only runs if no config files were found at all
    echo ""
    echo "Could not find a shell profile to update automatically."
    echo "To complete the setup, please add the following lines to your shell's configuration file:"
    
    echo ""
    echo "--- For bash or zsh (in ~/.bashrc or ~/.zshrc) ---"
    printf 'export INTELLI_HOME="%s"\n' "$INTELLI_HOME"
    printf 'export PATH="$INTELLI_HOME/bin:$PATH"\n'
    printf 'eval "$(intelli-shell init bash)" # Or "zsh"\n'
    
    echo ""
    echo "--- For Fish shell (in ~/.config/fish/config.fish) ---"
    printf 'set -gx INTELLI_HOME="%s"\n' "$INTELLI_HOME"
    printf 'fish_add_path "$INTELLI_HOME/bin"\n'
    printf 'intelli-shell init fish | source\n'
    
    echo ""
    echo "--- For Nushell (in ~/.config/nushell/config.nu) ---"
    printf '$env.INTELLI_HOME = "%s"\n' "$INTELLI_HOME"
    printf '$env.PATH = ($env.PATH | prepend "%s/bin")\n' "$INTELLI_HOME"
    printf 'mkdir ($nu.data-dir | path join "vendor/autoload")\n'
    printf 'intelli-shell init nushell | save -f ($nu.data-dir | path join "vendor/autoload/intelli-shell.nu")\n'
    
    echo ""
    echo "After saving the file, please restart your terminal."
  fi

else

  # If profile update was skipped, show manual instructions
  echo "Skipped automatic profile update as requested."
  echo "To complete the setup, please add the following lines to your shell's configuration file:"

  echo ""
  echo "--- For bash or zsh (in ~/.bashrc or ~/.zshrc) ---"
  printf 'export INTELLI_HOME="%s"\n' "$INTELLI_HOME"
  printf 'export PATH="$INTELLI_HOME/bin:$PATH"\n'
  printf 'eval "$(intelli-shell init bash)" # Or "zsh"\n'
  
  echo ""
  echo "--- For Fish shell (in ~/.config/fish/config.fish) ---"
  printf 'set -gx INTELLI_HOME="%s"\n' "$INTELLI_HOME"
  printf 'fish_add_path "$INTELLI_HOME/bin"\n'
  printf 'intelli-shell init fish | source\n'
  
  echo ""
  echo "--- For Nushell (in ~/.config/nushell/config.nu) ---"
  printf '$env.INTELLI_HOME = "%s"\n' "$INTELLI_HOME"
  printf '$env.PATH = ($env.PATH | prepend "%s/bin")\n' "$INTELLI_HOME"
  printf 'mkdir ($nu.data-dir | path join "vendor/autoload")\n'
  printf 'intelli-shell init nushell | save -f ($nu.data-dir | path join "vendor/autoload/intelli-shell.nu")\n'
  
  echo ""
  echo "After saving the file, please restart your terminal."

fi

exit 0
