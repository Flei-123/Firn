param([string]$Target, [int]$Radius, [string]$Tag, [int]$Secs = 45, [int]$Settle = 30, [int]$W = 1280, [int]$H = 720, [string]$Gfx = 'gl', [string]$Exe = 'mcplay-console.exe', [int]$Fps = 0)
# -Fps 0 = uncapped (MCPLAY_FPS=0), -Fps -1 = mcplay default cap (60, real player case), -Fps N = cap N.
# Target = server | vanilla | mcplay. -Gfx gl|vk selects the mcplay back end (Vanilla ignores it). -W/-H = window client size.
# Vanilla 1.20.1 vs mcplay measurement on one Windows laptop (see tools/mc/perf/vanilla_compare.md).
# Target = server (start local vanilla server + rcon, once) | vanilla | mcplay.
# Needs in %TEMP%\mcvan: srv\server.jar (1.20.1), pm\PresentMon.exe (v2.6.0 x64). No accounts/tokens: offline server + offline client.
$ErrorActionPreference = 'Stop'
$base = Join-Path $env:TEMP 'mcvan'
$srv = Join-Path $base 'srv'
$out = Join-Path $base 'out'
New-Item -ItemType Directory -Force $out | Out-Null
$port = 25599
$rconPort = 25575
$java17 = 'C:\Program Files\Java\jdk-17\bin\javaw.exe'
$log = Join-Path $out ("run_{0}_{1}.log" -f $Target, $Tag)
function L($m) { ("{0} {1}" -f (Get-Date -Format 'HH:mm:ss'), $m) | Add-Content -Path $log }

Add-Type @'
using System; using System.Runtime.InteropServices;
public class W32 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
'@

[W32]::SetProcessDPIAware() | Out-Null
# ---- rcon ----
$script:rc = $null
function Rcon-Open {
  $c = New-Object Net.Sockets.TcpClient('127.0.0.1', $rconPort)
  $script:rc = $c.GetStream()
  Rcon-Send 1 3 'mcvanpw' | Out-Null
}
function Rcon-Send([int]$id, [int]$type, [string]$body) {
  $b = [Text.Encoding]::UTF8.GetBytes($body)
  $ms = New-Object IO.MemoryStream
  $w = New-Object IO.BinaryWriter($ms)
  $w.Write([int](10 + $b.Length)); $w.Write([int]$id); $w.Write([int]$type); $w.Write($b); $w.Write([byte]0); $w.Write([byte]0)
  $w.Flush(); $buf = $ms.ToArray()
  $script:rc.Write($buf, 0, $buf.Length)
  $hdr = New-Object byte[] 4; $n = 0
  while ($n -lt 4) { $n += $script:rc.Read($hdr, $n, 4 - $n) }
  $len = [BitConverter]::ToInt32($hdr, 0)
  $rest = New-Object byte[] $len; $n = 0
  while ($n -lt $len) { $n += $script:rc.Read($rest, $n, $len - $n) }
  return [Text.Encoding]::UTF8.GetString($rest, 8, [Math]::Max(0, $len - 10))
}
function Rcon([string]$cmd) { return (Rcon-Send 2 2 $cmd) }

function Join-Count {
  $f = Join-Path $srv 'logs\latest.log'
  if (-not (Test-Path $f)) { return 0 }
  $fs = [IO.File]::Open($f, 'Open', 'Read', 'ReadWrite'); $sr = New-Object IO.StreamReader($fs)
  $t = $sr.ReadToEnd(); $sr.Close()
  return ([regex]::Matches($t, 'joined the game')).Count
}

# ---- server ----
if ($Target -eq 'server') {
  Set-Content -Path (Join-Path $srv 'eula.txt') -Value 'eula=true' -Encoding ASCII
  $props = @"
server-ip=127.0.0.1
server-port=$port
online-mode=false
enforce-secure-profile=false
gamemode=creative
force-gamemode=true
level-seed=4242
view-distance=16
simulation-distance=4
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
enable-rcon=true
rcon.port=$rconPort
rcon.password=mcvanpw
enable-query=false
max-tick-time=-1
"@
  Set-Content -Path (Join-Path $srv 'server.properties') -Value $props -Encoding ASCII
  Start-Process -FilePath $java17 -ArgumentList '-Xmx2G','-Xms512M','-jar','server.jar','nogui' -WorkingDirectory $srv -WindowStyle Hidden
  for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep 1
    try { Rcon-Open; break } catch { }
  }
  foreach ($g in 'doDaylightCycle false','doMobSpawning false','doWeatherCycle false','spawnRadius 0','randomTickSpeed 0') { Rcon "gamerule $g" | Out-Null }
  Rcon 'time set day' | Out-Null
  L 'server up + gamerules set'
  exit 0
}

