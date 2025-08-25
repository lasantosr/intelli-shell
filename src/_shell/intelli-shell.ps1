# --- PowerShell Integration ---

#Requires -Modules PSReadLine

# Set strict mode
Set-StrictMode -Version Latest

# Ensure PSReadLine module is available
if (-not (Get-Module -Name PSReadLine -ListAvailable)) {
  Write-Warning "PSReadLine module not found. IntelliShell key bindings require PSReadLine."
  return
}
# Import the module if it's not already loaded
Import-Module PSReadLine -ErrorAction SilentlyContinue

# --- Configuration ---
$IntelliSearchChord = if ([string]::IsNullOrEmpty($env:INTELLI_SEARCH_HOTKEY)) { 'Ctrl+Spacebar' } else { $env:INTELLI_SEARCH_HOTKEY }
$IntelliBookmarkChord = if ([string]::IsNullOrEmpty($env:INTELLI_BOOKMARK_HOTKEY)) { 'Ctrl+b' } else { $env:INTELLI_BOOKMARK_HOTKEY }
$IntelliVariableChord = if ([string]::IsNullOrEmpty($env:INTELLI_VARIABLE_HOTKEY)) { 'Ctrl+l' } else { $env:INTELLI_VARIABLE_HOTKEY }
$IntelliFixChord = if ([string]::IsNullOrEmpty($env:INTELLI_FIX_HOTKEY)) { 'Ctrl+x' } else { $env:INTELLI_FIX_HOTKEY }

# Encapsulates the logic for running intelli-shell and updating the buffer
function Invoke-IntelliShellAction {
  param(
    [Parameter(Mandatory=$true)]
    [string]$Subcommand, # The intelli-shell subcommand (e.g., search, new, replace)

    [Parameter(Mandatory=$false)]
    [string[]]$Args # Array of arguments to pass to intelli-shell.exe after the subcommand
  )
  $executePrefix = "____execute____"

  # Define the executable name (assuming it's in PATH)
  $exeName = 'intelli-shell.exe'

  # Escape arguments
  $processedArgs = @()
  if ($null -ne $Args) {
    $processedArgs = $Args | ForEach-Object { Escape-ArgumentForCommandLine -Argument $_ }
  }

  # Create a temporary file for the output 
  $stdoutTempFilePath = $null
  try {
    $stdoutTempFilePath = [System.IO.Path]::GetTempFileName()
    
    # Construct the full argument list for intelli-shell.exe
    $fullArgumentList = (@('--extra-line', '--skip-execution', '--file-output', $stdoutTempFilePath, $Subcommand) + $processedArgs) -join ' '

    Write-Verbose "Starting process: $exeName $fullArgumentList"

    # Clear the current line in the buffer first
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::BeginningOfLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::KillLine()

    # Execute intelli-shell.exe directly
    $process = Start-Process -FilePath $exeName `
      -ArgumentList $fullArgumentList `
      -Wait `
      -NoNewWindow `
      -PassThru 

    # Check the exit code of the intelli-shell.exe process
    if ($null -eq $process -or $process.ExitCode -ne 0) {
      # When it fails it might have already printed an error message to the console
      # We just need to redraw a new prompt
      [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
      return
    }

    # This ensures the prompt is redrawn correctly after any output IntelliShell might have produced
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert('')
    $promptString = (prompt) -join ''
    Write-Host -NoNewline "`r`e[2K"
    Write-Host -NoNewline $promptString

    # Read the output from the temporary stdout file
    $intelliOutput = Get-Content -Path $stdoutTempFilePath -Raw -ErrorAction SilentlyContinue

    if (-not $?) { # Check if Get-Content for stdout failed
      Display-ErrorMessage -Message "Failed to read IntelliShell stdout from '$stdoutTempFilePath'."
      return
    }

    # Check if the output starts with the special execution prefix
    if ($intelliOutput -and $intelliOutput.StartsWith($executePrefix, [System.StringComparison]::Ordinal)) {
      # If it does, strip the prefix from the output
      $commandToRun = $intelliOutput.Substring($executePrefix.Length)
      
      # Insert the command into the PSReadLine buffer
      [Microsoft.PowerShell.PSConsoleReadLine]::Insert($commandToRun)
      
      # And execute it immediately
      [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
    }
    else {
      # Otherwise, just update the line with the output
      if (-not [string]::IsNullOrWhiteSpace($intelliOutput)) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Insert($intelliOutput)
      }
    }
  } catch {
    Display-ErrorMessage -Message "An error occurred during IntelliShell action: $_"
  } finally {
    # Clean up temporary file
    if ($null -ne $stdoutTempFilePath -and (Test-Path $stdoutTempFilePath)) {
      Write-Verbose "Removing temporary stdout file: $stdoutTempFilePath"
      Remove-Item $stdoutTempFilePath -Force -ErrorAction SilentlyContinue
    }
  }
}

# Escapes an argument for use in a native process command line
function Escape-ArgumentForCommandLine {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)]
    [AllowEmptyString()]
    [string]$Argument
  )

  # If the argument is empty, it must be represented as empty double quotes
  if ([string]::IsNullOrEmpty($Argument)) {
    return '""'
  }

  # If the argument contains no special characters, it can be passed as-is without quotes
  if (-not ($Argument -match '[\s"]')) {
    return $Argument
  }

  # This regex-based replacement handles backslashes and quotes:
  # 1. It doubles any backslashes that are at the very end of the string
  # 2. It doubles any backslashes that are followed by a double quote, and then escapes that quote
  $escaped = [regex]::Replace($Argument, '(\\+)$', '$1$1')
  $escaped = [regex]::Replace($escaped, '(\\*)"', '$1$1\"')

  # Finally, wrap the entire escaped string in double quotes
  return '"' + $escaped + '"'
}

