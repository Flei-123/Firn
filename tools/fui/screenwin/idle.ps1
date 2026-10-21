param([string]$Exe, [int]$Secs = 8)
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 4
$p.Refresh()
$c0 = $p.TotalProcessorTime.TotalMilliseconds
$t0 = Get-Date
Start-Sleep -Seconds $Secs
$p.Refresh()
$c1 = $p.TotalProcessorTime.TotalMilliseconds
$t1 = Get-Date
$wall = ($t1 - $t0).TotalMilliseconds
$ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
$thr = $p.Threads.Count
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
("{0} idle_cpu_pct={1:N2} cpu_ms={2:N0} wall_ms={3:N0} ws_mb={4} threads={5}" -f ([IO.Path]::GetFileName($Exe)), (100.0 * ($c1 - $c0) / $wall), ($c1 - $c0), $wall, $ws, $thr)
