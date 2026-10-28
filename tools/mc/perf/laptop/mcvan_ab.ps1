param([string]$Specs, [string]$Label = 'ab', [int]$Secs = 30, [int]$Settle = 22)
$out = Join-Path $env:TEMP 'mcvan\out'
$sum = Join-Path $out "sum_$Label.txt"
$run = 'C:\Users\Public\mcvan_run.ps1'
$stats = 'C:\Users\Public\mcvan_stats.ps1'
function Log($m) { ("{0} {1}" -f (Get-Date -Format 'HH:mm:ss'), $m) | Add-Content $sum }
foreach ($spec in ($Specs -split '#')) {
  $f = $spec -split '\|'
  $tag = $f[0]; $exe = $f[1]; $gfx = $f[2]; $r = $f[3]
  $saved = @{}
  if ($f.Count -gt 4 -and $f[4]) {
    foreach ($kv in ($f[4] -split '\+')) {
      $k, $v = $kv -split '=', 2
      $saved[$k] = [Environment]::GetEnvironmentVariable($k)
      [Environment]::SetEnvironmentVariable($k, $v)
    }
  }
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $run, '-Target', 'mcplay', '-Radius', "$r", '-Tag', $tag, '-Secs', "$Secs", '-Settle', "$Settle", '-Gfx', $gfx, '-Fps', '-1', '-Exe', $exe)
  Log "START tag=$tag exe=$exe gfx=$gfx r=$r env=$($f[4])"
  $p = Start-Process powershell -ArgumentList $a -PassThru -WindowStyle Hidden
  $p.WaitForExit(240000) | Out-Null
  if (-not $p.HasExited) { Log "TIMEOUT tag=$tag"; Stop-Process -Id $p.Id -Force }
  foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
  Get-ChildItem $out -Filter "pm_*_r${r}_${tag}.csv" | % { Log (& powershell -NoProfile -ExecutionPolicy Bypass -File $stats -Csv $_.FullName -Skip 5) }
  Get-ChildItem $out -Filter "run_*_${tag}*.log" | % { Select-String -Path $_.FullName -Pattern 'METRICS|joined=|window client' | % { Log ("  " + $_.Line) } }
  Get-ChildItem $out -Filter "mcplay_*_r${r}_${tag}.out" | % { Select-String -Path $_.FullName -Pattern 'profile phase|workers=|api=|present=' | % { Log ("  " + $_.Line) } }
}
Log 'ALLDONE'
New-Item -ItemType File -Force (Join-Path $out "ALLDONE_$Label") | Out-Null
