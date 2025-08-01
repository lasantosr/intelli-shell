#Requires -Modules Microsoft.PowerShell.Archive, Microsoft.PowerShell.Utility
<#
.SYNOPSIS
Installs the IntelliShell tool for PowerShell.
.DESCRIPTION
Downloads the latest release of IntelliShell for the correct architecture,
extracts it to the user's AppData directory (or a custom path specified
by $env:INTELLI_HOME), adds the binary directory to the user's PATH,
and updates the PowerShell profile to source the IntelliShell integration script.
.NOTES
File Name: install.ps1
Author   : Luis Santos
Version  : 1.1
#>

# Set strict mode
Set-StrictMode -Version Latest

# --- Configuration ---
$AppName = "IntelliShell"
$GitHubRepo = "lasantosr/intelli-shell"
$BaseUrl = "https://github.com/$GitHubRepo/releases/latest/download"

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
$downloadUrl = "$BaseUrl/$artifactFilename"

# --- Determine Installation Path (INTELLI_HOME) ---
# Priority: 1. Existing $env:INTELLI_HOME, 2. Default AppData path
if (-not ([string]::IsNullOrEmpty($env:INTELLI_HOME))) {
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
$tempFile = Join-Path $env:TEMP -ChildPath ([System.Guid]::NewGuid().ToString() + ".$archiveExtension")

Write-Host "Downloading IntelliShell ($artifactFilename) ..."
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

# --- Update PowerShell Profile (Optional) ---

# Check if profile update should be skipped
if ($env:INTELLI_SKIP_PROFILE -eq '1') {
  Write-Host "Skipping profile update because INTELLI_SKIP_PROFILE is set to 1."
  Write-Host "You may need to add the following to your PowerShell profile (`$Profile`):"
  Write-Host "`$env:INTELLI_HOME = `"$($env:INTELLI_HOME)`""
  Write-Host "iex ((intelli-shell init powershell) -join `"``n`")"
  Write-Host "Remember to restart your terminal after modifying the profile."
} else {
  # Proceed with profile update
  try {
    # Ensure profile file and its directory exist
    if (-not (Test-Path -Path $Profile -PathType Leaf)) {
      $parentDir = Split-Path -Parent $Profile
      if (-not (Test-Path -Path $parentDir -PathType Container)) {
        $null = New-Item -Path $parentDir -ItemType Directory -Force -ErrorAction Stop
      }
      $null = New-Item -Path $Profile -ItemType File -Force -ErrorAction Stop
    }

    # Read profile content
    $profileContent = Get-Content -Path $Profile -Raw -ErrorAction SilentlyContinue

    # Check if IntelliShell is already mentioned
    if ($profileContent -match [regex]::Escape("intelli-shell")) {
      Write-Host "IntelliShell configuration already found in profile: $Profile"
    } else {
      Write-Host "Updating profile: $Profile"

      Add-Content -Path $Profile -Value ""
      Add-Content -Path $Profile -Value "# IntelliShell"
      Add-Content -Path $Profile -Value "`$env:INTELLI_HOME = `"$($env:INTELLI_HOME)`""
      Add-Content -Path $Profile -Value "# `$env:INTELLI_SEARCH_HOTKEY = 'Ctrl+Spacebar'"
      Add-Content -Path $Profile -Value "# `$env:INTELLI_BOOKMARK_HOTKEY = 'Ctrl+b'"
      Add-Content -Path $Profile -Value "# `$env:INTELLI_VARIABLE_HOTKEY = 'Ctrl+l'"
      Add-Content -Path $Profile -Value "iex ((intelli-shell.exe init powershell) -join `"``n`")"
      Add-Content -Path $Profile -Value ""

      Write-Host "Please close this terminal and open a new one for changes to take effect."
    }
  } catch {
    Write-Warning "Failed to update PowerShell profile '$Profile': $_"
    Write-Warning "You may need to add the IntelliShell configuration manually."
  }
}
