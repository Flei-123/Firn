# Runde WINDOWS — Firn baut Windows-Programme

SPEC.md sagte in Zeile 127: **Zielbinaerformat ELF**. Der Uebersetzer kannte
zwei Befehlssaetze und zwei Umgebungen (Linux, freistehend), gab
Intel-Assembler aus und rief `as` und `ld`. `syscall(nr, a1..a6)` war in die
Sprache eingebaut und bildete direkt auf die Linux-Systemaufrufnummern ab —
und genau das ist auf Windows nicht moeglich, weil es dort keinen
Systemaufruf gibt, den ein Programm benutzen darf: die Nummern von `ntdll`
sind zwischen Windows-Fassungen absichtlich nicht stabil.

Diese Runde fuegt der zweiten Achse des Zielmodells aus Runde
ARM-FREESTANDING (`docs/ROUND-ARM-FREESTANDING.md`) einen dritten Wert
hinzu: **`Os::Windows`**. Drei Behauptungen, jede davon eine Messung und
kein Satz:

1. **`firnc --target=x86_64-windows` baut eine `.exe`, die laeuft.**
   `examples/hello.fi` und `examples/tour.fi` geben unter Wine
   **zeichengleich** dasselbe aus wie die Linux-Bauten.
2. **Von 304 vergleichbaren Faellen der Testsammlung verhalten sich 299
   auf beiden Betriebssystemen identisch (98 %)** — gleiche Ausgabe,
   gleicher Ausgangswert. Die fuenf uebrigen scheitern aus **genau zwei**
   Gruenden, und beide heissen „zweiter Kontrollfluss": Faeden (`clone`)
   und Prozesse (`fork`/`execve`/`wait4`).
3. **Die Linux-Seite hat sich um nichts verschlechtert.** 314 von 314
   Programmen aus `tests/` und `examples/` liefern vom Uebersetzer **vor**
   dieser Runde und vom Uebersetzer **nach** ihr **zeichengleichen**
   `--emit=asm`-Text. 0 unterschiedlich.

---

## 1. Die Entscheidungen, und warum

### 1.1 PE/COFF: fremder BINDER ja, fremder CODE nein

Ausgegeben wird derselbe Intel-Syntax-Assembler wie immer; uebersetzt und
gebunden wird er von der **COFF-Ausgabe derselben Binutils**:

| | Linux | Windows |
|---|---|---|
| Uebersetzer | `as --64` | `x86_64-w64-mingw32-as` |
| Binder | `ld` | `x86_64-w64-mingw32-ld -e _start --subsystem console` |

Beide sind GNU Binutils 2.40. Das ist genau die Rolle, in der `as` und `ld`
schon immer benutzt werden: als Assembler und als Binder, **nie** als
Uebersetzer. Ein C-Uebersetzer wird auch hier nicht aufgerufen.

**Die Importtabelle schreibt der Uebersetzer selbst**
(`compiler/src/win.rs`, `idata_asm`). Der uebliche Weg waere `-lkernel32`
gewesen — eine Importbibliothek, deren Objektdateien dann im Bild landen.
Firns Regel „kein Fremdcode im ausgefuehrten Programm" ist hier leichter zu
halten als zu erklaeren: die fuenf Tabellen einer PE-Bindung sind reine
DATEN, sie sind zwanzig Zeilen Assembler pro DLL, und das PE-Bindeskript des
Binders sammelt `.idata$2` bis `.idata$7` bereits in der richtigen Reihenfolge
ein und schliesst das Verzeichnis ab.

```
.idata$2   ein 20-Oktett-Deskriptor je DLL
.idata$4   die Nachschlagetabelle (was der Lader liest)
.idata$5   die Adresstabelle (was der Lader ueberschreibt, was wir rufen)
.idata$6   Hinweis/Name-Paare
.idata$7   die DLL-Namen
```

**Was wirklich im Bild steht** (`tools/windows/machine.sh`, Abschnitt 1):
kein `mainCRTStartup`, kein `__mingw*`, kein `_pei386_runtime_relocator`,
keine Importbibliothek. Der einzige fremde Name ist `__CTOR_LIST__` /
`__DTOR_LIST__` — sechzehn Oktett leere Listenmarkierungen, die das
**Bindeskript** einsetzt, so wie `ld` auf Linux `_edata`/`_end` einsetzt.
Das steht hier, statt verschwiegen zu werden.

`lld-link` wurde nicht genommen. Es haette gegangen (es liegt auf diesem
Rechner), aber es haette eine zweite Werkzeugfamilie in ein Projekt gebracht,
das seit Runde 1 mit GNU Binutils baut — und die eigene Importtabelle
funktioniert mit dem GNU-Binder ohne jede Sonderbehandlung.

`hello.exe` ist **43 492 Oktett**, `tour.exe` **355 982 Oktett**, mit vier
Abschnitten: `.text`, `.rodata`, `.bss` (ohne Platz in der Datei),
`.idata`.

### 1.2 Der Aufrufvertrag: ein Zwischenstueck statt einer zweiten Konvention

Das ist die Stelle, an der Portierungsversuche sterben. Windows x64 nimmt die
ersten vier ganzzahligen Argumente in **RCX, RDX, R8, R9**, verlangt **32
Oktett Schattenplatz** unter der Ruecksprungadresse, den der Gerufene
beschreiben darf, und zaehlt **RSI, RDI und XMM6–XMM15 als
aufgerufenen-bewahrt**. System V nimmt sechs Argumente in RDI, RSI, RDX,
RCX, R8, R9, hat keinen Schattenplatz und behandelt RSI/RDI als Kladde.

**Firn-interne Aufrufe muessen aber gar nicht Win64 sein.** Nichts an
Windows schaut zu, wie eine Firn-Funktion eine andere ruft; SPEC §13 nennt
den Firn-zu-Firn-Vertrag ohnehin eine dokumentierte ABI eigener Art. Win64
sein MUSS jeder Aufruf, der das Programm verlaesst — und jeder, der
hereinkommt (ein Rueckruf, den es in dieser Runde nicht gibt).

Also bekommt die Grenze **eine** klar umrissene Stelle: fuer jede eingefuehrte
Funktion schreibt der Uebersetzer ein **Zwischenstueck**, das die Argumente
nach System V annimmt, sie nach Win64 umsortiert, den Schattenplatz belegt
und durch die Importadresstabelle springt.

