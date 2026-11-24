# Self-install (lib/pack/self*.fi)

A running portable program installs itself for the current user, no administrator needed.

- `pack.selfplan`: pure paths and names (`plan_build`), testable on any OS.
- `pack.selfinst`: `install_run` (copy the running exe, Start menu and Desktop shortcut, Apps & features entry, optional autostart), `install_state`, `set_desktop_link`, `set_autostart`, `launch_installed`, `sync_version`.
- `pack.selfuninst`: `uninstall_request` (Windows: a temp copy finishes the job after the program exited), CLI flags `--uninstall`, `--keep-data`, `--delete-data`, `cli_run`.
- `pack.selfsys`: small file and process helpers.

Windows: `%LOCALAPPDATA%\Programs\<name>`, `.lnk` files written by `pack.lnk`, `HKCU\...\Uninstall\<id>` and the Run key via `pack.winreg` and `desktop.autostart`.
Linux: `~/.local/bin/<id>`, `.desktop` entry, 256 px icon, autostart entry.

Tested on Linux and under Wine (`tests/install/` of Jarvis Native). Not seen on a real Windows.
