param([string]$Exe = 'mcplay-bench.exe', [int]$Reps = 3, [string]$Radii = '8,16', [string]$Kinds = 'A,V,P', [string]$Label = 'all', [int]$Secs = 30, [int]$Settle = 22)
$out = Join-Path $env:TEMP 'mcvan\out'
$sum = Join-Path $out "sum_$Label.txt"
$run = 'C:\Users\Public\mcvan_run.ps1'
$stats = 'C:\Users\Public\mcvan_stats.ps1'
function Log($m) { ("{0} {1}" -f (Get-Date -Format 'HH:mm:ss'), $m) | Add-Content $sum }
function One($kind, $r, $tag) {
  Remove-Item Env:MC_VANILLA -ErrorAction SilentlyContinue
  Remove-Item Env:MCPLAY_SPIN -ErrorAction SilentlyContinue
  $target = 'mcplay'
  if ($kind -eq 'A') { $env:MC_VANILLA = '0'; $env:MCPLAY_SPIN = '3' }
  if ($kind -eq 'P') { $env:MCPLAY_SPIN = '3' }
  if ($kind -eq 'V') { $target = 'vanilla' }
  $a = @('-NoProfile', '-File', $run, '-Target', $target, '-Radius', "$r", '-Tag', $tag, '-Secs', "$Secs", '-Settle', "$Settle", '-Fps', '0', '-Exe', $Exe)
  Log "START kind=$kind r=$r tag=$tag"
  $p = Start-Process powershell -ArgumentList $a -PassThru -WindowStyle Hidden
  $p.WaitForExit(240000) | Out-Null
  if (-not $p.HasExited) { Log "TIMEOUT kind=$kind r=$r tag=$tag"; Stop-Process -Id $p.Id -Force }
  Get-ChildItem $out -Filter "pm_*_r${r}_${tag}.csv" | % {
    Log ("KIND=$kind " + (& powershell -NoProfile -File $stats -Csv $_.FullName -Skip 5))
  }
  Get-ChildItem $out -Filter "run_*_${tag}*.log" | % { Select-String -Path $_.FullName -Pattern 'METRICS|joined=|window client' | % { Log ("  " + $_.Line) } }
  Get-ChildItem $out -Filter "mcplay_*_r${r}_${tag}.out" | % { Select-String -Path $_.FullName -Pattern 'profile phase' | % { Log ("  " + $_.Line) } }
  Remove-Item Env:MC_VANILLA -ErrorAction SilentlyContinue
  Remove-Item Env:MCPLAY_SPIN -ErrorAction SilentlyContinue
}
if ($Kinds -match 'W') { One 'V' 16 'w0' }
foreach ($rep in 1..$Reps) {
  foreach ($r in ($Radii -split ',')) {
    foreach ($k in ($Kinds -split ',')) { if ($k -ne 'W') { One $k $r "$($k.ToLower())$rep" } }
  }
}
Log 'ALLDONE'
New-Item -ItemType File -Force (Join-Path $out "ALLDONE_$Label") | Out-Null
