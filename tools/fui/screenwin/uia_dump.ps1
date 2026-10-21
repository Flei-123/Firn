param([string]$Exe, [string]$Out, [int]$Wait = 4, [string]$Invoke = "")
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$AE = [Windows.Automation.AutomationElement]
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds $Wait
$cond = New-Object Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $p.Id)
$win = $AE::RootElement.FindFirst([Windows.Automation.TreeScope]::Children, $cond)
$lines = New-Object System.Collections.Generic.List[string]
if ($win -eq $null) {
    $lines.Add("NOWINDOW pid=$($p.Id) exited=$($p.HasExited)")
} else {
    $walker = [Windows.Automation.TreeWalker]::RawViewWalker
    function Walk($el, $depth) {
        $c = $el.Current
        $pats = ($el.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName.Replace('PatternIdentifiers.Pattern', '') }) -join ','
        $r = $c.BoundingRectangle
        $lines.Add(("{0}|{1}|{2}|{3:N0},{4:N0},{5:N0},{6:N0}|en={7}|foc={8}|{9}" -f $depth, $c.ControlType.ProgrammaticName.Replace('ControlType.', ''), $c.Name, $r.X, $r.Y, $r.Width, $r.Height, $c.IsEnabled, $c.HasKeyboardFocus, $pats))
        $k = $walker.GetFirstChild($el)
        while ($k -ne $null) {
            Walk $k ($depth + 1)
            $k = $walker.GetNextSibling($k)
        }
    }
    Walk $win 0
    if ($Invoke -ne "") {
        $nc = New-Object Windows.Automation.PropertyCondition($AE::NameProperty, $Invoke)
        $t = $win.FindFirst([Windows.Automation.TreeScope]::Descendants, $nc)
        if ($t -eq $null) { $lines.Add("INVOKE-NOTFOUND $Invoke") }
        else {
            try {
                $ip = $t.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
                $ip.Invoke()
                $lines.Add("INVOKED $Invoke")
            } catch { $lines.Add("INVOKE-ERR $Invoke $($_.Exception.Message)") }
            Start-Sleep -Seconds 2
            $lines.Add("--- AFTER ---")
            Walk $win 0
        }
    }
}
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
[IO.File]::WriteAllLines($Out, $lines)
"lines=$($lines.Count)"
