$ErrorActionPreference = "Stop"
$BinDir = $PSScriptRoot
$Firnc = Join-Path $BinDir "firnc.exe"
$Firnfmt = Join-Path $BinDir "firnfmt.exe"
if (-not $env:FIRNLIB) { $env:FIRNLIB = Join-Path (Split-Path $BinDir -Parent) "lib" }

function Fail($message) {
    [Console]::Error.WriteLine("firn: $message")
    exit 125
}

function Show-Usage {
    @(
        "firn run   [FILE|DIR] [args]   build and run a file or the project in DIR (default: .)",
        "firn build FILE|DIR [-o out]   compile",
        "firn check [FILE|DIR]          parse and type check only",
        "firn test  [FILE...]           run tests (default: tests\*.fi)",
        "firn fmt   [-c|-w] FILE...     format (-c: check only)",
        "firn new   NAME [--lib]        new console project",
        "firn doctor                    show the tool locations"
    ) | ForEach-Object { Write-Output $_ }
}

function Is-Project($path) { (Test-Path -PathType Container $path) -and (Test-Path (Join-Path $path "firn.package")) }

function Invoke-Build($target, $output) {
    if (Is-Project $target) { & $Firnc --package $target -o $output } else { & $Firnc $target -o $output }
    return $LASTEXITCODE
}

function Command-Run($rest) {
    $target = "."
    if ($rest.Count -gt 0 -and (Test-Path $rest[0])) { $target = $rest[0]; $rest = $rest[1..($rest.Count - 1)] }
    elseif ($rest.Count -gt 0 -and -not (Test-Path "firn.package")) { Fail "no such file: $($rest[0])" }
    $out = Join-Path $env:TEMP ("firn-run-" + [IO.Path]::GetFileNameWithoutExtension((Resolve-Path $target).Path) + ".exe")
    if (Is-Project $target) {
        $dir = Join-Path $target ".firn"
        New-Item -ItemType Directory -Force $dir | Out-Null
        $out = Join-Path (Resolve-Path $dir).Path "run.exe"
    }
    if ((Invoke-Build $target $out) -ne 0) { exit 125 }
    & $out @rest
    exit $LASTEXITCODE
}

function Command-Build($rest) {
    if ($rest.Count -lt 1) { Fail "usage: firn build FILE|DIR [-o out]" }
    $target = $rest[0]
    $extra = @()
    if ($rest.Count -gt 1) { $extra = $rest[1..($rest.Count - 1)] }
    if (Is-Project $target) { & $Firnc --package $target @extra } else { & $Firnc $target @extra }
    exit $LASTEXITCODE
}

function Command-Check($rest) {
    $target = "."
    if ($rest.Count -gt 0) { $target = $rest[0] }
    if (Is-Project $target) {
        $manifest = Get-Content (Join-Path $target "firn.package") | Where-Object { $_ -match '^start\s+(.+)$' } | Select-Object -First 1
        if (-not $manifest) { Fail "firn.package has no start" }
        $target = Join-Path $target (($manifest -split '\s+', 2)[1].Trim())
    }
    & $Firnc --emit=fir-raw $target | Out-Null
    exit $LASTEXITCODE
}

function Command-Test($rest) {
    $files = $rest
    if ($files.Count -eq 0) { $files = @(Get-ChildItem tests\*.fi -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }) }
    if ($files.Count -eq 0) { Fail "no test files (tests\*.fi)" }
    $status = 0
    foreach ($file in $files) { & $Firnc --test --format=tap $file; if ($LASTEXITCODE -ne 0) { $status = 1 } }
    exit $status
}

function Write-Utf8($path, $text) {
    $full = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($path)
    [IO.File]::WriteAllText($full, $text, (New-Object Text.UTF8Encoding($false)))
}

function Command-New($rest) {
    $name = ($rest | Where-Object { $_ -notlike "--*" } | Select-Object -First 1)
    $lib = $rest -contains "--lib"
    if (-not $name) { Fail "usage: firn new NAME [--lib]" }
    if ($name -notmatch '^[A-Za-z_]\w*$') { Fail "package name must start with a letter or _" }
    if (Test-Path $name) { Fail "'$name' already exists" }
    New-Item -ItemType Directory -Force (Join-Path $name "src"), (Join-Path $name "tests") | Out-Null
    if ($lib) {
        Write-Utf8 (Join-Path $name "firn.package") "package  $name`nversion  0.1.0`nsource   src`n"
        Write-Utf8 (Join-Path $name "src\$name.fi") "fn add(a: i32, b: i32) -> i32 {`n    return a + b`n}`n"
    } else {
        Write-Utf8 (Join-Path $name "firn.package") "package  $name`nversion  0.1.0`nstart    src/main.fi`nsource   src`n"
        Write-Utf8 (Join-Path $name "src\main.fi") "fn main() {`n    println(`"hi`")`n}`n"
    }
    Write-Utf8 (Join-Path $name "tests\basic.fi") "import std.testkit`n`n#[test]`nfn it_adds() {`n    testkit.assert_eq_i64(1 + 1, 2)`n}`n"
    Write-Utf8 (Join-Path $name ".gitignore") "/.firn/`n*.exe`n"
    Write-Output "created $name\ -- cd $name; firn run"
}

$sub = if ($args.Count -gt 0) { $args[0] } else { "help" }
$rest = if ($args.Count -gt 1) { @($args[1..($args.Count - 1)]) } else { @() }
if (-not (Test-Path $Firnc)) { Fail "firnc.exe not found next to firn.ps1" }
switch ($sub) {
    "run" { Command-Run $rest }
    "build" { Command-Build $rest }
    "check" { Command-Check $rest }
    "test" { Command-Test $rest }
    "fmt" { & $Firnfmt @rest; exit $LASTEXITCODE }
    "new" { Command-New $rest }
    "doctor" {
        Write-Output "firnc  $Firnc"
        Write-Output "lib    $env:FIRNLIB"
        Write-Output "fmt    $Firnfmt"
    }
    default { Show-Usage }
}
