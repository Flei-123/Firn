param([string]$Csv, [double]$Skip = 10)
# PresentMon CSV -> p50 / p95 / p99 / 1% low (mean of the slowest 1 %) of MsBetweenPresents, first $Skip seconds dropped.
# Percentile = numpy linear interpolation (same definition as docs/MC-PERF.md).
$rows = Import-Csv $Csv
if ($rows.Count -eq 0) { "RESULT empty"; exit 1 }
$cols = $rows[0].PSObject.Properties.Name
$mcol = ($cols | ? { $_ -ieq 'MsBetweenPresents' } | Select -First 1)
if (-not $mcol) { "cols: " + ($cols -join ','); exit 1 }
$inv = [Globalization.CultureInfo]::InvariantCulture
$v = New-Object Collections.Generic.List[double]
$cum = 0.0
foreach ($r in $rows) {
  $d = [double]::Parse($r.$mcol, $inv)
  $cum += $d
  if ($cum -ge ($Skip * 1000.0)) { $v.Add($d) }
}
$s = $v.ToArray() | Sort-Object
$n = $s.Count
function Pct([double]$q) { $pos = ($n - 1) * $q; $lo = [Math]::Floor($pos); $hi = [Math]::Ceiling($pos); return $s[$lo] + ($s[$hi] - $s[$lo]) * ($pos - $lo) }
$k = [Math]::Max(1, [int][Math]::Ceiling($n * 0.01))
$low = ($s[($n - $k)..($n - 1)] | Measure-Object -Average).Average
$mean = ($s | Measure-Object -Average).Average
"RESULT {0} frames={1} p50={2:N2} p95={3:N2} p99={4:N2} low1pct_mean={5:N2} avg_fps={6:N1} total_s={7:N1}" -f (Split-Path $Csv -Leaf), $n, (Pct 0.5), (Pct 0.95), (Pct 0.99), $low, (1000.0 / $mean), ($cum / 1000.0)
