# Windows build of mcplay / mcclient

Cross-compile on Linux, no Windows needed:

    FIRNLIB=lib firnc --target=x86_64-windows --opt-level=release-fast -o dist/mcplay.exe demos/mcplay/main.fi
    FIRNLIB=lib firnc --target=x86_64-windows --opt-level=release-fast -o dist/mcclient.exe demos/mcclient/main.fi

(`--win-subsystem=windows` hides the console window, but then the stdout log is lost.)
Output: single PE exe, only system DLLs (kernel32, user32, gdi32, ws2_32, advapi32, crypt32, winmm, dwmapi, ...); opengl32 / WASAPI are loaded at run time.
Release exes are copied to /root/projects/minecraft-neu/dist/ (not committed).

Test: `bash tools/mc/play/run_wine.sh` (Wine 8 win64 + Xvfb + official 1.20.1 server; own WINEPREFIX, MCPLAY_GL=1 adds the wgl run).
Proves: link, start, TCP login, chunk stream, window, CPU raster, WGL GL path (Wine llvmpipe), audio sink.
Does NOT prove: real Windows, real GPU/driver, real WASAPI device, raw mouse input, fullscreen, DPI.

Platform twins used: window.backend.windows, gpu.glctx.windows + gpu.wgl.windows, audio.dev.windows + audio.wasapi.windows,
input.rawdev.windows, std.env.windows. Network goes through the compiler's Winsock seam (win_seam.rs).

Findings fixed here:
- `lib/gpu/wgl.windows.fi`: local `at` broke the parser (`at[0] = ...`) -> renamed `attrs`; the Windows WGL libs did not compile.
- Seam maps WSAETIMEDOUT of a timed-out recv to ETIMEDOUT (110), Linux gives EAGAIN (11): mc_codec.fill read it as a broken
  connection (login failed -22). mc_codec now accepts both (real fix belongs in the seam, see roadmap).
- mcplay sound bank default dir: %LOCALAPPDATA%/mcplay/sounds (no HOME on Windows).

Audio under Wine: no sound card here; FIRN_AUDIO=device works with a pulseaudio null sink (winepulse): ~1.45 MB written in 8 s.