```
_Fwin.WriteFile:                    # 5 Argumente
    push rbp
    mov rbp, rsp
    sub rsp, 48                     # 32 Schatten + 1 Wort, auf 16 aufgerundet
    mov qword ptr [rsp+32], r8      # Argument 5 ZUERST ...
    mov r9, rcx                     # ... denn r8/r9 werden gleich ueberschrieben
    mov r8, rdx
    mov rdx, rsi
    mov rcx, rdi
    call [rip + __imp_WriteFile]
    leave
    ret
```

Die **Reihenfolge der Bewegungen ist die ganze Richtigkeit**: `r8` und `r9`
sind Win64-Argumentregister 3 und 4 UND gleichzeitig System-V-Argumentregister
5 und 6. Wer `mov r8, rdx` schreibt, bevor Argument fuenf `r8` verlassen hat,
verliert es. Argumente ab dem siebten kommen aus dem Rahmen des Rufers
(`[rbp+16]`, `[rbp+24]`, …) und gehen nach `[rsp+48]`, `[rsp+56]`, …
`CreateFileW` mit seinen sieben Argumenten ist der Fall, der das erzwingt.

Der Preis, ehrlich: **vier bis acht zusaetzliche `mov` je Win32-Aufruf**, und
eine Firn-Funktion kann Windows noch nicht als **Rueckruf** uebergeben werden
(dafuer braeuchte es das Spiegelbild dieses Zwischenstuecks). Der Gewinn: der
Registerzuteiler, die Rahmenaufteilung und beide Codeerzeuger bleiben
unberuehrt — und genau deswegen KANN sich die Linux-Seite nicht
verschlechtern (Abschnitt 4.6).

Das Zwischenstueck ist fuer System-V-Rufer durchsichtig: es zerstoert nur
`rax`, `rcx`, `rdx`, `r8`–`r11`, und die sind in beiden Vertraegen
Ruferbewahrt.

### 1.3 Stapel-Sondierung

Ein Windows-Fadenstapel ist reservierter Adressraum mit einer
**PAGE_GUARD**-Seite am unteren Ende; erst wenn diese Seite beruehrt wird,
legt der Kern eine weitere an und schiebt die Wache nach unten. Eine
Funktion, die `rsp` in einem Befehl um 40 KiB senkt und dann am Boden ihres
Rahmens schreibt, steigt ueber die Wache hinweg in reservierten, nicht
zugeteilten Speicher — und der Prozess stirbt an einer Stelle, die mit der
Ursache nichts zu tun hat.

Deshalb sondiert **jeder Rahmen ab einer Seite** (`codegen_x86::emit_frame`,
`win.rs::chkstk_asm`):

```
    mov rax, 60016
    call _Fwin.chkstk        # laeuft Seite fuer Seite nach unten
    sub rsp, 60016
```

Das ist kein Randfall: `var buf: [u8; 8192]` ist einer, und jeder Zerteiler
in diesem Baum hat einen. Gemessen (`tools/windows/machine.sh`, Abschnitt 3):
ein Programm mit einem Rahmen von 60 000 Oktett laeuft unter Wine und gibt
seinen erwarteten Ausgangswert zurueck; ein kleiner Rahmen sondiert **nicht**
(Gegenprobe).

### 1.4 SEH und Aufrollen — NICHT gebaut

`.pdata` und `.xdata` gibt es in dieser Runde **nicht**. Was dadurch nicht
geht, ausdruecklich und nicht kleingeredet:

* **Kein brauchbarer Absturzbericht.** Der Windows-Stapelrueckblick
  (`StackWalk64`, der Debugger, WER) findet fuer unsere Funktionen keinen
  Eintrag und kann den Rahmen nicht aufloesen. Der eigene Panikpfad ist davon
  nicht betroffen — er schreibt seine Meldung selbst und beendet sich selbst
  (Abschnitt 4.1) —, aber wer mit einem Debugger an einen Absturz herangeht,
  sieht ab der ersten Firn-Funktion nichts Verwertbares.
* **Kein Aufrollen.** `#[unwinds]`/`throw` (SPEC §5.3) kann auf Windows
  nicht funktionieren, sobald es ueber eine Systemgrenze laufen soll. Innerhalb
  von Firn laeuft es, weil Firn seine Fehlerunionen als Werte transportiert
  und nicht ueber SEH — aber eine Windows-Ausnahme aus einer DLL heraus wird
  von unseren Rahmen nicht gefangen und nicht durchgereicht.
* **Keine `__try`/`__except`-Zusammenarbeit** mit fremdem Code.

Da unsere Rahmen alle die einfachste denkbare Form haben (`push rbp` / `mov
rbp, rsp` / `sub rsp, N`), waere ein `.xdata`-Eintrag je Funktion mechanisch
erzeugbar. Er ist Arbeit fuer eine naechste Runde, nicht ein prinzipielles
Hindernis.

---

## 2. Die Naht statt `syscall`

### 2.1 Wie viele Systemdienste wirklich gebraucht werden

Nachgezaehlt statt geschaetzt (`syscall(...)`-Aufrufe mit aufgeloesten
Konstanten, ueber den ganzen Baum):

| | verschiedene Nummern |
|---|---:|
| `lib/` — die ganze Grundbibliothek | **34** |
| `lib/` + `tests/` + `examples/` | **35** |
| **Certus** (`/root/certus/lib`, der Browsermotor selbst) | **7** — `read`, `write`, `open`, `close`, `mmap`, `munmap`, `clock_gettime` |
| Certus, benannte `SYS_`-Konstanten im Baum | **14** (die sieben oben plus `socket`, `bind`, `setsockopt`, `ioctl`, `poll`, `fcntl`, `exit`) |

Die Groessenordnung, die der Projekteigner genannt hat, stimmt also: der
BROWSER kommt mit vierzehn aus, und sieben davon ruft er selbst. Der Rest der
Bibliothek braucht mehr, weil dort Faeden, Prozesse und der Netzstapel
liegen.

**Abgebildet sind 35 Nummern**, ueber **42 Win32-Funktionen** aus drei DLLs
(27 aus `kernel32.dll`, 14 aus `ws2_32.dll`, 1 aus `advapi32.dll`).

### 2.2 Die Abbildung

