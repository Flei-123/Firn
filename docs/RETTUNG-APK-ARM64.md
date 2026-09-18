# Rettungszweig `rettung/apk-arm64` — Stand und was noch fehlt

Wiederhergestellt am 18.09.2026 aus
`/root/repo-backup/firn-worktree-patches/firn-apk.patch`, nachdem der
History-Rewrite den losen Arbeitsbaum `/root/firn-apk` abgeraeumt hatte.

## Drin
- `compiler/src/syscalls.rs`: ARM64-Nummern fuer mkdir(83) -> mkdirat,
  unlink(87) -> unlinkat, chmod(90) -> fchmodat, umask(95). Ohne die war
  der Browser fuers Telefon nicht uebersetzbar.
- `compiler/src/codegen_a64.rs`, `lib/gc/gc.fi`: zugehoerige Aenderungen.
- `compiler/src/gc.rs`: `RUNTIME_COLLECTS` von 4 auf 6 Namen.

## UNFERTIG — hier weitermachen
`RUNTIME_COLLECTS` nennt **`gc_bottom_swap`**, aber diese Funktion gibt es
nirgends im Baum (geprueft ueber alle `*.fi` und `*.rs`). Der Eintrag
greift also ins Leere und ist wirkungslos. `gc_cycle_finish` dagegen
existiert (`lib/gc/gc.fi:2740`).

Zu klaeren: sollte `gc_bottom_swap` noch geschrieben werden (Stapel-Boden
tauschen beim Thread-Wechsel?), oder war der Name nur vorgemerkt und
gehoert wieder raus?

## Geprueft
`cargo check` im Ordner `compiler/` laeuft sauber durch (nur Warnungen).
Ein Bau fuers Telefon wurde NICHT gemacht.
