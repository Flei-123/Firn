# Prints one RESULT line per PresentMon CSV (first 10 s dropped) and every METRICS line of the run logs.
# Aggregate on the server: tools/mc/perf/laptop/mcvan_agg.py (median of the repetitions).
$out = Join-Path $env:TEMP 'mcvan\out'
foreach ($f in (Get-ChildItem $out -Filter 'pm_*.csv' | Sort-Object Name)) { & (Join-Path $PSScriptRoot 'mcvan_stats.ps1') -Csv $f.FullName -Skip 10 }
foreach ($l in (Get-ChildItem $out -Filter 'run_*.log')) { Select-String -Path $l.FullName -Pattern 'METRICS' | ForEach-Object { $_.Line } }
