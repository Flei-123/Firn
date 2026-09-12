# Runde WINDOWS-AUF-MAIN (12.09.2026)

Vier Aenderungen aus zwei Seitenzweigen nach `main` geholt, damit der
Windows-Bau von Certus wieder aus EINEM festgenagelten Commit laeuft.

## Warum

Certus baut `certus.exe` aus `lib/windows/certus_main.fi`. Dafuer
braucht es vier Dinge, die bis heute in **keinem einzigen** Firn-Zweig
zusammen lagen:

| | woher | was |
|---|---|---|
| `#[win_callback]` | `certus-windows` 6a10f8ca | Win64 -> System-V-Thunk, damit Windows eine Firn-Funktion zurueckrufen kann (WM_SIZE, WM_DESTROY werden GESENDET) |
| `--win-subsystem` | `certus-windows` cb9feb8a | ein Fensterprogramm ist kein Konsolenprogramm; dazu iphlpapi/advapi32/user32 in der Einfuhrtabelle |
| Importtabelle | `certus-windows` 44b7d7cc | `SystemParametersInfoW`, `GetWindowRect` |
| `gc_bottom_swap` | `dns-pic` f90abcf2 | Stapelboden tauschen, fuer eine Koroutine auf eigenem Stapel (`lib/core/fiber.fi` ruft es) |

Der Zweig `certus-windows` ist gegenueber `main` um 736 Commits
zurueck und kann Certus **nicht mehr uebersetzen**: ihm fehlt die
Aufhebung von *"'const' supports only integer and bool types in
stage 0"* (`sema.rs`), und fUi braucht `const f64`
(`ITALIC_SHEAR`, `BOLD_STEP`). Es gab also keinen Uebersetzer mehr, der
beides kann -- Windows UND fUi.

## Kostet es die Linux-Seite etwas?

Nein, und das ist gemessen und nicht behauptet. Gemessen wird der
ERZEUGTE ASSEMBLER (`--emit=asm`), nicht das gebundene Programm: der
Assembler haengt nur am Uebersetzer, das gebundene Programm auch am
Binder.

Derselbe Quelltext, einmal mit dem Uebersetzer von `55497b7f` und
einmal mit diesem Stand:

    tasten_main       GLEICH       447 961 Oktett
    vault_main        GLEICH       490 920 Oktett
    hookprobe_main    GLEICH     4 169 333 Oktett
    web1_main         GLEICH    35 052 038 Oktett
    -----------------------------------------------
    GLEICH 4   ANDERS 0

Dazu drei fertig gebundene Programme, ebenfalls oktettgleich:
`tasten_main` (129 776), `probe` (98 088), `vault_main` (141 672).

Das deckt sich mit der Messung des urspruenglichen Commits 6a10f8ca
("`--emit=asm` ueber tests/ und examples/, alter gegen neuen
Uebersetzer, GLEICH 230 ANDERS 0").

### Zwei Fallen, die dabei aufgefallen sind

Der erste Anlauf dieser Gegenprobe war FALSCH, und beide Gruende sind
es wert, aufgeschrieben zu werden:

1. Der Uebersetzer, den ich fuer "den alten" hielt, war ein SYMLINK in
   einen anderen Arbeitsbaum -- also ein anderer Commit. Wer zwei
   Uebersetzer vergleicht, muss
   `ls -la <baum>/compiler/target/release/firnc` ansehen und nicht nur
   den Pfad.

2. Der Uebersetzer sucht ein Modul ZULETZT in
   `<Verzeichnis der Uebersetzerdatei>/../lib`. Zwei Uebersetzer aus
   verschiedenen Baeumen sehen deshalb VERSCHIEDENE Bibliotheken, auch
   wenn `$FIRNLIB` gleich ist. Daran hing ein scheinbarer
   1-MB-Unterschied bei `km_main` -- in Wahrheit war es
   `gc_bottom_swap`, also genau der vierte Commit dieser Runde.

## Der Nachweis, dass es wirkt

    FIRNLIB=<certus>/lib firnc --opt-level=release-safe \
        --target=x86_64-windows --win-subsystem=windows \
        -o certus.exe lib/windows/certus_main.fi

    -> RC=0, 7 808 015 Oktett
    -> PE32+ executable (GUI) x86-64, 6 sections, Subsystem 2 (WINDOWS_GUI)

Vorher brach derselbe Aufruf mit `unknown attribute 'win_callback'` ab.

## Nebenbefund: `km_main` war ebenfalls unbaubar

Certus' `lib/core/fiber.fi` ruft `gc_bottom_swap` (Zeilen 329, 336),
und `tools/corebench/km_main.fi` zieht es ueber `core.script` herein.
Mit dem festgenagelten Uebersetzer:

    error: unknown function 'gc_bottom_swap'
        --> lib/core/fiber.fi:329:22

Von zehn Firn-Arbeitsbaeumen auf dieser Maschine hatte GENAU EINER die
Funktion. Der vierte Commit dieser Runde schliesst also nicht nur eine
Windows-Luecke, sondern repariert auch `km_main` auf Linux:
RC=0, 7 251 320 Oktett.
