# Windows build of mcplay / mcclient

Cross-compile on Linux, no Windows needed:

    FIRNLIB=lib firnc --target=x86_64-windows --opt-level=release-fast -o dist/mcplay.exe demos/mcplay/main.fi
    FIRNLIB=lib firnc --target=x86_64-windows --opt-level=release-fast -o dist/mcclient.exe demos/mcclient/main.fi

`--win-subsystem=windows` hides the console window; mcplay then writes stdout/stderr into a log file (below).
Output: single PE exe, only system DLLs (kernel32, user32, gdi32, ws2_32, advapi32, crypt32, winmm, dwmapi, ...); opengl32 / WASAPI are loaded at run time.
Release exes are copied to /root/projects/minecraft-neu/dist/ (not committed).

Test: `bash tools/mc/play/run_wine.sh` (Wine 8 win64 + Xvfb + official 1.20.1 server; own WINEPREFIX, MCPLAY_GL=1 adds the wgl run).
Proves: link, start, TCP login, chunk stream, window, CPU raster, WGL GL path (Wine llvmpipe), audio sink.
Does NOT prove: real Windows, real GPU/driver, real WASAPI device, raw mouse input, fullscreen, DPI.

Platform twins used: window.backend.windows, gpu.glctx.windows + gpu.wgl.windows, audio.dev.windows + audio.wasapi.windows,
input.rawdev.windows, std.env.windows. Network goes through the compiler's Winsock seam (win_seam.rs).

Findings fixed here:
- `lib/gpu/wgl.windows.fi`: local `at` broke the parser (`at[0] = ...`) -> renamed `attrs`; the Windows WGL libs did not compile.
- Seam mapped WSAETIMEDOUT of a timed-out recv to ETIMEDOUT (110), Linux gives EAGAIN (11): mc_codec.fill read it as a broken
  connection (login failed -22). FIXED in the seam (compiler/src/win_seam.rs `__win_ioerrno`: send/recv/sendto/recvfrom -> -11;
  connect keeps -110). Proof: `bash tools/windows/timeout.sh` (Linux + Wine both `RECV -11`; the old firnc gives -110). The mc_codec
  workaround stays (accepts both).
- mcplay sound bank default dir: %LOCALAPPDATA%/mcplay/sounds (no HOME on Windows).

Audio under Wine: no sound card here; FIRN_AUDIO=device works with a pulseaudio null sink (winepulse): ~1.45 MB written in 8 s.

## Distribution (r134, r137)
`bash tools/mc/dist.sh` (via heavy) builds into $DIST (default /root/projects/minecraft-neu/dist, never committed):
- `mcplay-win/`: mcplay.exe (GUI subsystem, no console), mcplay-console.exe, mcclient.exe, `sounds/` (bank from tools/sounds/gen, 252 wavs, 15 MB), README.txt
- `mcplay-win.zip` (pack.py win-zip) and `mcplay-setup.exe` (pack.py win-installer: Start-menu shortcut, desktop shortcut, uninstall.exe; needs the mingw binutils).
Sound bank search order: `MCPLAY_SOUNDS` -> `<exe dir>/sounds` -> `$HOME/.cache/mcplay/sounds` / `%LOCALAPPDATA%/mcplay/sounds`.
Log: `MCPLAY_LOG=<file>` (`-` = never); unset and no console (fd 1 dead, GUI exe from a shortcut) -> `%LOCALAPPDATA%/mcplay/mcplay.log`.
`mcplay.exe` without arguments opens the start screen (lib/mc/mc_startscreen_run.fi): host, port, name, saved server list, config in `%LOCALAPPDATA%\mcplay\mcplay.cfg` (MCPLAY_CONFIG overrides, MCPLAY_START_AUTO=1 skips the form when the config is complete). Shortcuts need no arguments. Gate: tools/mc/play/run_wine.sh (typed form under Wine, config saved, second launch prefilled).

## Input under Wine (r135)
`run_wine.sh` stage `input` drives mcplay.exe with xdotool (X11 -> Wine): 8 relative moves of (12,5) arrive as raw_dx=96 raw_dy=40 (exact), look yaw/pitch follow,
Esc releases the capture (grab=false), F11 toggles fullscreen (window rect 640x360 -> 1024x600 -> 640x360, mcplay counts 2 presses), DPI reported 96.
Keys must go through XTEST (`xdotool key` without --window); XSendEvent keys are ignored by Wine. Still untested: real raw input device, ClipCursor on real Windows, DPI != 96, multi-monitor fullscreen.
The end line `mcplay: input grab= fullscreen= f11= raw_dx= raw_dy= yaw= pitch= win= dpi=` is the observable.