# Displays a warning message on a popup
function Display-ErrorMessage {
  param(
    [Parameter(Mandatory=$true)]
    [string]$Message
  )
  
  $wshell = New-Object -ComObject WScript.Shell
  $wshell.Popup($Message, 0, "IntelliShell Warning", 48) | Out-Null
}

# --- Key Handler Definitions ---

Write-Verbose "Setting IntelliShell PSReadLine key handlers..."

# Search Handler
Set-PSReadLineKeyHandler -Chord $IntelliSearchChord -BriefDescription "IntelliShell Search" -Description "Searches for a bookmarked command based on current line" -ScriptBlock {
  $line = $null
  $cursor = $null
  [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

  # Set command arguments and execute it
  $args = @('-i')
  if (-not [string]::IsNullOrWhiteSpace($line)) {
    $args += $line
  }
  Invoke-IntelliShellAction -Subcommand 'search' -Args $args
}

# Bookmark Handler
Set-PSReadLineKeyHandler -Chord $IntelliBookmarkChord -BriefDescription "IntelliShell Bookmark" -Description "Bookmarks current command line (or opens new bookmark)" -ScriptBlock {
  $line = $null
  $cursor = $null
  [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

  # Set command arguments and execute it
  $args = @('-i')
  if (-not [string]::IsNullOrWhiteSpace($line)) {
    $args += $line
  }
  Invoke-IntelliShellAction -Subcommand 'new' -Args $args
}

# Variable Replacement Handler
Set-PSReadLineKeyHandler -Chord $IntelliVariableChord -BriefDescription "IntelliShell Variable Replacement" -Description "Triggers variable replacement for current command line" -ScriptBlock {
  $line = $null
  $cursor = $null
  [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

  # Set command arguments and execute it
  $args = @('-i')
  if (-not [string]::IsNullOrWhiteSpace($line)) {
    $args += $line
  }
  Invoke-IntelliShellAction -Subcommand 'replace' -Args $args
}

# Fix Command Handler
Set-PSReadLineKeyHandler -Chord $IntelliFixChord -BriefDescription "IntelliShell Fix" -Description "Fixes the current command line" -ScriptBlock {
  $line = $null
  $cursor = $null
  [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

  # Initialize an empty array for arguments
  $args = @()

  # Safely get the last commands
  $historyLines = @()
  $historyObjects = Get-History -Count 5 -ErrorAction SilentlyContinue
  if ($null -ne $historyObjects) {
    $historyLines = $historyObjects.CommandLine
  }

  # Only add the history argument if history lines were found
  if ($historyLines.Count -gt 0) {
    $historyString = $historyLines -join "`n"
    $args += @('--history', $historyString)
  }

  # Add the current line if it's not empty
  if (-not [string]::IsNullOrWhiteSpace($line)) {
    $args += $line
  }
  
  # Call the main action function
  Invoke-IntelliShellAction -Subcommand 'fix' -Args $args
}

# Export the execution prompt variable
$env:INTELLI_EXEC_PROMPT = ">> "

Write-Verbose "IntelliShell PSReadLine key handlers set."
