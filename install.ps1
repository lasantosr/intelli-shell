$null = New-Item -Force -Path $env:APPDATA\IntelliShell\Intelli-Shell\bin -Type Directory
Invoke-WebRequest -UseBasicParsing -URI "https://github.com/lasantosr/intelli-shell/releases/latest/download/intelli-shell-x86_64-pc-windows-msvc.zip" -OutFile .\intelli-shell.zip
Expand-Archive -Force -Path intelli-shell.zip -DestinationPath $env:APPDATA\IntelliShell\Intelli-Shell\bin
Remove-Item intelli-shell.zip
$Path = [Environment]::GetEnvironmentVariable("PATH", [EnvironmentVariableTarget]::User) 
if ($Path -NotLike "*IntelliShell*") { 
    $Path = $Path + [IO.Path]::PathSeparator + "$env:APPDATA\IntelliShell\Intelli-Shell\bin"
    [Environment]::SetEnvironmentVariable("Path", $Path, [EnvironmentVariableTarget]::User)
}
$ProfileContent = $null
if (Test-Path -Path $Profile -PathType Leaf) {
    $ProfileContent = Get-Content -Raw $Profile
} else {
    $Parent = Split-Path -parent $Profile
    $null = New-Item -Force -ItemType Directory -Path $Parent
    $null = New-Item -ItemType File -Path $Profile
}
if (($null -eq $ProfileContent) -Or ($ProfileContent -NotLike "*IntelliShell*")) { 
    Add-Content $Profile "`n# IntelliShell"
    Add-Content $Profile "# `$env:INTELLI_SEARCH_HOTKEY = 'Ctrl+Spacebar'"
    Add-Content $Profile "# `$env:INTELLI_BOOKMARK_HOTKEY = 'Ctrl+b'"
    Add-Content $Profile "# `$env:INTELLI_LABEL_HOTKEY = 'Ctrl+l'"
    Add-Content $Profile ". `$env:APPDATA\IntelliShell\Intelli-Shell\bin\intelli-shell.ps1"
}