| Nr. | Linux | Windows |
|---:|---|---|
| 0 | `read` | `ReadFile` bzw. `recv`, je nach Art der Kennung |
| 1 | `write` | `WriteFile` bzw. `send` |
| 2 | `open` | `CreateFileW` (Pfad nach UTF-16, Fahnen uebersetzt) |
| 3 | `close` | `CloseHandle` bzw. `closesocket` |
| 8 | `lseek` | `SetFilePointerEx` |
| 9 | `mmap` | `VirtualAlloc(MEM_COMMIT\|MEM_RESERVE)` |
| 10 | `mprotect` | `VirtualProtect` |
| 11 | `munmap` | `VirtualFree(MEM_RELEASE)`, sonst `MEM_DECOMMIT` |
| 21 | `access` | `GetFileAttributesW` |
| 24 | `sched_yield` | `SwitchToThread` |
| 32/33 | `dup`/`dup2` | `DuplicateHandle` |
| 35 | `nanosleep` | `Sleep` |
| 39 | `getpid` | `GetCurrentProcessId` |
| 41 | `socket` | `WSAStartup` einmalig, dann `socket` |
| 42 | `connect` | `connect` |
| 43/288 | `accept`/`accept4` | `accept` |
| 44 | `sendto` | `send` (nur die verbundene Form) |
| 45 | `recvfrom` | `recv` (nur die verbundene Form) |
| 48 | `shutdown` | `shutdown` (0/1/2 heissen dasselbe) |
| 49 | `bind` | `bind` |
| 50 | `listen` | `listen` |
| 51 | `getsockname` | `getsockname` |
| 54 | `setsockopt` | `setsockopt`, mit uebersetzten Ebenen und Namen (2.5) |
| 60/231 | `exit`/`exit_group` | `ExitProcess` |
| 74 | `fsync` | `FlushFileBuffers` |
| 79 | `getcwd` | `GetCurrentDirectoryW` |
| 87 | `unlink` | `DeleteFileW` |
| 158 | `arch_prctl(ARCH_SET_FS)` | nichts — Windows fuehrt seinen eigenen Fadenblock |
| 186 | `gettid` | `GetCurrentThreadId` |
| 228 | `clock_gettime` | `QueryPerformanceCounter` bzw. `GetSystemTimeAsFileTime` |
| 257 | `openat(AT_FDCWD, …)` | wie 2 |
| 318 | `getrandom` | `SystemFunction036` (das ist `RtlGenRandom`) |

Alles andere antwortet **`-38` (`ENOSYS`)** — sichtbar falsch statt leise
falsch.

**Die Naht ist in Firn geschrieben** (`compiler/src/win_seam.rs`, 1 073
Zeilen, davon etwa 620 Zeilen Firn-Quelltext) und wird in die
Uebersetzungseinheit eingespeist wie der Quelltext von `comptime` (Runde 35)
und dem Testlaeufer (Runde 94): gelext, zerteilt, angehaengt, danach vom
Typpruefer nicht mehr von Handgeschriebenem zu unterscheiden. Das ist eine
bewusste Wahl: die Naht ist Datenstrukturarbeit — Kennungstabelle,
UTF-16-Umsetzung, Fehlerabbildung — und genau darin ist Assembler am
schlechtesten.

Eine Stelle bleibt Assembler und muss es bleiben: `panic_rt.rs` schreibt
seine Meldung mit einem Rahmen, den es selbst gebaut hat, und mit den
Panikwerten in selbstgewaehlten Registern. Dort steht auf Windows statt der
`syscall`-Anweisung ein `call _Fwin.syscall` — ein Stummel, der genau die
Linux-Systemaufruf-Registerbelegung annimmt und an die Naht weiterreicht.
**Gepruefte Behauptung:** in keinem der 314 Windows-Bauten des Korpus steht
noch eine `syscall`-Anweisung (`tools/windows/machine.sh`, Abschnitt 3b).

### 2.3 Pfade sind UTF-16 — der erste harte Unterschied

