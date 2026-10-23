# Start the server first (mcvan_run.ps1 -Target server -Radius 0 -Tag s), warm up once with a vanilla r16 run, then:
$g = 'C:\Users\Public\mcvan_run.ps1'
foreach ($r in 8, 16) {
  foreach ($n in 1, 2, 3) {
    foreach ($t in 'vanilla', 'mcplay') {   # interleaved to spread thermal / background drift over both clients
      & powershell -NoProfile -ExecutionPolicy Bypass -File $g -Target $t -Radius $r -Tag "n$n" -Secs 45 -Settle 30
      Start-Sleep 3
    }
  }
}
Set-Content (Join-Path $env:TEMP 'mcvan\out\BATCH_DONE') 'done'
