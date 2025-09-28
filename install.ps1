#Requires -Modules Microsoft.PowerShell.Archive, Microsoft.PowerShell.Utility
<#
.SYNOPSIS
Installs the IntelliShell tool for PowerShell and Nushell on Windows.
.DESCRIPTION
Downloads the latest release of IntelliShell, extracts it, adds the binary to the
user's PATH, and updates both the PowerShell and Nushell profiles if they exist.
.NOTES
File Name: install.ps1
Author   : Luis Santos
Version  : 1.2
#>

param(
  # Specifies the version of IntelliShell to install (e.g., "v0.5.0" or "0.5.0").
  # If not provided, the latest version will be installed.
  [string]$Version
)

# Set strict mode
Set-StrictMode -Version Latest

# --- Configuration ---
$AppName = "IntelliShell"

# --- Determine Architecture ---
$architecture = $env:PROCESSOR_ARCHITECTURE
switch ($architecture) {
  'AMD64' { $targetArch = 'x86_64' }
  'ARM64' { $targetArch = 'aarch64' }
  default {
    Write-Error "Unsupported processor architecture: $architecture"
    return
  }
}
$osSlug = "pc-windows-msvc"
$target = "$targetArch-$osSlug"
$archiveExtension = "zip"
$artifactFilename = "intelli-shell-$target.$archiveExtension"

# --- Determine Installation Path (INTELLI_HOME) ---
# Priority: 1. Existing $env:INTELLI_HOME, 2. Default AppData path
if ($env:INTELLI_HOME) {
  $installPath = $env:INTELLI_HOME
} else {
  # Default path within AppData
  $installPath = Join-Path $env:APPDATA "$AppName\Intelli-Shell\data"
  $env:INTELLI_HOME = $installPath
}
$binPath = Join-Path $installPath "bin"

# --- Ensure Target Directory Exists ---
try {
  if (-not (Test-Path -Path $binPath -PathType Container)) {
    $null = New-Item -Path $binPath -ItemType Directory -Force -ErrorAction Stop
  }
} catch {
  Write-Error "Failed to create installation directory '$binPath': $_"
  return
}

# --- Download and Extract ---
$targetVersion = if ($Version) { $Version } else { $env:INTELLI_VERSION }
if ([string]::IsNullOrWhiteSpace($targetVersion)) {
  $releasePath = "releases/latest/download"
  Write-Host "Downloading latest IntelliShell ($artifactFilename) ..."
} else {
  $cleanVersion = $targetVersion.TrimStart('v')
  $releasePath = "releases/download/v$cleanVersion"
  Write-Host "Downloading IntelliShell v$cleanVersion ($artifactFilename) ..."
}
$downloadUrl = "https://github.com/lasantosr/intelli-shell/$releasePath/$artifactFilename"
$tempFile = Join-Path $env:TEMP -ChildPath ([System.Guid]::NewGuid().ToString() + ".$archiveExtension")

try {
  Invoke-WebRequest -Uri $downloadUrl -OutFile $tempFile -UseBasicParsing -TimeoutSec 300 -ErrorAction Stop
} catch {
  Write-Error "An error occurred during download: $_"
  if (Test-Path -Path $tempFile) {
    Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
  }
  return
}

Write-Host "Extracting ..."
try {
  Expand-Archive -Path $tempFile -DestinationPath $binPath -Force -ErrorAction Stop
} catch {
  Write-Error "An error occurred during extraction: $_"
  return
} finally {
  # Clean up the temporary archive in all cases (success or failure)
  if (Test-Path -Path $tempFile) {
    Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
  }
}

Write-Host "Successfully installed $AppName at: $installPath"

# --- Update User PATH Environment Variable ---
try {
  $currentUserPath = [Environment]::GetEnvironmentVariable("PATH", [EnvironmentVariableTarget]::User)
  $pathItems = $currentUserPath -split [IO.Path]::PathSeparator
  if ($binPath -notin $pathItems) {
    $newPath = ($pathItems + $binPath) -join [IO.Path]::PathSeparator
    [Environment]::SetEnvironmentVariable("PATH", $newPath, [EnvironmentVariableTarget]::User)
  }
} catch {
  Write-Warning "Failed to update user PATH environment variable: $_"
  Write-Warning "You may need to add '$binPath' to your PATH manually."
}