Ein Linux-Pfad ist eine nullterminierte Oktettkette; ein Windows-Pfad ist
eine nullterminierte `u16`-Kette mit Laufwerksbuchstaben und Rueckstrichen.
`__win_u8_to_u16` setzt um (mit Ersatzpaaren fuer alles ueber U+FFFF) und
macht dabei aus `/` ein `\`, damit ein Firn-Programm, das `"tmp/x.txt"`
schreibt, dieselbe Datei findet wie auf Linux.

**Was es NICHT tut: einen Laufwerksbuchstaben erfinden.** `/etc/passwd` wird
zu `\etc\passwd` auf dem aktuellen Laufwerk, und das ist eine andere Datei
als die auf Linux. **Absolute Linux-Pfade ueberstehen den Uebergang nicht**,
und nichts hier tut so, als waere es anders. Relative Pfade und
Windows-Pfade mit Laufwerksbuchstaben (`C:/x/y` wird zu `C:\x\y`)
funktionieren.

Die Rueckrichtung (`__win_u16_to_u8`) braucht man fuer `GetCommandLineW` und
`GetCurrentDirectoryW`; `getcwd` gibt Schraegstriche zurueck, damit ein
Programm den Pfad wiedererkennt.

### 2.4 Steckdosen sind keine Dateikennungen — der zweite harte Unterschied

Auf Windows ist ein `SOCKET` eine eigene Art von Kennung. `ReadFile` darauf
geht nicht, `recv` auf einer Dateikennung geht nicht, und `CloseHandle` und
`closesocket` sind zwei verschiedene Funktionen. Linux-Code sagt fuer beides
`read(fd)`.

Die Naht fuehrt darum eine **Kennungstabelle**: `fd` bleibt die kleine ganze
Zahl, die Linux-Code erwartet, und die Tabelle merkt sich, ob ein HANDLE, ein
SOCKET oder eine nur hier vorhandene Datei (2.6) dahinter steht. `0`, `1`, `2`
werden beim Start aus `GetStdHandle` gefuellt. 256 Plaetze; ein voller Tisch
antwortet `-24` (`EMFILE`).

`struct sockaddr_in` hat auf beiden Seiten dieselben sechzehn Oktett und
`AF_INET` ist auf beiden `2` — die Adresse geht deshalb unveraendert durch.
Das ist gemessen und nicht geglaubt (`tools/windows/net.sh`).

### 2.5 Fehler und Optionen

`GetLastError` bzw. `WSAGetLastError` statt `errno`, abgebildet auf die
negativen Linux-Nummern, auf die die Grundbibliothek ohnehin prueft
(`r < 0 && r > -4096`): `ERROR_FILE_NOT_FOUND` → `-2`,
`ERROR_ACCESS_DENIED` → `-13`, `ERROR_BROKEN_PIPE` → `-32`,
`WSAECONNREFUSED` → `-111`, `WSAETIMEDOUT` → `-110`, und so weiter; alles
Unbekannte wird `-5` (`EIO`).

Bei `setsockopt` reicht das Uebersetzen der Nummern nicht: `SOL_SOCKET` ist
auf Linux `1` und auf Windows `0xFFFF`, `SO_REUSEADDR` dort `2` und hier `4`.
Und **eine Option hat einen anderen WERT und nicht nur eine andere Nummer**:
`SO_RCVTIMEO`/`SO_SNDTIMEO` nehmen auf Linux ein `struct timeval` aus zwei
Woertern, auf Windows eine einzelne DWORD in Millisekunden. Wer das
durchreicht, setzt eine Zeitgrenze in der Groesse des Sekundenfeldes — leise
falsch, die schlimmste Sorte. Die Naht rechnet um.

### 2.6 `/proc` gibt es nicht — und Wine macht es schlimmer

Das ist der Fund dieser Runde, und er hat 35 der 46 anfangs scheiternden
Faelle erklaert.

Der Sammler (`lib/gc/gc.fi`, `__gc_stack_bottom_maps`) braucht die
**Stapelgrenzen** und liest sie auf Linux aus `/proc/self/maps`. Windows hat
kein `/proc`. „Dann schlaegt `open` eben fehl" waere hier die falsche
Antwort: `gc_init` gibt dann `false` zurueck, und **jedes Programm mit einer
`gc class`** hoert auf zu arbeiten.

Schlimmer noch — unter Wine schlaegt es gar nicht fehl. Wine bildet das
Laufwerk `Z:` auf die Wurzel des Wirtsystems ab, also oeffnet
`\proc\self\maps` wirklich die **Linux**-Datei. Der Sammler liest dann die
Grenzen einer Linux-Abbildung, laeuft vom Windows-Stapelzeiger aus darueber
hinaus und der Prozess stirbt an einem Seitenfehler. Genau das wurde
gemessen, bevor es die Behebung gab.

Die Naht beantwortet die Datei jetzt **selbst**, aus
`GetCurrentThreadStackLimits` (Windows 8 und neuer), in genau der Form, die
der Sammler zerteilt:

```
21f000-330000 rw-p 00000000 00:00 0 [stack]
```

Jeder andere Name unter `/proc/` ist ein sauberes `ENOENT` — statt dessen,
was Wines Laufwerk `Z:` finden wuerde.

### 2.7 Der Einsprungpunkt und `argv`

Auf Linux gibt `_start` den anfaenglichen Stapelzeiger als erstes Argument an
`main` weiter, damit ein Programm `argc`/`argv` erreicht
(`docs/SELF_HOSTING.md` §2). Windows uebergibt gar nichts: die Befehlszeile
ist eine einzige UTF-16-Kette hinter `GetCommandLineW`.

Also baut die Naht denselben Block, den Linux auf den Stapel gelegt haette —
`[argc][argv0]…[0][envp…][0]` — und gibt seine Adresse zurueck; `main` merkt
keinen Unterschied. Zerlegt wird nach den einfachen Regeln (Anfuehrungszeichen
gruppieren, Leerzeichen trennen). Das ist mitgemessen:
`tools/windows/net.fi` holt seine Portnummer aus `argv[1]`.

---

## 3. Was gemessen wurde

### 3.1 Die zwei Programme

```
$ firnc --target=x86_64-windows examples/hello.fi -o hello.exe
$ wine hello.exe
Hallo Welt aus Firn!                       Ausgangswert 0

