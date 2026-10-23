# Client spec: vanilla | mcplay:gl | mcplay:vk (uncapped) | mcplay:gl:cap | mcplay:vk:cap (default 60 fps cap).
# Matrix runner (see tools/mc/perf/vanilla_compare.md). Start the server first (mcvan_run.ps1 -Target server -Radius 0 -Tag s),
# warm up once with a vanilla r16 run, then run this. Order: resolution > radius > repetition > client (interleaved to spread drift).
param([string[]]$Res = @('1280x720', '640x360'), [int[]]$Radii = @(8, 16), [int]$Reps = 3, [string[]]$Clients = @('vanilla', 'mcplay:gl', 'mcplay:vk'), [int]$Secs = 45, [int]$Settle = 30, [string]$Exe = 'mcplay-console.exe')
$g = 'C:\Users\Public\mcvan_run.ps1'
foreach ($rs in $Res) {
  $w, $h = $rs -split 'x'
  foreach ($r in $Radii) {
    foreach ($n in 1..$Reps) {
      foreach ($c in $Clients) {
        $t, $gfx, $cap = $c -split ':'; if (-not $gfx) { $gfx = 'gl' }; $fps = if ($cap -eq 'cap') { -1 } else { 0 }
        & powershell -NoProfile -ExecutionPolicy Bypass -File $g -Target $t -Radius $r -Tag "n$n" -Secs $Secs -Settle $Settle -W $w -H $h -Gfx $gfx -Exe $Exe -Fps $fps
        Start-Sleep 2
      }
    }
  }
}
Set-Content (Join-Path $env:TEMP 'mcvan\out\BATCH_DONE') 'done'