# --- Reusable Profile Update Function ---
function Update-Profile {
  param(
    [string]$ProfilePath,
    [string]$ShellType # 'PowerShell' or 'Nushell'
  )

  try {
    # Ensure profile file and its directory exist
    if (-not (Test-Path -Path $ProfilePath -PathType Leaf)) {
      $parentDir = Split-Path -Parent $ProfilePath
      if (-not (Test-Path -Path $parentDir -PathType Container)) {
        $null = New-Item -Path $parentDir -ItemType Directory -Force -ErrorAction Stop
      }
      $null = New-Item -Path $ProfilePath -ItemType File -Force -ErrorAction Stop
    }

    # Read profile content
    $profileContent = Get-Content -Path $ProfilePath -Raw -ErrorAction SilentlyContinue

    # Check if IntelliShell is already mentioned
    if ($profileContent -match [regex]::Escape('intelli-shell')) {
      Write-Host "IntelliShell configuration already found in profile: $ProfilePath"
    } else {
      Write-Host "Updating profile: $ProfilePath"

      $configBlock = if ($ShellType -eq 'PowerShell') {
        @"

# IntelliShell
`$env:INTELLI_HOME = "$($env:INTELLI_HOME)"
# `$env:INTELLI_SEARCH_HOTKEY = 'Ctrl+Spacebar'
# `$env:INTELLI_VARIABLE_HOTKEY = 'Ctrl+l'
# `$env:INTELLI_BOOKMARK_HOTKEY = 'Ctrl+b'
# `$env:INTELLI_FIX_HOTKEY = 'Ctrl+x'
# Set-Alias -Name 'is' -Value 'intelli-shell'
intelli-shell.exe init powershell | Out-String | Invoke-Expression

"@
      } else { # Nushell
        @"

# IntelliShell
`$env.INTELLI_HOME = '$($env:INTELLI_HOME)'
# `$env.INTELLI_SEARCH_HOTKEY = "control space"
# `$env.INTELLI_VARIABLE_HOTKEY = "control char_l"
# `$env.INTELLI_BOOKMARK_HOTKEY = "control char_b"
# `$env.INTELLI_FIX_HOTKEY = "control char_x"
# alias is = intelli-shell
mkdir (`$nu.data-dir | path join "vendor/autoload")
intelli-shell init nushell | save -f (`$nu.data-dir | path join "vendor/autoload/intelli-shell.nu")

"@
      }
      Add-Content -Path $ProfilePath -Value $configBlock
    }
  } catch {
    Write-Warning "Failed to update $ShellType profile '$ProfilePath': $_"
  }
}

# --- Update Shell Profiles (Optional) ---
if ($env:INTELLI_SKIP_PROFILE -eq '1') {
  Write-Host ""
  Write-Host "Skipping automatic profile update as requested."
  Write-Host "The binary path '$($binPath)' has been permanently added to your user PATH."
  Write-Host "To complete the setup, you must manually add the integration lines to your"
  Write-Host "shell's profile file and then restart your terminal."
  Write-Host ""
  Write-Host "--- For PowerShell (in `$Profile`) ---"
  Write-Host "`$env:INTELLI_HOME = `"$($env:INTELLI_HOME)`""
  Write-Host "intelli-shell init powershell | Out-String | Invoke-Expression"
  Write-Host ""

  # Conditionally show Nushell instructions if 'nu' is detected
  $nuCommand = Get-Command nu -ErrorAction SilentlyContinue
  if ($nuCommand) {
    Write-Host "--- For Nushell (in your config.nu) ---"
    Write-Host "`$env:INTELLI_HOME = '$($env:INTELLI_HOME)'"
    Write-Host "mkdir (`$nu.data-dir | path join `"vendor/autoload`")"
    Write-Host "intelli-shell init nushell | save -f (`$nu.data-dir | path join `"vendor/autoload/intelli-shell.nu`")"
    Write-Host ""
  }
} else {
  # Update PowerShell Profile
  Update-Profile -ProfilePath $Profile -ShellType 'PowerShell'

  # Update Nushell Profile if `nu` is found
  $nuCommand = Get-Command nu -ErrorAction SilentlyContinue
  if ($nuCommand) {
    try {
      $nuConfigPath = (nu -c '$nu.config-path').Trim()
      if ($nuConfigPath) {
        Update-Profile -ProfilePath $nuConfigPath -ShellType 'Nushell'
      }
    } catch {
      Write-Warning "Found 'nu' command but could not determine its config path: $_"
    }
  }
  Write-Host "Profile updates complete. Please restart your terminal for changes to take effect."
}