$ firnc --target=x86_64-windows examples/tour.fi -o tour.exe
$ wine tour.exe
hello, Firn -- dist2 25, dist 5, box 12, sum 10      Ausgangswert 0
```

`tour.fi` ist der interessantere der beiden: Zeichenketten, der
**Sammler**, `std.io`, `std.math`, eine Schnittstelle mit `impl`,
Zeichenketten-Verkettung auf dem GC-Haufen und formatierte Ausgabe. Die
Zeile ist zeichengleich mit der des Linux-Baus.

Dazu die Panik: ein `i64`-Ueberlauf schreibt auf Windows dieselbe Meldung
nach Kennung 2 und endet mit demselben Ausgangswert 101 wie auf Linux —
durch die Naht, nicht durch eine `syscall`-Anweisung.

### 3.2 Die Testsammlung (`tools/windows/run.sh`)

Jeder Fall aus `tests/*.fi` zweimal gebaut, der Linux-Bau nativ gelaufen, der
Windows-Bau unter Wine, Standardausgabe zeichenweise und Ausgangswert
verglichen.

| | |
|---|---:|
| **SAME** (gleiche Ausgabe, gleicher Ausgangswert) | **299** |
| DIFFERENT | **5** |
| NOT SUPPORTED (vom Uebersetzer mit Grund abgelehnt) | 0 |
| Linux erfuellt seine eigene Erwartung nicht | 5 |
| vergleichbare Faelle | **304** |
| **Ergebnis** | **299 von 304 (98 %)** |

**Die fuenf, die nicht laufen, nach Ursache gruppiert** — diese Liste ist
wertvoller als die Quote:

| Ursache | Faelle | warum |
|---|---:|---|
| **FAEDEN** (`clone(2)`) | 4 | `tests/860_thread_basic`, `861_thread_gc`, `862_thread_local`, `1600_net_echo` |
| **PROZESSE** (`fork`/`execve`/`wait4`) | 1 | `tests/700_process_start` |

Mehr Gruppen gibt es nicht. `1600_net_echo` ist der lehrreichste Fall: er
scheitert bei `thread_start` und **nicht** an den Steckdosen — die
Horchstelle wird geoeffnet, `bind`, `listen` und `getsockname` liefern den
richtigen Port, und erst der Bedienfaden fehlt.

**Die fuenf `Linux erfuellt seine eigene Erwartung nicht`** sind nicht neu und
nicht diese Runde: `028_cast_narrow`, `030_wrap_u8`, `054_i16_ops`,
`1334b_type_truncation` sind dieselben vier, die schon
`docs/ROUND-ARM-FREESTANDING.md` §1.1 als „x86 already failing" auffuehrt;
`834_arc_thread` misst ein Zeitverhalten, das unter sechs gleichzeitigen
Faellen auf diesem Rechner nicht zustande kommt. Wo eine Seite ihre eigene
Erwartung nicht erfuellt, gibt es nichts zu vergleichen, und das Werkzeug
sagt das, statt den Fall als Erfolg zu zaehlen.

### 3.3 Das Netzprogramm (`tools/windows/net.sh`)

Ein TCP-Klient (`tools/windows/net.fi`) mit nichts unter sich als `syscall`,
gegen einen Bediener mit **fester** Antwort, damit der Vergleich ein
Vergleich ist und keine Messung fremder Kopfzeilen. Fuenf Pruefungen, alle
bestanden:

```
  OK    linux: 1032 FIRN-OK
  OK    windows: die .exe wurde gebaut
  OK    windows: das Bild fuehrt WS2_32.dll wirklich in der Importtabelle
  OK    windows unter wine: 1032 FIRN-OK
  OK    beide Betriebssysteme sagen dasselbe
```

Damit ist zweierlei belegt: die Naht traegt mehr als `printf` (Steckdose,
Verbindung, Senden, Empfangen bis zum Dateiende, Schliessen — durch
`ws2_32.dll`), und der Startblock aus `GetCommandLineW` ist derselbe Block,
den Linux auf den Stapel legt (der Port kommt aus `argv[1]`).

### 3.4 Das Bild, nicht das Verhalten (`tools/windows/machine.sh`)

25 Pruefungen, alle bestanden. Sie fragen, was die DATEI ist — jede davon
kann falsch sein, waehrend unter Wine alles laeuft, und jede waere dann ein
Absturz auf einem echten Windows: PE32+, `.idata` vorhanden, Bindung an
`KERNEL32.dll`, kein CRT-Symbol, eigener `_start`; die Reihenfolge der
Bewegungen im Zwischenstueck, der Schattenplatz, der Aufruf durch die
Adresstabelle; die Sondierung mit Gegenprobe; `.bss` ohne Platz in der Datei;
**und der Durchlauf durch das ganze Korpus, der keine einzige
`syscall`-Anweisung mehr findet**.

### 3.5 Was unter Wine gemessen wurde — und was das nicht beweist

Alles Windows-Seitige dieser Runde lief unter **Wine 8.0 (Debian
8.0~repack-4)** auf Linux. Es gab keinen Windows-Rechner.

**Was Wine gut beweist:** das PE-Bild wird von einem Lader angenommen, der
nicht unserer ist; die Importtabelle wird aufgeloest; der Aufrufvertrag
stimmt (Wine ruft echte, kompilierte Win64-Funktionen — ein falsch gelegtes
Argument oder ein fehlender Schattenplatz faellt sofort auf, und ist in
dieser Runde auch aufgefallen: die erste Fassung stand um ein Wort daneben
und starb in `ExitProcess`); die Naht liefert die richtigen Werte; `ws2_32`
spricht mit einem echten Bediener.

**Was Wine NICHT beweist:**

* **Die Wache am Stapel.** Wine legt seine Stapel anders an als Windows. Dass
  ein Rahmen von 60 000 Oktett unter Wine laeuft, sagt wenig darueber, ob die
  Sondierung auf einem echten Windows das Richtige tut — sie ist nach der
  dokumentierten Regel geschrieben, aber nicht dort gemessen.
* **`/proc` gibt es unter Wine (2.6).** Auf einem echten Windows wuerde ein
  `open("/proc/self/maps")` fehlschlagen; hier oeffnet es die Linux-Datei.
  Die Naht faengt den Pfad jetzt VOR `CreateFileW` ab, sodass dieser
  Unterschied nicht mehr durchschlaegt — aber gemessen wurde nur die
  abgefangene Fassung, und nur unter Wine.
* **`GetCurrentThreadStackLimits`** gibt es erst ab Windows 8. Wine hat es;
  ein Windows 7 haette es nicht, und dann faellt der Sammler wieder aus.
* **Zeitverhalten, Konsolenverhalten, Zeichensatz der Konsole,
  Dateisperren, Rechte, lange Pfade (`\\?\`)** — nichts davon ist gemessen.
* **Absturzberichte und Aufrollen** koennen ohne `.pdata` ohnehin nicht
  funktionieren (1.4), auch nicht unter Wine.

### 3.6 Die Linux-Seite: unveraendert

Das ist die Bedingung, unter der die Runde ueberhaupt stattfinden durfte, und
sie ist auf drei Arten geprueft — **vorher und nachher gemessen**.

| | vorher | nachher |
|---|---|---|
| `cargo test --release` (Modultests des Uebersetzers) | **270 bestanden, 0 gescheitert** | **281 bestanden, 0 gescheitert** (11 neue dieser Runde) |
| `--emit=asm` ueber `tests/` + `examples/`, Datei gegen Datei | — | **314 von 314 zeichengleich, 0 unterschiedlich** |
| `tools/packages/run.sh`, 39 Faelle | 22 ok / 17 ERROR | 22 ok / 17 ERROR, **Zeile fuer Zeile dieselben** |

Zur mittleren Zeile: der Assemblertext-Vergleich ist das schaerfste
Werkzeug, das es hier gibt, und er wurde **dreimal** gefahren — nach dem
Zielmodell, nach dem Umbau des Panikpfades und nach der letzten Aenderung.
Jedes Mal 314/314. Der Uebersetzer vor dieser Runde ist der Bau des
Basiszweigs `main` (`2a20c514b`) in einem eigenen Arbeitsbaum.

Zur unteren Zeile, und das gehoert benannt: **`tools/packages/run.sh` ist auf
dem Basiszweig `main` nicht gruen.** 17 der 39 Faelle scheitern, alle auf der
Seite des selbstgehosteten Uebersetzers (`.firnc1` beendet sich bei
`--package` schweigend mit 2, waehrend `--package-info` funktioniert). Das
wurde auf einem **unberuehrten** Arbeitsbaum desselben Standes nachgemessen
(`/root/mg-firn`) und ist zeichengleich dasselbe Ergebnis. Der Auftrag dieser
Runde nannte 39 bestandene Faelle; auf diesem Stand sind es 22, vorher wie
nachher. Diese Runde hat daran nichts geaendert und behebt es auch nicht —
sie sagt nur, was sie vorgefunden hat.

---

## 4. Offene Punkte

### 4.1 Faeden (`clone(2)`)

`Op::ThreadSpawn` antwortet auf Windows `-38` (`ENOSYS`). **Nicht** ein
Uebersetzungsfehler, und das ist gemessen begruendet: `lib/gc/gc.fi`
ENTHAELT einen Fadenstart, also wuerde eine harte Ablehnung jedes Programm
treffen, das den Sammler bindet — in einem Zwischenstand waren das **93 von
309 Faellen**, von denen fast keiner je einen Faden startet. Ein Programm,
das nie einen startet, erreicht die Anweisung nie; eines, das es tut, bekommt
einen lesbaren Fehlschlag an der Stelle, an der es ihn ausloest.

Was fehlt: `CreateThread` gibt dem Kind einen eigenen Stapel und eine andere
Einsprungkonvention, und die Fadentabelle des Sammlers ist auf die
Linux-Form gebaut. Dazu `futex` — auf Windows 8 und neuer waeren
`WaitOnAddress`/`WakeByAddressSingle` die naheliegende Entsprechung.

### 4.2 Prozesse (`fork`/`execve`/`wait4`)

Antworten `-38`. `CreateProcessW` hat eine andere Form (kein `fork`, keine
Trennung von Erzeugen und Ersetzen), und `posix_spawn`-artig nachzubauen ist
eine eigene Runde.

### 4.3 Weiteres

* **`.pdata`/`.xdata`** — siehe 1.4.
* **Rueckrufe.** Eine Firn-Funktion kann Windows nicht als Rueckruf
  uebergeben werden (das braucht das Spiegel-Zwischenstueck Win64→System V).
  Fuer ein GDI-Fenster (Abschnitt 5) ist das die erste zu schliessende Luecke:
  eine Fensterprozedur IST ein Rueckruf.
* **Gleitkomma in `extern fn`.** Die Zwischenstuecke sortieren nur
  ganzzahlige Argumente um; Win64 legt Gleitkommaargumente nach POSITION in
  `xmm0`–`xmm3`, System V nach Gleitkomma-INDEX in `xmm0`–`xmm7`. Keine der
  42 gebrauchten Win32-Funktionen hat ein Gleitkommaargument; eine, die eines
  haette, wuerde falsch gerufen. Das ist eine bekannte Luecke, keine
  geprueffte Ablehnung.
* **Fehlerinformation fuer den Fehlersucher.** DWARF ist auf dem
  Windows-Ziel **aus**: `dwarf_info.rs` schreibt Abschnittsfahnen in
  ELF-Schreibweise (`,"",@progbits`), die der COFF-Uebersetzer nicht nimmt.
  Kein `.loc`, kein `.debug_info`. Behebbar, aber nicht in dieser Runde.
* **Jedes Windows-Programm bindet alle drei DLLs**, auch wenn es nur
  schreibt — die Naht nennt sie alle, und der Binder raeumt unbenutzte
  Zwischenstuecke nicht weg. `hello.exe` fuehrt darum `ws2_32.dll` in seiner
  Importtabelle. Kosmetisch, aber es steht hier.
* **Der Zerteiler in der Naht** ist eine Kette von `if nr == …` — bis zu 35
  Vergleiche je Systemaufruf. Fuer ein Programm, das viel schreibt, ist das
  messbar. Eine Sprungtabelle waere die offensichtliche naechste Stufe.
* **`munmap` eines TEILS einer Abbildung.** `VirtualFree(MEM_RELEASE)`
  verlangt die Basisadresse; schlaegt es fehl, versucht die Naht
  `MEM_DECOMMIT`. Das ist nicht dasselbe wie Linux und kann bei einem
  Zuteiler, der Stuecke einzeln zurueckgibt, Adressraum liegen lassen.

### 4.4 Der selbstgehostete Uebersetzer wurde NICHT mitgezogen

Das sei klar gesagt: **`lib/firnc1/*.fi` kennt das Ziel `x86_64-windows`
nicht.** `firnc1 --target=x86_64-windows` lehnt den Namen ab. Die harte
Projektregel (beide Uebersetzerseiten liefern zeichengleiche Ausgaben) ist
damit fuer dieses Ziel nicht verletzt, sondern noch nicht anwendbar: es gibt
auf der `firnc1`-Seite nichts, was abweichen koennte.

Was dort fehlt, damit `firnc1` nachziehen kann:

1. **Das Zielmodell.** `lib/firnc1/` braucht den dritten Wert auf der
   zweiten Achse und die zugehoerigen Werkzeugnamen (`target.rs` ist auf der
   Rust-Seite 380 Zeilen; die Firn-Entsprechung ist kleiner).
2. **Die Zwischenstuecke, die Importtabelle und die Sondierung** — das ist
   `compiler/src/win.rs`, 539 Zeilen, davon etwa 300 Zeilen erzeugter
   Assemblertext. Reine Textausgabe, gut uebertragbar.
3. **Die Naht selbst braucht gar nicht uebertragen zu werden**: sie IST
   Firn-Quelltext (`win_seam.rs` haelt ihn nur als Zeichenkette). `firnc1`
   muesste ihn einspeisen koennen — es hat mit dem Testlaeufer schon eine
   Stelle, die Quelltext waehrend der Uebersetzung anhaengt.
4. **`Op::Syscall` und der Rahmenaufbau** im Codeerzeuger von `firnc1`, je
   eine Fallunterscheidung.
5. **Der Panikpfad** (`lib/firnc1/`-Gegenstueck zu `panic_rt.rs`): dieselbe
   Ersetzung der `syscall`-Anweisung durch den Stummel.

Danach muss `tools/fir_compare.sh` und der Assemblertext-Vergleich fuer das
neue Ziel genauso gefahren werden wie fuer die bestehenden.

---

## 5. Die ehrliche Einschaetzung fuer Certus

Der Projekteigner will Certus als **Hauptbrowser auf Windows** benutzen —
nicht als Spielerei, sondern damit seine Daten mit seinen eigenen Geraeten
abgeglichen sind und kein Chromium mitliest. Was fehlt nach dieser Runde
dafuer noch?

### 5.1 Was jetzt schon traegt

Der **Motor** von Certus (90 155 Zeilen Firn nach `docs/CERTUS-STATUS.md`:
JavaScript, HTML, CSS, Layout, Malen, Schriften, Netz, TLS) ruft selbst nur
**sieben** Systemnummern — `read`, `write`, `open`, `close`, `mmap`,
`munmap`, `clock_gettime` — und alle sieben sind abgebildet und gemessen.
Was er ueber `std.net` und `std.rt` zusaetzlich braucht (`socket`,
`connect`, `send`/`recv`, `setsockopt`, `getsockname`, `getrandom`), ist
ebenfalls abgebildet; `tests/1600_net_echo` zeigt, dass davon alles ausser
dem Bedienfaden funktioniert.

Drei Dinge, die man haette fuerchten koennen und die kein Problem sind:

* **Der Sammler laeuft** (Abschnitt 2.6). Ohne die Stapelgrenzen waere
  Certus auf Windows gar nicht gestartet — der ganze DOM- und JS-Baum haengt
  am GC.
* **DNS geht ueber TCP.** `lib/net/dns.fi` schreibt ausdruecklich, dass es
  keinen neuen Systemaufruf braucht (RFC 1035 4.2.2); `sendto`/`recvfrom`
  mit Adressstruktur — die eine Sache, die die Naht nur in der verbundenen
  Form kann — wird von Certus also nicht gebraucht.
* **Certus ist einfaedig.** Weder `lib/browser/window_main.fi` noch
  `b5_main.fi` starten einen Faden. Die groesste Luecke dieser Runde
  (Abschnitt 4.1) trifft den Browser nicht.

### 5.2 Das Fenster: wie tief X11 wirklich sitzt

Nachgesehen, nicht geschaetzt:

| | |
|---|---:|
| `lib/browser/x11.fi` — der ganze X11-Klient ueber einen Unix-Socket | **702 Zeilen** |
| `lib/browser/window_main.fi` — Fenster, Adressleiste, Ereignisschleife | **706 Zeilen** |
| Dateien im ganzen Baum, die X11 ueberhaupt beruehren | **2** (genau diese beiden) |
| Aufrufstellen von `x11.*` in `window_main.fi` | **27** |
| Namen an der Grenze (`export {}` von `x11.fi`) | **18 Funktionen + 6 Ereigniskonstanten** |

**Das ist die gute Nachricht dieser Untersuchung: X11 sitzt nicht tief.** Es
sitzt in genau einer Datei, hinter genau einer Schnittstelle, und die
Zeichnung laeuft ueber `x11_put_image` — einen fertigen BGRX-Bildpunktpuffer.
Der Motor weiss von X11 nichts; `paint.canvas` malt in einen Speicherbereich,
und das Fenster schiebt ihn nur weiter.

Die achtzehn Namen und ihre GDI-Entsprechung:

| X11 | Windows |
|---|---|
| `x11_open`, `x11_create_window`, `x11_map` | `RegisterClassW` + `CreateWindowExW` + `ShowWindow` |
| `x11_put_image` (BGRX-Puffer) | `StretchDIBits` bzw. `SetDIBitsToDevice` auf den Fenster-DC |
| `x11_fill` (Rechteck) | `FillRect` — oder gar nicht: in den Puffer malen |
| `x11_text`, `x11_open_font` | `CreateFontW` + `TextOutW` — **oder** mit Certus' eigenem TrueType-Rasterer (`lib/font`, 1 910 Zeilen, gibt es schon) in den Puffer malen |
| `x11_next_event`, `x11_pending` | `PeekMessageW`/`GetMessageW` + `DispatchMessageW` |
| `x11_width`, `x11_height` | `GetClientRect` bzw. `WM_SIZE` |
| `x11_set_title` | `SetWindowTextW` |
| `x11_key_ascii` | `WM_CHAR` — auf Windows sogar einfacher als `GetKeyboardMapping` |
| `x11_close`, `x11_ok`, `x11_flush`, `x11_new` | `DestroyWindow`, Zustand, `UpdateWindow` |

**Aufwandsschaetzung:** eine Datei `lib/browser/gdi.fi` in der
Groessenordnung von `x11.fi`, also **600 bis 800 Zeilen Firn**, plus eine
Fallunterscheidung in `window_main.fi` (etwa 30 Zeilen, weil es 27
Aufrufstellen mit gleicher Signatur sind). Wenn man die Schriftausgabe der
Fensterleiste dem eigenen Rasterer ueberlaesst — was ohnehin konsistenter
aussieht als eine GDI-Schrift neben Certus' eigener —, schrumpft die
GDI-Flaeche auf **fuenf** echte Aufgaben: Fenster erzeugen, Puffer blitten,
Ereignisse holen, Titel setzen, schliessen.

**Was dafuer im Uebersetzer noch fehlt**, und das ist der eigentliche
Blocker: **eine Fensterprozedur ist ein Rueckruf.** `CreateWindowExW`
erwartet einen Zeiger auf eine Funktion, die Windows nach **Win64** ruft.
Firn kann heute nach draussen rufen, aber nicht von draussen gerufen werden
(Abschnitt 4.3). Das ist das Spiegel-Zwischenstueck: eine Funktion, die
Win64-Argumente annimmt (`rcx, rdx, r8, r9`, Schattenplatz vorhanden), sie
nach System V umsortiert und die Firn-Funktion ruft — **strukturell dieselbe
Arbeit wie das Zwischenstueck, das es schon gibt, in der anderen Richtung**,
und mit der zusaetzlichen Pflicht, `rsi`, `rdi` und `xmm6`–`xmm15` zu
bewahren, weil Windows das vom Gerufenen verlangt. Schaetzung: **50 bis 80
Zeilen in `win.rs` plus ein Attribut** (`#[win64]` oder ein Anhaengsel an
`#[export_c]`), damit der Uebersetzer weiss, fuer welche Funktion er es
schreiben soll.

Es gibt einen Ausweg ohne Rueckruf, und er ist einen Absatz wert, weil er die
erste lauffaehige Fassung stark beschleunigen wuerde: eine Fensterklasse mit
`DefWindowProcW` als Prozedur (ein Zeiger, den `GetProcAddress` liefert) und
die ganze Arbeit in der Nachrichtenschleife, die `GetMessageW` selbst dreht.
Damit braeuchte die erste Fassung **gar keinen Rueckruf** — nur `WM_PAINT`
liesse sich so nicht sauber bedienen, und dafuer kann man nach jeder
Nachricht schlicht neu blitten.

### 5.3 Was ausserdem noch fehlt

* **Pfade.** Certus bekommt den Wurzelspeicher und die Schriftdatei als
  Befehlszeilenargument, und `window_main.fi` traegt fest
  `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` als Vorgabe. Auf Windows
  muss das ein Windows-Pfad sein (`C:/Windows/Fonts/segoeui.ttf`), und der
  Wurzelspeicher muesste entweder mitgeliefert oder aus dem Windows-Speicher
  geholt werden (`CertOpenSystemStoreW` — eine weitere DLL). Kleine Arbeit,
  aber sie muss jemand machen.
* **`.pdata`.** Fuer einen Browser, der abstuerzen kann, ist ein
  unbrauchbarer Absturzbericht laestig. Kein Blocker, aber das erste, was man
  vermisst.
* **Hoher DPI, Fenstergroessenaenderung, Zwischenablage, Mausrad** — alles
  Fensterarbeit, alles in `gdi.fi`.
* **Faeden** werden von Certus heute nicht gebraucht (5.1), aber sobald das
  Laden von Bildern oder das Netz nebenlaeufig werden soll, kommt 4.1 zurueck.

### 5.4 Die Reihenfolge

1. **Rueckrufe (Win64 → System V).** 50–80 Zeilen. Ohne sie kein Fenster —
   und mit ihnen ist auch jede andere Win32-Schnittstelle erreichbar, die
   einen Funktionszeiger will.
2. **`lib/browser/gdi.fi`**, die achtzehn Namen hinter derselben
   Schnittstelle wie `x11.fi`, mit dem eigenen Rasterer fuer die Schrift.
   600–800 Zeilen. Danach laeuft Certus als Fenster auf Windows.
3. **Pfade und Wurzelspeicher** (5.3), damit es ohne Handarbeit startet.
4. **`.pdata`/`.xdata`**, damit ein Absturz einen Bericht ergibt.
5. **Faeden** (`CreateThread` + `WaitOnAddress`), sobald Nebenlaeufigkeit
   gebraucht wird — und erst dann.
6. **`firnc1` nachziehen** (4.4), damit die Projektregel wieder fuer alle
   Ziele gilt.
7. **Prozesse** (`CreateProcessW`) — Certus braucht sie nicht; das ist
   Bibliotheksvollstaendigkeit.

Punkte 1 und 2 zusammen sind der ganze Weg zu einem Certus-Fenster auf
Windows, und beide sind der Groesse nach eine Runde, keine drei.

---

## 6. Was in dieser Runde entstanden ist

| Datei | Zeilen | was |
|---|---:|---|
| `compiler/src/win.rs` | 539 | Importtabelle, Zwischenstuecke, Sondierung, Einsprungpunkt, Systemaufruf-Stummel |
| `compiler/src/win_seam.rs` | 1 073 | die Naht (Firn-Quelltext) und ihre Selbstpruefungen |
| `tools/windows/run.sh` | 214 | der Kreuzvergleich mit den Ursachengruppen |
| `tools/windows/machine.sh` | 156 | das Bild statt des Verhaltens, 25 Pruefungen |
| `tools/windows/net.sh` + `net.fi` | 237 | der Netzbeweis |
| `tools/windows/causes.txt`, `minquota.txt` | 25 | die Ursachenliste und der Boden der Quote |

Geaendert: `target.rs` (dritter Wert auf der zweiten Achse), `main.rs`
(Einspeisung der Naht, Binderaufruf, `.exe`-Endung), `codegen_x86.rs`
(Einsprungpunkt, `Op::Syscall`, Rahmen, Symbol einer `extern fn`),
`regalloc.rs` (dasselbe auf dem registerbewussten Pfad), `panic_rt.rs`
(der Stummel), `statics.rs` (COFF kennt `@nobits` nicht), `dwarf.rs`
(aus auf Windows), `thread.rs` (`ENOSYS` statt `clone`), `SPEC.md` §2.2 und
§13, `README.md`, `--help`.

---

## 7. Kurzfassung

* **`firnc --target=x86_64-windows` baut `.exe`-Dateien, die laufen.**
  PE/COFF mit einer **selbstgeschriebenen Importtabelle** (keine
  Importbibliothek, kein C-Anlauf im Bild), Win64 an der Aussengrenze ueber
  ein Zwischenstueck je Importfunktion, Stapel-Sondierung ab einer Seite,
  und `syscall` beantwortet von einer **in Firn geschriebenen Naht** ueber 42
  Win32-Funktionen aus `kernel32`, `ws2_32` und `advapi32`.
* **299 von 304 vergleichbaren Testfaellen verhalten sich auf Linux und
  Windows identisch (98 %)**, gemessen unter Wine. Die fuenf Ausnahmen haben
  **genau zwei** Ursachen: Faeden (4) und Prozesse (1). `hello.exe`,
  `tour.exe` und ein TCP-Klient ueber `ws2_32` sagen zeichengleich dasselbe
  wie ihre Linux-Bauten.
* **Die Linux-Seite ist nachweislich unberuehrt**: 314 von 314 Programmen
  liefern zeichengleichen Assemblertext vor und nach der Runde, `cargo test`
  270 → 281 ohne einen Fehlschlag, `tools/packages/run.sh` Zeile fuer Zeile
  gleich (und auf dem Basiszweig ohnehin nur 22 von 39 gruen — vorher wie
  nachher).
* **Fuer Certus auf Windows fehlt vor allem eines: das Fenster.** X11 sitzt
  in genau zwei Dateien hinter achtzehn Namen; die Uebersetzung nach GDI sind
  600–800 Zeilen Firn — davor braucht es allerdings **Rueckrufe**
  (Win64 → System V, 50–80 Zeilen), weil eine Fensterprozedur einer ist.
  Der Motor selbst ruft nur sieben Systemnummern, und alle sieben laufen.
