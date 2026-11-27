param(
    [string]$Destination = (Join-Path $env:LOCALAPPDATA "firn"),
    [switch]$SkipExtension
)
$ErrorActionPreference = "Stop"
$Source = $PSScriptRoot

function Add-ToUserPath($directory) {
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = @()
    if ($current) { $entries = $current -split ";" | Where-Object { $_ } }
    if ($entries -notcontains $directory) {
        [Environment]::SetEnvironmentVariable("Path", (($entries + $directory) -join ";"), "User")
        Write-Output "added to PATH: $directory"
    }
    if (($env:Path -split ";") -notcontains $directory) { $env:Path = "$env:Path;$directory" }
}

Write-Output "installing Firn to $Destination"
New-Item -ItemType Directory -Force $Destination | Out-Null
foreach ($part in "bin", "lib", "licenses") {
    $target = Join-Path $Destination $part
    if (Test-Path $target) { Remove-Item -Recurse -Force $target }
    Copy-Item -Recurse (Join-Path $Source $part) $target
}
Add-ToUserPath (Join-Path $Destination "bin")
[Environment]::SetEnvironmentVariable("FIRNLIB", (Join-Path $Destination "lib"), "User")
$env:FIRNLIB = Join-Path $Destination "lib"

if (-not $SkipExtension) {
    $extension = Get-ChildItem (Join-Path $Source "vscode-firn-*.vsix") | Select-Object -First 1
    $editors = "code", "code-insiders", "codium" | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue }
    if (-not $editors) { Write-Output "VS Code not found on PATH. Install later: code --install-extension `"$($extension.FullName)`"" }
    foreach ($editor in $editors) { & $editor --install-extension $extension.FullName --force }
}

& (Join-Path $Destination "bin\firnc.exe") --version
Write-Output "done. Open a NEW terminal, then: firn new hello; cd hello; firn run"