# ---- client ----
Rcon-Open
$j0 = Join-Count
$proc = $null
$t0 = [Diagnostics.Stopwatch]::StartNew()
$tWin = -1.0
$tg = if ($Target -eq 'mcplay') { "mcplay$Gfx" + $(if ($Fps -ne 0) { 'cap' } else { '' }) } else { $Target }
$pmCsv = Join-Path $out ("pm_{0}_{1}x{2}_r{3}_{4}.csv" -f $tg, $W, $H, $Radius, $Tag)
Remove-Item $pmCsv -ErrorAction SilentlyContinue
if ($Target -eq 'vanilla') {
  $mc = Join-Path $env:APPDATA '.minecraft'
  $json = Get-Content (Join-Path $mc 'versions\1.20.1\1.20.1.json') -Raw | ConvertFrom-Json
  $cp = New-Object Collections.Generic.List[string]
  foreach ($lib in $json.libraries) {
    $ok = $true
    if ($lib.rules) {
      $ok = $false
      foreach ($r in $lib.rules) {
        $m = $true
        if ($r.os) {
          if ($r.os.name -and $r.os.name -ne 'windows') { $m = $false }
          if ($r.os.arch) { $m = $false }
        }
        if ($m) { $ok = ($r.action -eq 'allow') }
      }
    }
    if ($ok -and $lib.downloads.artifact) {
      $p = Join-Path (Join-Path $mc 'libraries') ($lib.downloads.artifact.path -replace '/', '\')
      if ($p -match 'natives-(linux|macos)' -or $p -match 'natives-windows-(x86|arm64)') { continue }
      if (Test-Path $p) { $cp.Add($p) } else { L "missing lib $p" }
    }
  }
  $cp.Add((Join-Path $mc 'versions\1.20.1\1.20.1.jar'))
  $nat = Join-Path $base 'natives'; New-Item -ItemType Directory -Force $nat | Out-Null
  $gd = Join-Path $base 'vgame'; New-Item -ItemType Directory -Force $gd | Out-Null
  # own gameDir: the real .minecraft options are never touched; vsync off, uncapped, sound off
  $opt = "version:3465`nenableVsync:false`nmaxFps:260`nrenderDistance:$Radius`nsimulationDistance:5`nfullscreen:false`nonboardAccessibility:false`nskipMultiplayerWarning:true`ntutorialStep:none`nguiScale:2`nsoundCategory_master:0.0`nmusicVolume:0.0`nshowAutosaveIndicator:false`nautoJump:false`n"
  Set-Content -Path (Join-Path $gd 'options.txt') -Value $opt -Encoding ASCII
  $a = @('-Xmx2G','-XX:+UseG1GC',"-Djava.library.path=$nat","-Djna.tmpdir=$nat","-Dorg.lwjgl.system.SharedLibraryExtractPath=$nat","-Dio.netty.native.workdir=$nat",'-cp',($cp -join ';'),'net.minecraft.client.main.Main',
    '--username',"V$Tag",'--version','1.20.1','--gameDir',$gd,'--assetsDir',(Join-Path $mc 'assets'),'--assetIndex','5','--uuid','00000000-0000-0000-0000-000000000001','--accessToken','0','--userType','legacy','--versionType','release','--width',"$W",'--height',"$H",'--quickPlayMultiplayer',"127.0.0.1:$port")
  $proc = Start-Process -FilePath $java17 -ArgumentList $a -WorkingDirectory $gd -PassThru
  L "vanilla started pid $($proc.Id) cp=$($cp.Count)"
} else {
  $inst = Join-Path $env:LOCALAPPDATA 'Programs\Voxel Client'
  if ($Gfx -eq 'vk') { $env:MCPLAY_GFX = 'vk'; Remove-Item Env:MCPLAY_GL -ErrorAction SilentlyContinue } else { $env:MCPLAY_GL = '1'; Remove-Item Env:MCPLAY_GFX -ErrorAction SilentlyContinue }; $env:MCPLAY_VSYNC = '0'; if ($Fps -lt 0) { Remove-Item Env:MCPLAY_FPS -ErrorAction SilentlyContinue } else { $env:MCPLAY_FPS = "$Fps" }; $env:MCPLAY_VIEW = "$Radius"
  $env:MCPLAY_W = "$W"; $env:MCPLAY_H = "$H"; $env:MCPLAY_FRAME_STATS = '1'; $env:MCPLAY_PROFILE = '1'; $env:MCPLAY_PROFILE_FROM = [string]($Settle + 6); $env:MCPLAY_PROGRESS = '1'
  $env:MCPLAY_SOUNDS = '0'; $env:MCPLAY_AUDIO = '0'
  $env:MCPLAY_RUN_S = [string]($Settle + $Secs + 90); $qf = Join-Path $out ('quit_' + $Tag); Remove-Item $qf -ErrorAction SilentlyContinue; $env:MCPLAY_QUIT_FILE = $qf
  $proc = Start-Process -FilePath (Join-Path $inst $Exe) -ArgumentList '127.0.0.1',"$port","M$Tag" -WorkingDirectory $inst -PassThru -RedirectStandardOutput (Join-Path $out ("mcplay_{0}_{1}x{2}_r{3}_{4}.out" -f $tg, $W, $H, $Radius, $Tag)) -RedirectStandardError (Join-Path $out ("mcplay_{0}_{1}x{2}_r{3}_{4}.err" -f $tg, $W, $H, $Radius, $Tag))
  L "mcplay started pid $($proc.Id)"
}
# wait for join
$joined = $false
for ($i = 0; $i -lt 600; $i++) {
  Start-Sleep -Milliseconds 250
  if ($tWin -lt 0) { $proc.Refresh(); if ($proc.MainWindowHandle -ne [IntPtr]::Zero) { $tWin = $t0.ElapsedMilliseconds } }
  if (($i % 4) -eq 3 -and (Join-Count) -gt $j0) { $joined = $true; break }
  if ($proc.HasExited) { break }
}
$tJoin = $t0.ElapsedMilliseconds
L "joined=$joined t_window_ms=$tWin t_join_ms=$tJoin exited=$($proc.HasExited)"
if (-not $joined) { if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }; exit 2 }
# window to front, topmost, report client size
Start-Sleep 3
$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 20; $i++) { $pp = Get-Process -Id $proc.Id; if ($pp.MainWindowHandle -ne [IntPtr]::Zero) { $hwnd = $pp.MainWindowHandle; break }; Start-Sleep 1 }
if ($hwnd -ne [IntPtr]::Zero) {
  [W32]::SetWindowPos($hwnd, [IntPtr](-1), 20, 20, 0, 0, 0x0001 -bor 0x0040) | Out-Null
  [W32]::SetForegroundWindow($hwnd) | Out-Null
  $r = New-Object W32+RECT; [W32]::GetClientRect($hwnd, [ref]$r) | Out-Null
  L ("window client {0}x{1} title='{2}'" -f ($r.R - $r.L), ($r.B - $r.T), $pp.MainWindowTitle)
} else { L 'no window handle' }
Start-Sleep $Settle
Rcon 'tp @a 0.5 ~ 0.5 0 8' | Out-Null
Start-Sleep 2
# PresentMon + yaw sweep: one full turn over $Secs seconds, one tp (rotation only) every 50 ms via rcon
$pm = Start-Process -FilePath (Join-Path $base 'pm\PresentMon.exe') -ArgumentList '--process_id',$proc.Id,'--output_file',$pmCsv,'--timed',"$Secs",'--terminate_after_timed','--no_console_stats','--stop_existing_session' -PassThru -WindowStyle Hidden
$n = $Secs * 20
$proc.Refresh(); $cpu0 = $proc.TotalProcessorTime.TotalSeconds; $wall0 = [Diagnostics.Stopwatch]::StartNew()
$sw = [Diagnostics.Stopwatch]::StartNew()
for ($k = 0; $k -lt $n; $k++) {
  $yaw = -180 + 360.0 * $k / $n
  Rcon ("tp @a ~ ~ ~ {0} 8" -f $yaw.ToString('0.0', [Globalization.CultureInfo]::InvariantCulture)) | Out-Null
  $wait = ($k + 1) * 50 - $sw.ElapsedMilliseconds
  if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
}
L "sweep done ${n} tp, elapsed $($sw.ElapsedMilliseconds) ms"
$proc.Refresh(); $cpuS = $proc.TotalProcessorTime.TotalSeconds - $cpu0; $wallS = $wall0.Elapsed.TotalSeconds
$cores = [Environment]::ProcessorCount
$srvp = Get-CimInstance Win32_Process -Filter "name='javaw.exe'" | ? { $_.CommandLine -match 'server.jar' } | select -First 1
$srvWs = 0; if ($srvp) { $sp = Get-Process -Id $srvp.ProcessId -ErrorAction SilentlyContinue; if ($sp) { $srvWs = [int]($sp.PeakWorkingSet64 / 1MB) } }
L ("METRICS target=$tg res=${W}x${H} r=$Radius tag=$Tag t_window_ms=$tWin t_join_ms=$tJoin cpu_core_pct={0:N0} cpu_total_pct={1:N1} peak_ws_mb={2} priv_mb={3} srv_peak_ws_mb={4}" -f (100.0 * $cpuS / $wallS), (100.0 * $cpuS / $wallS / $cores), [int]($proc.PeakWorkingSet64 / 1MB), [int]($proc.PrivateMemorySize64 / 1MB), $srvWs)
$pm.WaitForExit(15000) | Out-Null
if ($Target -eq 'mcplay') { Set-Content $qf 'q'; $proc.WaitForExit(15000) | Out-Null }
if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
L "client stopped; csv exists=$(Test-Path $pmCsv)"
Get-Process PresentMon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
