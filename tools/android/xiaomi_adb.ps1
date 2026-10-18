# SPDX-License-Identifier: MPL-2.0
# tools/android/xiaomi_adb.ps1 -- helpers for ADB on a Xiaomi/HyperOS phone from a WINDOWS machine.
#
#   . .\xiaomi_adb.ps1 -Adb <adb.exe> -Serial <serial> -Dir <work dir>
#   Inst C:\x\app.apk        install (taps the "install via USB" prompt, installer = packageinstaller)
#   Shot name                screenshot -> <Dir>\name.png (pull; NEVER `exec-out ... > f.png` in
#                            Windows PowerShell 5: the redirect re-encodes the bytes and breaks the PNG)
#   Lines (A shell ...)      adb output as a line array
#
# Facts measured on a Xiaomi 14T Pro (HyperOS, Android 16), see docs/ANDROID.md "Real device (Xiaomi)".
param([string]$Adb, [string]$Serial, [string]$Dir = "$env:TEMP")
$script:adb = $Adb; $script:s = $Serial; $script:d = $Dir
function A { & $script:adb -s $script:s @args 2>&1 | Out-String }
function Lines($x) { $x -split "`r?`n" }
function Inst($apk) {
    # HyperOS asks "Install this app via USB?" (10 s, then it cancels:
    # INSTALL_FAILED_USER_RESTRICTED). The button "Install" is at (351,2370) on a 1220x2712 screen.
    # -i packageinstaller: apps with an unknown installer get "Camera disabled by policy".
    $j = Start-Job { param($a, $s, $p) & $a -s $s install -r -i com.google.android.packageinstaller $p 2>&1 } -ArgumentList $script:adb, $script:s, $apk
    Start-Sleep 2
    & $script:adb -s $script:s shell input tap 351 2370 2>&1 | Out-Null
    Receive-Job $j -Wait | Out-String
}
function Shot($name) {
    & $script:adb -s $script:s shell screencap -p /sdcard/xm_tmp.png 2>&1 | Out-Null
    & $script:adb -s $script:s pull /sdcard/xm_tmp.png "$script:d\$name.png" 2>&1 | Out-Null
    & $script:adb -s $script:s shell rm /sdcard/xm_tmp.png 2>&1 | Out-Null
}
