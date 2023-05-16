$IntelliSearchChord = if ($null -eq $env:INTELLI_SEARCH_HOTKEY) { 'Ctrl+Spacebar' } else { $env:INTELLI_SEARCH_HOTKEY }
$IntelliBookmarkChord = if ($null -eq $env:INTELLI_BOOKMARK_HOTKEY) { 'Ctrl+b' } else { $env:INTELLI_BOOKMARK_HOTKEY }
$IntelliLabelChord = if ($null -eq $env:INTELLI_LABEL_HOTKEY) { 'Ctrl+l' } else { $env:INTELLI_LABEL_HOTKEY }

Set-PSReadLineKeyHandler -Chord $IntelliSearchChord -BriefDescription "IntelliShell Search" -Description "Searches for a bookmarked command" -ScriptBlock {
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

    $TempFile = New-TemporaryFile
    $line = $line -replace '"','""""""""""""'
    $Command = 'intelli-shell.exe --file-output=""""' + $TempFile.FullName + '"""" search """"' + $line + '""""' 
    Start-Process powershell.exe -Wait -NoNewWindow -ArgumentList "-command", "$Command" -RedirectStandardError "NUL"
    $IntelliOutput = Get-Content -Raw $TempFile
    Remove-Item $TempFile

    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::BeginningOfLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    if (-Not [string]::IsNullOrWhiteSpace($IntelliOutput)) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Insert($IntelliOutput)
    }
}

Set-PSReadLineKeyHandler -Chord $IntelliBookmarkChord -BriefDescription "IntelliShell Bookmark" -Description "Bookmarks current command" -ScriptBlock {
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

    $TempFile = New-TemporaryFile
    $line = $line -replace '"','""""""""""""'
    $Command = 'intelli-shell.exe --file-output=""""' + $TempFile.FullName + '"""" new -c """"' + $line + '""""' 
	if ([string]::IsNullOrWhiteSpace($line)) {
        $Command = 'intelli-shell.exe --file-output=""""' + $TempFile.FullName + '"""" new' 
    }
    Start-Process powershell.exe -Wait -NoNewWindow -ArgumentList "-command", "$Command" -RedirectStandardError "NUL"
    $IntelliOutput = Get-Content -Raw $TempFile
    Remove-Item $TempFile

    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::BeginningOfLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    if (-Not [string]::IsNullOrWhiteSpace($IntelliOutput)) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Insert($IntelliOutput)
    }
}

Set-PSReadLineKeyHandler -Chord $IntelliLabelChord -BriefDescription "IntelliShell Label" -Description "Triggers label replace for current command" -ScriptBlock {
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)

    $TempFile = New-TemporaryFile
    $line = $line -replace '"','""""""""""""'
    $Command = 'intelli-shell.exe --file-output=""""' + $TempFile.FullName + '"""" label """"' + $line + '""""' 
    Start-Process powershell.exe -Wait -NoNewWindow -ArgumentList "-command", "$Command" -RedirectStandardError "NUL"
    $IntelliOutput = Get-Content -Raw $TempFile
    Remove-Item $TempFile

    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::BeginningOfLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    if (-Not [string]::IsNullOrWhiteSpace($IntelliOutput)) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Insert($IntelliOutput)
    }
}
