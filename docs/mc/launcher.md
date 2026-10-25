# FleiLauncher integration (Flei Engine)

The launcher (`/root/FleiLauncher`, Tauri + Rust + Svelte) has the loader `fleiengine` ("Flei Engine (mcplay)") next to Vanilla/Fabric/Forge/NeoForge/Quilt.

## Start path (launcher `src-tauri/src/engine/`)
* `versions.rs`: Minecraft version -> protocol number (only versions mcplay supports; others fail with the supported list).
* `install.rs`: finds `mcplay(.exe)` (config `engine_path`, env `FLEI_ENGINE_PATH`, `~/.fleilauncher/engine/`, `engine/` next to the launcher exe); otherwise downloads `engine-<platform>.zip` from `manifest.json` (`https://fleilauncher.fleitec.com/download/engine/`), the manifest must carry a valid ed25519 signature (`manifest.json.sig`, hex, checked against the public key in `engine/verify.rs`), the zip is streamed with progress events (`launch://progress`, stage `engine`) and sha256 checked, re-installed when the manifest version changes. A signature failure never falls back to an old engine silently; only network errors do.
* `run.rs`: prepares the vanilla version json, asset index + objects and client jar (same as other profiles, no Java), writes a 0600 session file under `~/.fleilauncher/run/`, starts mcplay with the environment below.
* `session.rs`: session text (`name`, `protocol`, `view`, `host`, `port`, optional `uuid` + `mc_token` for online accounts). Without a server address (or auto-join off) it writes `form=1`: mcplay shows its start form prefilled with the launcher name and keeps the token.

## Environment given to mcplay
`MCPLAY_SESSION_FILE`, `MC_VANILLA_JAR` (`versions/<v>/<v>.jar`), `MC_VANILLA_ASSETS` (`assets/`), `MC_VANILLA_INDEX`, `MC_VANILLA_VERSION`, `MC_PACK_SOUNDS` (`both` default, config `engine_sounds`), `MCPLAY_W`, `MCPLAY_H`, `MCPLAY_FULLSCREEN` (config `engine_width/height/fullscreen`).

## Release of the engine package
`bash tools/mc/engine_release.sh` (via heavy) builds Windows + Linux mcplay, the sound bank (no Mojang assets) and writes `engine-windows.zip`, `engine-linux.zip`, `manifest.json` to `$OUT` (default `/root/projects/minecraft-neu/dist/engine`). Upload the three files to `/var/www/fleilauncher/download/engine/`.

## Gates
`bash tools/mc/play/run_session.sh` stage E: `form=1` session shows the form with the session name and joins.
Launcher unit tests (versions, session text, zip extraction) run in a scratch crate; the Tauri app itself needs a Windows/webkit build host.

## Singleplayer, way A (local server process)
Launcher module `src-tauri/src/engine/singleplayer/` (commands `list_singleplayer_worlds`, `create_singleplayer_world`, `delete_singleplayer_world`, `launch_singleplayer`; api.ts wrappers in `api`):
* `worlds.rs`: worlds live in `<instance>/singleplayer/<folder>/` with `world.json` (name, seed, mode, difficulty, hardcore, cheats), `server.properties`, `eula.txt`, `world/`. Folder names are sanitized and unique; path traversal is rejected.
* `jar.rs`: `downloads.server` + `javaVersion.majorVersion` from the cached vanilla version json; `~/.fleilauncher/servers/<mc>/server.jar` is downloaded with sha1 check and re-verified on every start. Java comes from the launcher's `ensure_java`.
* `settings.rs` / `properties.rs`: merges `server-ip=127.0.0.1`, free `server-port`, `online-mode=false`, `level-name=world`, seed, gamemode (forced), difficulty, hardcore into an existing `server.properties` without dropping other keys.
* `port.rs`: free loopback port per start. `server.rs`: starts `java -Xmx.. -jar server.jar nogui`, waits for the `Done (` log line, forwards lines as `minecraft://log` (stream `server`), stops with `stop` on stdin (45 s grace, then kill).
* `mod.rs`: `launch_singleplayer` starts the server, then `run::launch_engine_to` with `127.0.0.1:<port>` (session file with host/port = auto-join). A watcher thread stops the server when mcplay leaves the launcher's `running` map (exit or kill); a failed mcplay start stops it too.
* `eula.rs`: `launch_singleplayer` refuses until `AppConfig.singleplayer_eula_accepted` is set; the UI shows the EULA dialog once and calls `accept_singleplayer_eula`. `eula.txt` is only written after that.
* `orphans.rs` / `probe.rs`: each running server writes `launcher-server.pid` (`pid`, `launcher_pid`, `jar`) into its world folder; it is removed on stop/drop. On launcher start `reap_orphans` scans `instances/*/singleplayer/*`: a pid whose command line contains the recorded jar path and whose launcher is gone gets SIGTERM (taskkill on Windows), then KILL after 20 s. Foreign processes (pid reuse) and servers of another live launcher are left alone. `delete_singleplayer_world` refuses while the server is alive. Command line lookup: `ps -o args=` on unix, `Get-CimInstance Win32_Process` on Windows (untested there).
* Server jar download streams with progress (`launch://progress`, stage `server_download`); other stages: `prepare`, `java`, `server`, `starting`.
* UI: profile detail tab "Singleplayer" (Flei Engine profiles only): `SingleplayerPanel.svelte` (list, play, delete, stage progress), `WorldCreateForm.svelte`, `EulaDialog.svelte`, helpers in `src/lib/singleplayer.ts`; i18n keys `sp.*` (EN + DE).
Tests: 70 unit tests in the scratch crate plus an end-to-end test (also reaps a real orphaned server and checks the world is saved) against the real 1.20.1 server jar (fake mcplay does status ping + offline login, then exits; server stops, world saved).

## Signing and settings UI
* `engine_release.sh` signs `manifest.json` with the raw ed25519 key `SIGN_KEY` (default `/root/.fleilauncher-engine-signing/engine_signing.pem`, never in a repo) and writes `manifest.json.sig` (128 hex chars). Set `ALLOW_UNSIGNED=1` only for local tests; the launcher rejects unsigned manifests. Upload all four files (two zips, `manifest.json`, `manifest.json.sig`).
* Key rotation: add the new public key to `PUBLIC_KEYS` in `engine/verify.rs`, release a launcher, then switch `SIGN_KEY`.
* Settings page has a Flei Engine card (window size, fullscreen, view distance 2-32, sound mode, engine folder, download/update button with progress). Values are clamped in `AppConfig::normalize_engine`.
* Profile editor lists only versions from `engine/versions.rs` when the loader is Flei Engine (`engine_supported_versions`).
