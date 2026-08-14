# DESIGNZIELE.md — Was Firn von Anfang an anders machen soll

**Stand:** 2026-08-14 · **Bezug:** `SPEC.md` v0.2, `ROADMAP.md`, `ABNAHME.md`

Dieses Dokument sammelt bekannte Schwachstellen heutiger Systemsprachen und legt
für jede fest, wie Firn damit umgeht. **Es geht nicht darum, alles sofort zu
bauen** — sondern darum, nichts zu verbauen. Manche dieser Punkte lassen sich
später nachrüsten; andere sind Fundamentfragen, die man nach zehn Jahren Code
nicht mehr ändern kann, ohne alles aufzureißen. Die Trennung dieser beiden
Kategorien ist der eigentliche Zweck des Dokuments und steht in §10.

Aufbau je Punkt: **Problem** (mit realem Beispiel) · **Stand der Technik** ·
**Firns Ansatz** (technisch, keine Absichtserklärung) · **Konsequenz für Compiler
und Sprachkern heute** · **Priorität und Phase**.

Wo ein Ansatz teuer ist oder mit einem anderen Ziel kollidiert, steht der
Zielkonflikt ausdrücklich dabei. Das ist wichtiger als jede Lösungsidee.

---

## 1. Funktionsfarben — `async` zerreißt Ökosysteme

### Problem

Sobald eine Funktion `async` ist, muss jeder Aufrufer `async` sein. Die Farbe
frisst sich nach oben durch den gesamten Baum. Konkrete Folgen:

* **Rust:** Es gibt `std::io::Read` und `tokio::io::AsyncRead` — zwei
  inkompatible Ökosysteme für dieselbe Sache. Eine Bibliothek muss sich
  entscheiden oder beides doppelt pflegen (`feature = "async"`). Traits mit
  `async fn` waren jahrelang gar nicht möglich, und selbst heute ist
  `async fn` in Traits mit Einschränkungen behaftet. Zusätzlich färbt die
  *Laufzeit*: `tokio`-Code läuft nicht auf `async-std`.
* **JavaScript:** `await` geht nur in `async function`; alles andere braucht
  `.then()`. Die Sprache hat dadurch zwei Steuerflüsse.
* **Python:** `asyncio` hat ein zweites Universum an Bibliotheken erzeugt
  (`requests` ↔ `aiohttp`, `psycopg2` ↔ `asyncpg`).

Für den Browser ist das kein akademisches Problem: `fetch`, die Ereignisschleife,
Web Workers, `Promise`, Generatoren, Timer und Netzwerk-E/A sind alle
nebenläufig — und der Rasterizer, der Tokenizer und der Layout-Code sind es
nicht. Wenn die Sprache färbt, zerfällt die Engine in zwei Hälften.

### Stand der Technik

* **Rust/JS/Python:** färben. Zustandsmaschinen-Transformation im Compiler,
  Laufzeit in der Bibliothek. Beste Leistung, schlechteste Ergonomie.
* **Go:** färbt nicht — Goroutinen sind stapelvoll, jeder Aufruf darf blockieren.
  Preis: eine Laufzeit, die immer da ist, wachsende Stapel, GC-Zwang, kein
  freistehender Betrieb. Für einen Kernel unbrauchbar.
* **Zig 0.16 (April 2026):** der interessante dritte Weg. `Io` wird als
  **Parameter** übergeben, genau wie der `Allocator`. Aus den Release Notes:
  *„Starting with Zig 0.16.0, all input and output functionality requires being
  passed an `Io` instance."* Aus `file.close()` wird `file.close(io)`.
  `io.async(…)` erzeugt ein `Future(T)` und drückt **Unabhängigkeit** aus, nicht
  Nebenläufigkeit — deshalb ist es unfehlbar und läuft auch auf eingeschränkten
  `Io`-Umsetzungen. `io.concurrent(…)` verlangt echte Gleichzeitigkeit und darf
  mit `error.ConcurrencyUnavailable` scheitern. Umsetzungen: `Io.Threaded`
  (fertig), `Io.Evented` (experimentell, stapelvolle Koroutinen) mit den
  Rückenden `Io.Uring`, `Io.Kqueue`, `Io.Dispatch`, sowie `Io.failing` für Tests.
  **Die Farbe wandert aus der Sprache in das Typsystem** — sie ist immer noch da,
  aber sie ist ein gewöhnlicher Parameter und keine zweite Sprache.

### Firns Ansatz

**Zigs Modell wird übernommen — und zwar unverändert im Prinzip, weil es zu
Firns Leitsatz 1 („nichts passiert versteckt") und zu den bereits explizit
übergebenen Allokatoren exakt passt.** Firn hat damit **kein** `async`, **kein**
`await` als Schlüsselwort und **keine** Zustandsmaschinen-Transformation im
Compiler.

```firn
// Kein async. io ist ein Parameter wie alloc.
fn fetch(io: Io, url: &Str, alloc: inout Arena) -> NetError!Response {
    let conn = try io.connect(url.host, 443)
    defer conn.close(io)
    try conn.write(io, request_bytes)
    return try conn.read_all(io, inout alloc)
}

// Unabhängigkeit ausdrücken — nicht Nebenläufigkeit fordern:
fn load_page(io: Io, urls: &[Str], alloc: inout Arena) -> !Vec[Response] {
    var futs = Vec[Future[NetError!Response]].with_capacity(inout alloc, urls.len)
    var i: usize = 0
    while i < urls.len {
        futs.push(io.async(fetch, io, &urls[i], inout alloc))   // unfehlbar
        i = i + 1
    }
    defer futs.cancel_all(io)          // Abbruch ist Pflichtteil des Vertrags
    return futs.await_all(io)
}
```

Kernpunkte der Firn-Fassung:

1. **`Io` ist ein gewöhnlicher Schnittstellenwert** (`SPEC.md` §6.2,
   `dyn Io` mit Vtable oder statisch monomorphisiert). Keine Sprachmagie.
2. **`io.async` drückt Unabhängigkeit aus, nicht Gleichzeitigkeit** und ist
   unfehlbar. `io.concurrent` fordert Gleichzeitigkeit und darf scheitern. Diese
   Unterscheidung ist Zigs beste Einzelidee daran, weil sie denselben Code auf
   einem Einzelfaden-`Io` und auf einem io_uring-`Io` laufen lässt.
3. **`Future[T]` ist `#[must_consume]`** — wer ein Future fallen lässt, ohne
   `await` oder `cancel`, bekommt einen Übersetzungsfehler. Das schließt die
   häufigste Fehlerklasse von Rusts `async` (vergessene Futures, die nie laufen)
   von vornherein aus.
4. **Abbruch ist erstklassig, nicht nachgerüstet.** `cancel(io)` gehört zum
   Vertrag jedes `Future`. Genau das fehlt Rusts `async` bis heute schmerzhaft
   (Abbruchsicherheit ist dort eine ungeschriebene Regel, kein Typ).
5. **Umsetzungen im `app`-Profil:** `Io.Threaded` (Fäden, zuerst),
   `Io.SingleThread` (deterministisch, für reproduzierbare Reftests — erfüllt
   `N7`), später `Io.Evented` mit stapelvollen Koroutinen. Im `kernel`-Profil
   gibt es `Io` in einer eigenen Fassung, die auf karst-Primitive abbildet.
6. **Stapelvolle Koroutinen statt Zustandsmaschinen.** Das ist der Preis (siehe
   unten), aber es hält den Codegenerator klein und erlaubt, dass *jede*
   Funktion in einer Koroutine läuft — auch tief rekursive wie der HTML-
   Tree-Builder, den man als Zustandsmaschine nie transformieren wollte.

### Zielkonflikt — offen benannt

* **Stapelvolle Koroutinen kosten Speicher.** Jede braucht einen eigenen Stapel.
  Bei zehntausend gleichzeitigen Verbindungen ist das relevant; bei einem
  Browser mit einigen hundert gleichzeitigen Anfragen ist es unkritisch.
  Zustandsmaschinen wären speichersparsamer, aber sie färben. **Firn wählt
  Speicher über Ergonomie.**
* **`Io` durchreichen ist Fleißarbeit.** Jede Funktion, die irgendwo unten E/A
  macht, braucht den Parameter. Das ist ehrlich, aber lästig — dieselbe Kritik
  trifft Zigs Allokator-Modell. Gegenmittel: `Io` und `Allocator` werden
  häufig gemeinsam in einem `Ctx`-Struct gereicht.
* **Kollision mit `#[no_gc]` und Constant-Time:** eine Funktion, die `Io` nimmt,
  kann nicht `#[no_gc]` sein, wenn die `Io`-Umsetzung allokiert. Das ist
  gewollt und wird vom Prüfer erzwungen.

### Konsequenz für den Compiler HEUTE

Erstaunlich gering — **das ist das stärkste Argument für dieses Modell**:

* **Nichts im Codegenerator.** Kein `async`-Lowering, keine Zustandsmaschinen,
  keine Abwicklung über Koroutinen-Grenzen.
* Was gebraucht wird, wird ohnehin gebraucht: `interface` mit dynamischer
  Auflösung (`SPEC.md` §6.2), `#[must_consume]`, Generics.
* **Eine echte Fundamentanforderung:** stapelvolle Koroutinen brauchen
  **Stapelwechsel** — eine kleine Assembler-Routine, die `rsp`, `rbp` und die
  erhaltenen Register tauscht. Das ist wenig Code, verlangt aber, dass der
  Codegenerator **keine Annahmen über Stapelstetigkeit** trifft: keine
  roten Zonen ohne Not, keine Zeiger auf Stapelrahmen, die einen Wechsel
  überleben müssten. Firns zweitklassige Referenzen (`SPEC.md` §3.3) sind
  hier ein Glücksfall: eine Referenz kann ihren Aufrufrahmen ohnehin nicht
  verlassen.
* **Was heute schon falsch wäre:** ein `async`-Schlüsselwort einzuführen. Das
  passiert nicht.

### Priorität und Phase

**Fundament: nur die Stapelwechsel-Verträglichkeit** (kostenlos, wenn man es
weiß). **Umsetzung: ROADMAP Phase 3** (`Io.Threaded` mit der
Nebenläufigkeitsbibliothek), **Phase 4** (`Io.Evented`).
`SPEC.md` §7 wird entsprechend korrigiert: dort steht noch „kein `async`, `N6`
bleibt bewusst unerfüllt" — richtig ist ab jetzt „kein `async` **als
Sprachfarbe**; `N6` wird über `Io` erfüllt".

---

## 2. Fehlschlagende Allokation

### Problem

`Vec::push` in Rust **paniced**, wenn kein Speicher da ist. Für ein
Anwendungsprogramm ist das vertretbar; für einen Kernel, einen Browser-Tab mit
Speichergrenze oder ein eingebettetes Gerät ist es inakzeptabel.

Der Beleg ist Linux: Rust-for-Linux konnte `alloc` nicht benutzen, weil dort
jede Allokation abstürzen darf. Es brauchte `try_reserve` (stabilisiert erst
Rust 1.57, Ende 2021), einen eigenen Kernel-`Vec` mit `push_within_capacity`
und `try_push`, und bis heute ist die Kernel-Fassung eine Parallelwelt zur
Standardbibliothek. **Die Nachrüstung ist nie vollständig geworden**, weil der
Fehlschlagpfad im Typsystem fehlt: `Vec<T>` hat keine Stelle, an der ein Fehler
hindürfte.

Zig macht es richtig herum: `try list.append(alloc, x)` — der Fehler ist von
Anfang an im Typ. Genau deshalb ist Zig im Kernelbereich brauchbar.

### Stand der Technik

| Sprache | Verhalten |
|---|---|
| Rust `std` | Panik. `try_reserve` nachgerüstet, deckt nicht alles ab |
| Rust `no_std` + `alloc` | ebenso; Linux pflegt eine eigene Sammlungsbibliothek |
| C | `malloc` gibt `NULL` zurück — korrekt, aber niemand prüft es |
| C++ | `std::bad_alloc` als Ausnahme; in `-fno-exceptions`-Projekten (also fast allen Systemprojekten) unbrauchbar |
| **Zig** | **jede Allokation ist fehlbar, `Allocator` ist Parameter.** Vorbild |

### Firns Ansatz

**Jede Allokation ist fehlbar — von der ersten Zeile an, ohne Ausnahme.**

```firn
// Es gibt KEINE unfehlbare Allokationsfunktion in der Standardbibliothek.
fn build(alloc: inout Allocator) -> AllocError!Vec[u32] {
    var v = try Vec[u32].with_capacity(inout alloc, 16)
    try v.push(inout alloc, 42)        // fehlbar, sichtbar
    return v
}
```

Damit das **ergonomisch** bleibt — sonst schreibt es niemand und alle greifen zur
Abkürzung — kommen drei Mittel dazu:

1. **`try` ist ein Zeichen.** Wie in Zig. `try v.push(inout a, x)` ist genauso
   kurz wie `v.push(x)` plus vier Zeichen. Das ist der Unterschied zwischen
   „machbar" und „macht keiner".
2. **Kapazität vorab reservieren ist der empfohlene Stil.** `try
   v.reserve(inout a, n)` einmal, danach `v.push_within_capacity(x)` —
   **unfehlbar**, weil die Kapazität bewiesen ist. Der heiße Pfad (Tokenizer,
   Rasterizer) hat damit **null** Fehlerbehandlung im Innersten der Schleife.
   Das ist genau das, was Linux mit `push_within_capacity` nachgebaut hat —
   nur ist es hier von Anfang an der Normalweg.
3. **Arenen brauchen gar keine Fehlerbehandlung pro Element.** `Arena.alloc`
   schlägt nur fehl, wenn ein neuer Block gebraucht wird; wer die Arena vorher
   dimensioniert, hat einen unfehlbaren Pfad. Für Parser und Layout — die
   Hauptallokationsquellen des Browsers — ist das der Normalfall.

**Nicht behandelbare Allokationsfehler** gibt es trotzdem: Wenn der GC oder die
Laufzeit selbst keinen Speicher mehr bekommt, ist das eine Panik. Aber das ist
eine klar benannte, kleine Menge an Stellen — nicht jedes `push`.

**Zusammenspiel mit `app`-Profil und GC:** `Gc[T]`-Allokation ist **ebenfalls
fehlbar** (`AllocError!Gc[T]`). Ein Sammellauf wird vorher versucht; erst wenn
auch danach kein Speicher da ist, kommt der Fehler. Das ist die Grundlage für
Speichergrenzen pro Tab (`B13` aus den Browser-Anforderungen) — ohne fehlbare
GC-Allokation kann ein Tab, der zu viel will, nur den ganzen Browser töten.

### Zielkonflikt

* **Fehlerrauschen.** Jeder Aufruf mit `try` ist mehr Text. Gegenmittel siehe
  oben (Kapazität vorab); trotzdem bleibt Firn-Code an dieser Stelle länger als
  Rust-Code. **Bewusst akzeptiert.**
* **Kollision mit Operatoren.** Ein `+` auf einem Sammlungstyp, das allokieren
  müsste, kann es nicht — deshalb gibt es kein Operator-Overloading (steht schon
  in `SPEC.md` §4.5) und keine implizit allokierende Verkettung.
* **Kollision mit Punkt 6 (In-Place-Initialisierung):** ein fehlbarer
  Konstruktor, der am Zielort baut, muss bei Fehlschlag den halbfertigen Zustand
  aufräumen. Das ist genau die Schwierigkeit, die `try_pin_init!` in Linux löst.
  Firns Antwort steht in §6.

### Konsequenz für den Compiler HEUTE

* **Die Fehlerunion `!T` muss im Sprachkern sitzen** (`SPEC.md` §5.1) —
  ist bereits so vorgesehen, aber in Stufe 0 noch nicht gebaut.
* **`#[must_consume]` muss existieren**, sonst kann man `AllocError!T` still
  fallen lassen.
* **Die Standardbibliothek darf niemals eine unfehlbare Allokationsfunktion
  bekommen.** Das ist eine Regel für Phase 3 und die einzige Stelle, an der
  Disziplin wichtiger ist als Technik: Wenn `Vec.push(x)` ohne `try` je
  existiert, wird sie benutzt und der ganze Ansatz ist tot.
* Heute (Stufe 0) gibt es keine Allokation — es ist also **nichts verbaut**.

### Priorität und Phase

**FUNDAMENT.** Nachrüsten ist nachweislich nicht möglich (Linux hat es
versucht). `!T` + `#[must_consume]` in **Phase 2**, Standardbibliothek nach
dieser Regel in **Phase 3**.

---

## 3. Capability-basierte Module — Lieferketten-Sicherheit

### Problem

Heute darf **jede** eingebundene Bibliothek **alles**: Dateien lesen, Netz
öffnen, Prozesse starten, Umgebungsvariablen auslesen. Eine
Zeichenketten-Hilfsbibliothek hat dieselben Rechte wie der Netzwerkstapel.

Reale Vorfälle: `event-stream` (npm, 2018) — ein übernommenes Paket schleuste
Code zum Diebstahl von Bitcoin-Wallets ein. `ua-parser-js`, `coa`, `rc` (npm,
2021) — Kryptominer und Passwortdiebe in Paketen mit Millionen wöchentlicher
Downloads. `xz-utils` (2024) — eine über zwei Jahre aufgebaute Hintertür in
einer Kompressionsbibliothek, die über systemd in `sshd` landete. In allen
Fällen war die technische Voraussetzung dieselbe: **Der Code durfte Dinge tun,
die für seine Aufgabe nie nötig gewesen wären.**

Für einen Browser ist das die Kernfrage. Ein Bilddecoder verarbeitet feindliche
Eingaben aus dem Netz — er darf **niemals** eine Datei öffnen dürfen.

### Stand der Technik

* **Rust/Go/C++:** keinerlei Rechtemodell. `cargo` führt sogar `build.rs`
  beliebiger Pakete beim Bauen aus, mit voller Berechtigung.
* **Deno:** Rechte auf **Prozessebene** (`--allow-net`), nicht pro Modul. Besser
  als nichts, aber ein einziges bösartiges Modul erbt alle Rechte des Prozesses.
* **WASI / WebAssembly-Komponentenmodell:** capability-basiert und richtig
  gedacht — aber eine Sandbox um fremden Code, mit Marshalling-Kosten an jeder
  Grenze.
* **Karstos selbst:** capability-basiert. **Genau hier liegt Firns Chance.**

### Firns Ansatz

**Ein Modell für Sprache und Betriebssystem.** Eine Capability in Firn ist
**dasselbe** wie eine Capability in Karstos: ein nicht fälschbares Handle, das
man besitzen oder weitergeben, aber nicht erfinden kann.

Der Trick: **Firn braucht dafür fast keinen neuen Mechanismus** — der `Io`-Wert
aus §1 und der `Allocator` aus §2 *sind* bereits Capabilities. Wer kein `Io`
bekommt, kann keine E/A machen. Das ist keine Prüfung, sondern eine Folge des
Typsystems.

Dazu kommen zwei Ergänzungen:

**(a) Modul-Deklaration.** Jedes Paket erklärt in seiner Beschreibung, welche
Fähigkeiten es überhaupt anfordern darf:

```toml
# firn.toml eines Bilddecoders
[paket]
name = "png"

[faehigkeiten]
# leer = dieses Paket kann NICHTS tun, außer rechnen.
# Kein Io, kein Dateisystem, kein Netz, keine Zeit, kein Zufall.

[bauzeit]
skript = false        # dieses Paket führt beim Bauen keinen Code aus
```

Der Compiler prüft das **statisch**: Ein Modul ohne `netz`-Fähigkeit darf den
Typ `NetIo` nicht einmal nennen. Verstoß = Übersetzungsfehler, nicht
Laufzeitfehler.

**(b) Eingeschränkte Handles beim Weitergeben.** Fähigkeiten werden beim
Durchreichen **verengt**, nie erweitert:

```firn
// Der Aufrufer entscheidet, was der Decoder sehen darf:
let bild = try png.decode(daten, inout arena)   // kein io, kein alloc-global
```

**(c) Bauzeit ist der gefährlichste Moment.** `cargo`s `build.rs` ist der
bequemste Angriffsweg überhaupt. In Firn gilt: Bauskripte (`SPEC.md` §6.4) laufen
**in einer Sandbox ohne Netz und ohne Schreibrechte außerhalb ihres
Ausgabeverzeichnisses**, und ein Paket, das ein Bauskript hat, muss das
deklarieren (`skript = true`). Die Paketverwaltung zeigt das beim Hinzufügen an.

**(d) Durchsetzung zur Laufzeit.** Statische Prüfung deckt alles ab, was der
Compiler sieht. Für dynamisch geladene Komponenten (Karstos-Prozesse,
Browser-Tabs) setzt **Karstos** dieselben Capabilities durch — das
Sprachmodell und das Kernmodell sind deckungsgleich, es gibt keine
Übersetzungsschicht.

### Zielkonflikt

* **`unsafe` unterläuft alles.** Ein Modul mit `unsafe` und Inline-Assembler kann
  einen Syscall direkt absetzen. Ehrliche Antwort: **Die Capability-Prüfung der
  Sprache gilt nur für sicheren Code.** Für unsicheren Code muss Karstos
  durchsetzen. Die Paketverwaltung muss deshalb `unsafe`-Blöcke pro Paket zählen
  und anzeigen — das ist die einzige belastbare Warnung.
* **Ergonomie.** Fähigkeiten durchreichen ist derselbe Aufwand wie `Io`
  durchreichen (§1) — es addiert sich nicht, weil es *dasselbe* ist.
* **Kollision mit Punkt 4 (stabiles ABI):** Ein Handle, das über eine
  ABI-Grenze geht, braucht eine stabile Darstellung.

### Konsequenz für den Compiler HEUTE

* **Das Modulsystem muss von Anfang an eine Paketgrenze kennen**, nicht nur
  Dateien. Firn hat heute `import pfad.modul` innerhalb eines Projekts — die
  Erweiterung um „welches *Paket* ist das, und was darf es" muss beim Entwurf
  der Paketverwaltung (Phase 3) mitgedacht werden, sonst ist es später ein
  Bruch.
* **Keine Ambient-Fähigkeiten in der Standardbibliothek.** Es darf **nie** ein
  `std.fs.open(pfad)` ohne `Io`-Parameter geben. Das ist dieselbe Disziplinregel
  wie bei §2 — und genauso unumkehrbar.
* Heute nichts zu bauen; nur nichts falsch zu machen.

### Priorität und Phase

**FUNDAMENT (als Regel, nicht als Code).** Die Regel „keine Ambient-Autorität"
muss ab Phase 3 gelten. Deklaration und Prüfung in der Paketverwaltung:
**Phase 3**. Durchsetzung gegen `unsafe` über Karstos: **Phase 6**.

---

## 4. Stabiles ABI

### Problem

Rust hat **kein** stabiles ABI. `#[repr(Rust)]` darf Felder umsortieren, und
zwischen zwei Compilerversionen darf sich alles ändern. Folgen:

* Keine echten Plugins. Wer Erweiterungen will, muss `extern "C"` benutzen und
  jede Struktur von Hand als C-Datentyp modellieren — man verliert Generics,
  Traits, `Option`, Ergebnistypen.
* Keine austauschbaren Systembibliotheken. Ein Rust-`.so` gegen ein anderes
  auszutauschen, ist nicht definiert.
* `abi_stable`-artige Krücken existieren, sind aber Bibliotheken, die das
  Problem umgehen, nicht lösen.

### Stand der Technik

**Swift ist der einzige ernsthafte Beleg, dass es geht.** Seit Swift 5 (2019)
ist das ABI auf Apple-Plattformen stabil — deshalb liegt die Swift-Laufzeit im
Betriebssystem statt in jeder App. Der Mechanismus heißt *Library Evolution*
(intern „Resilience") und ist **opt-in pro Bibliothek**.

Der Preis ist gut dokumentiert und deutlich: Ist Library-Evolution-Modus an,
müssen Aufrufer auf Felder und Aufzählungsfälle **indirekt** zugreifen — über
nicht einbettbare Funktionsaufrufe. Größe und Feldlayout eines Typs sind dann
erst zur **Laufzeit** bekannt. Es gibt also keinen direkten Feldzugriff mehr,
kein Inlining über die Bibliotheksgrenze, keine Layout-Annahmen. Swift hat
deshalb `@inlinable` und `@frozen` eingeführt, mit denen eine Bibliothek Teile
ihres Layouts *einfriert* und damit wieder Leistung gewinnt — auf Kosten der
Änderbarkeit.

### Firns Ansatz

**Firn bekommt ein stabiles ABI — aber `opt-in` pro Komponentengrenze, nicht
als Voreinstellung.** Das ist Swifts Aufteilung, nur mit anderem Standard:
Swift ist innerhalb eines Moduls schnell und an Bibliotheksgrenzen resilient;
Firn ist überall schnell, **außer** wo `#[abi_stable]` steht.

```firn
// Eine austauschbare Karstos-Komponente:
#[abi_stable(version = 1)]
interface FensterManager {
    fn erzeuge(io: Io, breite: u32, hoehe: u32) -> !FensterId
    fn zerstoere(io: Io, id: FensterId)
}
```

Regeln:

1. **Voreinstellung ist instabil** (`repr(firn)`): Der Compiler darf umsortieren,
   einbetten, Layout annehmen — volle Leistung. Das gilt für den gesamten
   Browser-Code, den Kernel und alles statisch Gelinkte.
2. **`#[abi_stable]`** schaltet für genau diese Schnittstelle Swifts Modell ein:
   feste Aufrufkonvention, indirekter Feldzugriff, Größe zur Laufzeit,
   Versionsnummer im Symbol.
3. **`#[frozen]`** friert ein Layout ein und gibt die Leistung zurück — mit dem
   Versprechen, es nie wieder zu ändern. Wie Swifts `@frozen`.
4. **`extern "C"`** bleibt zusätzlich verfügbar (`L14`), für Werkzeuge und
   Testorakel.
5. **Firn↔Firn-ABI über Komponentengrenzen** ist in `SPEC.md` §13 bereits als
   `L13`-MUSS geführt — dieser Punkt ist also keine Neuerung, sondern die
   Ausarbeitung.

**Braucht Firn das wirklich?** Ja, aber später als man denkt:

* **Der Browser braucht es nicht.** Er wird statisch gelinkt (`R5`), Blöcke
  reden über IPC mit serialisierten Nachrichten — nicht über ein ABI.
* **Karstos braucht es**, sobald Systemkomponenten austauschbar sein sollen
  (Treiber, Fenstermanager, Dienste) oder ein App-Modell mit nachgeladenen
  Erweiterungen entsteht.
* **IPC schlägt ABI, wo es geht.** Ein serialisierter Nachrichtenkanal ist
  robuster, versionierbar und passt zum Capability-Modell (§3). Firn sollte das
  bevorzugen und `#[abi_stable]` nur dort einsetzen, wo die Kosten eines
  Kanals zu hoch sind.

### Zielkonflikt

* **Direkt gegen Leistung.** Resiliente Typen kosten Indirektion an jedem
  Feldzugriff und verhindern Inlining über die Grenze — Swift belegt das. Deshalb
  opt-in.
* **Gegen Punkt 5 und 8:** Layout-Kontrolle (§8) und stabiles ABI sind
  gegensätzlich — wer SoA-Layout wählt, kann es nicht einfrieren.
* **Gegen Hot Reload (§9):** dort ist stabiles ABI die *Voraussetzung*, nicht der
  Gegner. Siehe §9.

### Konsequenz für den Compiler HEUTE

* **Das Symbolschema muss von Anfang an Platz für Versionen haben.** Wenn Firn
  heute `main` und `add` als nackte Symbole ausgibt und später `_F1_add@v2`
  braucht, ist das ein Bruch. Ein einfaches, dokumentiertes Namensschema
  (Modulpfad + Name + Signaturkürzel) sollte in **Phase 3** mit dem Modulsystem
  kommen — das ist billig und macht später alles möglich.
* **`SPEC.md` §13 muss festhalten, dass das Standardlayout ausdrücklich
  *instabil* ist.** Sonst verlässt sich Code darauf und man kann es nie ändern.
* Sonst: nichts. Das ist ein Punkt, den man wirklich später bauen kann.

### Priorität und Phase

**NACHRÜSTBAR** — mit einer billigen Vorleistung (Symbolschema).
Vorleistung **Phase 3**, Umsetzung **Phase 7/8**, wenn Karstos austauschbare
Komponenten braucht. Vorher nicht.

---

## 5. Debug-Build-Geschwindigkeit

### Problem

Rust-Debug-Builds sind typisch **10–50× langsamer** als Release-Builds. Folgen
aus der Praxis:

* Spieleentwicklung in Rust ist deshalb ein Dauerthema: `bevy` empfiehlt,
  Abhängigkeiten mit `opt-level = 3` und den eigenen Code mit `opt-level = 1`
  zu bauen — eine Krücke, die jedes Projekt selbst konfigurieren muss.
* Ein Browser-Debug-Build, der Seiten 30× langsamer rendert, ist zum Debuggen
  von Layout- oder Rasterisierungsfehlern **unbrauchbar**, weil das Verhalten
  (Zeitverhalten, Animationen, Zeitüberschreitungen) ein anderes ist.
* Chromium löst das mit `is_component_build` + selektiver Optimierung; Firefox
  mit `--enable-optimize --enable-debug`. **Alle großen Projekte haben denselben
  Notbehelf gebaut** — das ist ein Sprachdesign-Signal.

Die Ursache ist ein **Alles-oder-Nichts-Schalter**: `-O0` erzeugt Code, in dem
jede Variable im Speicher liegt und jeder kleine Aufruf ein echter Call ist.

### Stand der Technik

* **Rust/C++:** `-O0`/`-O2`/`-O3` als grobe Stufen; `-Og` in GCC/Clang ist der
  richtige Gedanke, aber schwach ausgeprägt und wenig benutzt.
* **Zig:** vier Modi (`Debug`, `ReleaseSafe`, `ReleaseFast`, `ReleaseSmall`) —
  besser, weil `ReleaseSafe` optimiert *und* Prüfungen behält. Der Debug-Modus
  ist trotzdem langsam.
* **Go:** kaum Unterschied, weil der Compiler generell wenig optimiert. Löst das
  Problem, indem es das andere Problem hat.

### Firns Ansatz

**Vier Stufen, und die wichtigste ist die Voreinstellung.**

| Stufe | Optimierung | Prüfungen | Debuggen | Zweck |
|---|---|---|---|---|
| `--dev` | **keine** | alle | perfekt | nur für Compilerfehlersuche |
| **`--dev-fast`** *(Voreinstellung)* | **debugerhaltende Grundoptimierung** | alle | **sehr gut** | **der Alltagsmodus** |
| `--release-safe` | voll | alle | mäßig | Auslieferung mit Netz |
| `--release-fast` | voll | keine | schlecht | Messungen, Endprodukt |

**Was „debugerhaltende Grundoptimierung" konkret heißt** — die Auswahl ist der
eigentliche Entwurf:

**Erlaubt** (billig, große Wirkung, zerstört das Debugbild nicht):
* `mem2reg` — Variablen in Register statt in Stapelfächer. **Das ist der mit
  Abstand größte Einzelgewinn** und der Hauptgrund, warum `-O0` so langsam ist.
  Firn hat es bereits (`compiler/src/mem2reg.rs`) und misst dafür im Median
  **~10× gegenüber `--no-opt`** (`bench/RESULTS.md`).
* Registerzuteilung (linear scan) — vorhanden.
* Konstantenfaltung, Entfernen offensichtlich toten Codes, Blockverschmelzung.
* **Einbetten nur von trivialen Funktionen** (Zugriffsmethoden, Ein-Ausdruck-
  Funktionen, Umhüllungen). Das ist die Grenze: Diese Funktionen im Rückverfolg
  zu verlieren, stört niemanden — sie hatten ohnehin keine eigene Logik.

**Verboten in `--dev-fast`** (weil es das Debuggen zerstört):
* Aggressives Einbetten größerer Funktionen — der Aufrufstapel wird unlesbar.
* Schleifen abrollen, vertauschen, vektorisieren — Zeilennummern werden sinnlos.
* Variablen zusammenlegen, deren Lebensdauern sich nicht überschneiden — `gdb`
  zeigt dann falsche Werte, was schlimmer ist als gar keine.
* Umsortieren von Anweisungen über Zeilengrenzen hinweg.

**Die Regel, die alles zusammenhält:** In `--dev-fast` muss **jede benannte
Variable an jedem Haltepunkt ihren korrekten Wert zeigen**. Eine Optimierung,
die das bricht, gehört nicht in diese Stufe. Das ist ein prüfbares Kriterium,
kein Gefühl — und es wird als Test gefahren (`gdb`-Sitzung, Werte vergleichen).

### Zielkonflikt

* **Zwei Optimierungspfade zu pflegen** kostet Compilerarbeit. Gegenmittel:
  `--dev-fast` ist eine **Teilmenge** der Release-Durchgänge, keine eigene
  Kette — nur eine Auswahl.
* **Prüfungen bleiben an.** `--dev-fast` bleibt dadurch langsamer als
  `--release-fast`; das ist gewollt.
* **Erwartungsdämpfer:** `--dev-fast` wird nicht Release-Geschwindigkeit
  erreichen. Ziel ist Faktor **2–3× langsamer als Release**, nicht 30×. Das
  reicht, um einen Browser oder ein Spiel bedienbar zu debuggen.

### Konsequenz für den Compiler HEUTE

* **Jeder Optimierungsdurchgang muss einzeln an- und abschaltbar sein** und ein
  Etikett tragen: *debugerhaltend* oder nicht. Wenn die Durchgänge zu einer
  festen Kette verdrahtet werden, ist diese Stufe später nicht mehr baubar.
  **Das ist die einzige echte Fundamentanforderung dieses Punktes — und sie
  kostet heute fast nichts.**
* **Zeileninformation muss jeden Durchgang überleben.** Firn hat bereits
  `.debug_line` (`docs/DEBUGGER.md`); die Durchgänge müssen die Zuordnung
  mitführen, statt sie zu verlieren. Nachrüsten heißt jeden Durchgang anfassen.
* Firn hat heute nur `--no-opt` und „voll". Die Vierteilung kommt in Phase 3.

### Priorität und Phase

**Fundament: die Durchgangsarchitektur** (einzeln schaltbar, mit Etikett,
Zeileninfo erhaltend) — **Phase 2/3**, jetzt billig.
Umsetzung der vier Stufen: **Phase 3**.

---

## 6. In-Place-Initialisierung

### Problem

Rust baut einen Wert erst auf dem Stapel und kopiert ihn dann an sein Ziel. Der
Optimierer entfernt die Kopie meistens — aber **meistens** ist keine Garantie.
Konkret:

```rust
// Rust: baut 8 MB auf dem Stack, DANN in die Box. Stack-Overflow.
let b = Box::new([0u8; 8 * 1024 * 1024]);
```

`Box::new_uninit` existiert, ist aber unbequem und `unsafe`. Für Rust-for-Linux
war das ein echter Blocker: Kernel-Strukturen enthalten selbstbezügliche Teile
(verkettete Listen, Sperren mit Adressabhängigkeit) und sind zu groß für den
Kernelstapel (typisch 8–16 KB). Die Antwort war die Bibliothek **`pin-init`**:
`#[pin_data]`, `pin_init!`, `try_pin_init!` — Makros, die einen *In-Place-
Konstruktor* als Wert erzeugen. Aus der Dokumentation: *„It also allows in-place
initialization of big structs that would otherwise produce a stack overflow."*

Das ist eine **Krücke** — brillant gemacht, aber eine Bibliothek, die eine
fehlende Sprachfähigkeit nachbaut, mit Makros, eigenem Trait-Zoo und einer
Lernkurve.

### Stand der Technik

| Sprache | Verhalten |
|---|---|
| Rust | Kopie mit Optimiererhoffnung; `pin-init` als Bibliothek |
| C++ | Platzierungs-`new` und garantierte Kopieelision seit C++17 — löst es, aber nur für Rückgabewerte |
| Zig | **Ergebnisort-Semantik**: `var x: T = f();` gibt `f` die Adresse von `x`; es wird direkt dort gebaut. Sprachgarantie, kein Makro |
| C | von Hand mit Zeigern; funktioniert, ist unsicher |

Zigs Lösung ist die richtige: **der Zielort ist Teil der Aufrufsemantik**, nicht
eine Optimierung.

### Firns Ansatz

**Garantierte Konstruktion am Zielort als Sprachregel** — Zigs Ergebnisort-
Semantik, verbunden mit Firns fehlbarer Allokation (§2).

1. **Ergebnisort-Regel:** Bei `let x: T = ausdruck` und `return ausdruck` kennt
   der erzeugende Ausdruck die Zieladresse und schreibt direkt dorthin. Für
   Structliterale, Arrayliterale und Funktionsrückgaben ist das eine
   **Garantie**, keine Optimierung — sie gilt auch in `--dev` (§5).
2. **`init`-Ausdruck für fehlbaren Aufbau am Zielort:**

```firn
fn neuer_puffer(alloc: inout Allocator, n: usize) -> AllocError!Gross {
    // 'init at' baut direkt im frisch belegten Speicher, nie auf dem Stapel:
    return try alloc.new_init(Gross, init {
        kopf:  try Kopf.neu(inout alloc),   // fehlbar mitten im Aufbau
        daten: [0; 8 * 1024 * 1024],        // 8 MB, direkt am Ziel
        ende:  0xDEAD,
    })
}
```

3. **Aufräumen bei Teilfehlschlag ist Compilerarbeit, nicht Handarbeit.**
   Schlägt `Kopf.neu` fehl, gibt der Compiler die bereits fertiggestellten Felder
   in umgekehrter Reihenfolge frei und meldet den Fehler weiter. Genau das ist
   der schwierige Teil, den `try_pin_init!` in Linux von Hand modelliert.
4. **Kein `Pin`.** Firn braucht Rusts `Pin` nicht, weil Werte nicht hinter dem
   Rücken verschoben werden können: Moves sind statisch sichtbar (`SPEC.md` §3.3)
   und `Gc[T]` wird wegen des konservativen Stapelscans **nie** verschoben
   (§3.5.3). Selbstbezügliche Strukturen bleiben trotzdem `unsafe` — aber sie
   brauchen keinen eigenen Typzoo.
5. **`#[no_move]`** markiert Typen, die nach dem Aufbau nicht mehr verschoben
   werden dürfen (Sperren, Listenknoten, DMA-Puffer). Ein Move ist dann ein
   Übersetzungsfehler statt eines Laufzeitfehlers.

### Zielkonflikt

* **Der Codegenerator wird komplizierter.** Ergebnisort-Semantik heißt, dass
  jeder Ausdruck einen optionalen Zielzeiger mitbekommt und die Aufrufkonvention
  für aggregierte Rückgaben (System-V `MEMORY`-Klasse, versteckter Zeiger in
  `rdi`) korrekt bedient wird. Das ist echte Arbeit im Lowering.
* **Kollision mit §2:** Fehlbarer Aufbau am Zielort braucht die Aufräumlogik aus
  Punkt 3. Ohne die ist es unsicher; mit ihr ist es Compilerarbeit.
* **Kollision mit §8 (SoA):** Ein Wert, dessen Felder in getrennten Arrays
  liegen, hat keinen einzelnen „Zielort". Für SoA-Sammlungen gilt die Garantie
  feldweise, nicht als Ganzes.

### Konsequenz für den Compiler HEUTE

* **Aggregat-Rückgabe muss von Anfang an über den versteckten Zeiger laufen**,
  nicht über eine Kopie. Firn hat Aggregate an Funktionsgrenzen seit der letzten
  Runde (`compiler/src/abi.rs`, SPEC §14.1 Punkt 1 gestrichen). **Jetzt ist der
  richtige Moment zu prüfen, ob dort schon direkt am Ziel geschrieben wird** —
  wenn ja, ist der Grundstein gelegt; wenn nein, ist es später ein Umbau des
  gesamten Lowerings.
* **Die IR muss einen Zielort-Operanden kennen können.** In FIR ist das ein
  zusätzlicher Operand an `call` und an Aggregatkonstruktion. Später
  nachzurüsten heißt, jeden Lowering-Pfad anzufassen.
* Das ist die **teuerste Fundamentanforderung** dieses Dokuments nach §1.

### Priorität und Phase

**FUNDAMENT.** Ergebnisort im Lowering und in der IR: **Phase 2**
(jetzt, solange das Lowering klein ist). `init`-Ausdruck mit Teilaufräumung und
`#[no_move]`: **Phase 3**, zusammen mit `drop` und dem Move-Prüfer.

---

## 7. Metaprogrammierung zur Übersetzungszeit

### Problem

* **Rusts `proc-macro`s sind eigene Programme.** Sie werden als separate Crate
  kompiliert, laufen als Prozess und arbeiten auf einem Tokenstrom — **ohne
  Typinformation**. Deshalb braucht `serde` einen kompletten eigenen
  Ableitungsapparat und `syn`/`quote` als Parser. Die Bauzeit leidet massiv:
  `syn` + `serde_derive` sind in vielen Projekten die größten Einzelposten.
  Und weil Makros beliebigen Code beim Bauen ausführen, sind sie zugleich das
  Loch aus §3.
* **C++ bekommt Reflexion erst mit C++26** (P2996, im Juni 2025 abgestimmt) —
  mit `^^` als Reflexionsoperator und `[: … :]` zum Wiedereinsetzen. Bis dahin
  gab es 30 Jahre lang nur Template-Metaprogrammierung und Präprozessor-Tricks.
  Vollständige Codeerzeugung (Token-Injektion, P3294) kommt erst danach.
* **Für den Browser ist das kein Komfort, sondern Pflicht:** Ladybird hat
  **697 `.idl`-Dateien**, Blink 2.235. Dazu 2.231 HTML-Entities,
  CSS-Eigenschaftstabellen, Unicode-Tabellen aus der UCD, CLDR-Daten. Das sind
  hunderttausende Zeilen **generierter** Code. Ohne einen guten Mechanismus baut
  man eine externe Werkzeugkette in Python daneben — und pflegt sie ewig.

### Stand der Technik

| Ansatz | Typinformation? | Bauzeit | Sicherheit |
|---|---|---|---|
| C-Präprozessor | nein | schnell | keine |
| C++-Templates | teilweise | sehr langsam | — |
| C++26 `^^`/`[::]` | **ja** | gut | gut |
| Rust `proc-macro` | **nein** (nur Tokens) | **schlecht** | beliebiger Code |
| Rust `build.rs` | nein | mittel | beliebiger Code |
| **Zig `comptime`** | **ja** | gut | eingeschränkt |

Zigs Modell ist das beste verfügbare: **dieselbe Sprache, nur früher
ausgeführt**, mit `type` als gewöhnlichem Wert.

### Firns Ansatz

**Zwei Ebenen, klar getrennt** — `comptime` für alles im Programm,
Bauskripte nur für externe Daten.

**(a) `comptime` — Ausführung im Compiler.**
Bereits in `SPEC.md` §6.1 festgelegt: `comptime`-Parameter, `type` als Wert,
Monomorphisierung, `comptime if` statt `#ifdef`. Umgesetzt wird das als
**Interpreter über FIR** — nicht über dem AST. Grund: FIR ist bereits typisiert
und entzuckert, der Interpreter bleibt klein, und `comptime`-Code läuft durch
dieselbe Typprüfung wie Laufzeitcode.

**(b) Reflexion über Typen** — das, was Rust fehlt und C++ 30 Jahre gefehlt hat:

```firn
// Web-IDL-Bindung ohne externes Werkzeug:
fn erzeuge_getter[comptime T: type]() {
    comptime for feld in reflect.fields(T) {
        comptime if feld.hat_attribut("idl") {
            emit fn @[feld.name]() -> feld.typ { return self.@[feld.name] }
        }
    }
}
```

* `reflect.fields(T)`, `reflect.variants(T)`, `reflect.methods(T)`,
  `reflect.attributes(T)` liefern zur Übersetzungszeit **typisierte** Daten —
  nicht Tokens. Das ist der entscheidende Unterschied zu `proc-macro`.
* `emit` fügt erzeugte Elemente ein; `@[ausdruck]` bildet einen Namen aus einer
  Übersetzungszeit-Zeichenkette. Bewusst eng gehalten: **kein beliebiger
  Tokenstrom**, nur wohlgeformte Elemente. Damit bleibt der Parser einfach und
  die Fehlermeldungen brauchbar.
* Erzeugter Code ist **lesbar und ablegbar** (`--emit=generated`) und behält per
  `#line` den Bezug zur Quelle (`G2`), damit der Debugger etwas Sinnvolles zeigt.

**(c) Bauskripte** (`SPEC.md` §6.4) bleiben für das, was `comptime` nicht kann:
externe Dateien lesen (UCD, CLDR, `.idl`, HTML-Entities). Sie laufen in der
Sandbox aus §3 — **kein Netz, Schreibrechte nur im Ausgabeverzeichnis**.
F�r große Tabellen liefert die Bibliothek perfektes Hashing und komprimierte
Tries (`G4`).

**Grenze, bewusst gezogen:** `comptime` darf **keine** E/A. Kein Dateizugriff,
kein Netz, kein Prozessstart. Sonst ist es dasselbe Sicherheitsloch wie
`proc-macro` und `build.rs`. Wer Dateien braucht, nimmt ein Bauskript — und das
steht in `firn.toml` und ist sichtbar.

### Zielkonflikt

* **Übersetzungszeit.** Ein `comptime`-Interpreter, der ganze Tabellen
  ausrechnet, kostet Bauzeit. Gegenmittel: Zwischenspeichern der Ergebnisse
  (`W10`), Auswertungsschranke (`--comptime-budget`) mit klarer Fehlermeldung
  statt Aufhängen.
* **Fehlermeldungen in generischem Code** bleiben schlechter als bei echten
  Traits — steht schon in `SPEC.md` §6.1 und wird durch Reflexion nicht besser.
* **`emit` erhöht die Compilerkomplexität deutlich.** Elemente zur
  Übersetzungszeit einzufügen bedeutet, dass Namensauflösung und Typprüfung
  mehrfach laufen müssen. Das ist der Grund, warum es Phase 3 ist und nicht
  Phase 2.

### Konsequenz für den Compiler HEUTE

* **Die Übersetzungsphasen müssen wiedereintrittsfähig sein.** Wenn
  Namensauflösung und Typprüfung als einmaliger Durchlauf über einen festen
  AST gebaut werden, ist `emit` später nicht nachrüstbar. Der Compiler muss
  „prüfe diese neu entstandene Funktion" können.
* **FIR muss interpretierbar bleiben** — also keine Instruktion, die nur im
  Codegenerator Sinn ergibt. Das ist heute erfüllt und muss so bleiben.
* Firn hat heute Monomorphisierung (`mono.rs`, `sema_generic.rs`) — die halbe
  Miete. `comptime` selbst gibt es nicht.

### Priorität und Phase

**Fundament: Wiedereintrittsfähigkeit der Prüfphasen** (Phase 2, billig, wenn
man es weiß). `comptime`-Interpreter und Reflexion: **Phase 3** — sie sind
Voraussetzung für Abnahmepunkt 6 (UCD-Tabelle) und für jede Web-IDL-Bindung.

---

## 8. Datenlayout-Kontrolle — AoS gegen SoA

### Problem

Sprachen erzwingen praktisch immer **Array von Strukturen** (AoS):

```
[{x,y,z,farbe}, {x,y,z,farbe}, {x,y,z,farbe}, …]
```

Der Cache will oft **Struktur von Arrays** (SoA):

```
xs: [x,x,x,…]  ys: [y,y,y,…]  zs: […]  farben: […]
```

Wer nur `x` liest, lädt bei AoS die ganze Struktur in den Cache — bei einer
64-Byte-Cachezeile und einer 32-Byte-Struktur werden 75 % der Bandbreite
verschwendet. Bei SoA ist die Zeile voll mit Nutzdaten. Faktoren von 2–5× auf
speichergebundenen Schleifen sind normal.

**Direkt relevant für dieses Projekt:**
* **Rasterizer:** Kantenlisten, Scanline-Aktivlisten — reine Zahlenschleifen.
* **DOM-Knoten:** werden millionenfach angelegt. Layout-Durchläufe lesen oft nur
  ein einzelnes Feld über alle Knoten (z. B. „alle mit `display: none`").
  `P7` aus den Browser-Anforderungen sagt ausdrücklich: *„Ein DOM-Knoten wird
  millionenfach angelegt. Jedes überflüssige Byte kostet Cache."*
* **Layout-Baum, Stilwerte, Glyphenlisten** — dasselbe Muster.

Die übliche Antwort ist, SoA von Hand zu schreiben: sechs parallele Arrays und
`u32`-Indizes statt Zeigern. Das funktioniert, aber der Code wird unlesbar und
jede Änderung an der Struktur muss an sechs Stellen nachgezogen werden.

### Stand der Technik

* **Zig `std.MultiArrayList(T)`:** erzeugt aus einer gewöhnlichen Structdefinition
  automatisch SoA — eine einzige Allokation, in Feld-Arrays aufgeteilt, Felder
  nach absteigender Ausrichtung sortiert, dadurch **kein Füllbyte zwischen den
  Feldern**. Zugriff über `items(.feld)` oder `get(i)`. Das ist der bisher beste
  Ansatz einer Systemsprache — aber es ist ein **Bibliothekstyp**, kein
  Sprachmittel, und der Zugriff sieht anders aus als bei `ArrayList`.
* **Jai (Jonathan Blow):** `using` plus SoA-Umschalter am Typ — die
  radikalste Fassung, aber die Sprache ist nicht öffentlich stabil.
* **Rust:** nur von Hand oder über Crates (`soa_derive`, `soa-rs`) mit Makros.
* **C++:** von Hand; `std::experimental::simd` hilft nur bei Vektoren.

### Firns Ansatz

**Layout ist eine Eigenschaft der *Sammlung*, nicht des Typs — und der Zugriff
sieht identisch aus.**

```firn
struct Knoten {
    eltern:  u32,
    erstes:  u32,
    naechst: u32,
    flags:   u16,
    tag:     Atom,
}

// Dieselbe Struktur, zwei Layouts. Der Code darüber ändert sich NICHT:
var baum:  Vec[Knoten]      = …   // AoS  — Standard
var baum2: SoaVec[Knoten]   = …   // SoA  — pro Feld ein Array

// Zugriff ist in beiden Fällen gleich:
baum2[i].flags = baum2[i].flags | SICHTBAR

// Feldweiser Durchlauf nur bei SoA — und dann bandbreitenoptimal:
for f in baum2.spalte(.flags) { … }
```

1. **`SoaVec[T]` wird vom Compiler erzeugt**, nicht von einer Makrobibliothek:
   Er kennt das Feldlayout von `T` ohnehin und generiert die Spalten-Arrays,
   die Zugriffsmethoden und die Ausrichtungssortierung. Das ist genau
   `MultiArrayList`, nur ohne den Bruch in der Benutzung.
2. **`baum2[i]` liefert keinen Zeiger auf eine `Knoten`-Struktur** (die es
   physisch nicht gibt), sondern einen **Sichtwert** — ein compilergeneriertes
   Bündel von Feldverweisen. Weil Firns Referenzen zweitklassig sind
   (`SPEC.md` §3.3), ist das unproblematisch: die Sicht kann ihren Aufrufrahmen
   ohnehin nicht verlassen. **Ein Glücksfall des Speichermodells** — in Rust
   bräuchte man dafür Lebensdauern und einen eigenen Typ.
3. **`#[layout(soa)]`** kann alternativ am Typ stehen, wenn *jede* Sammlung
   dieses Typs SoA sein soll.
4. **Ergänzende Layout-Mittel** (teils schon in `SPEC.md` §13):
   `#[packed]`, `#[align(n)]`, feste Feldreihenfolge, `#[bitfeld]` für
   Flaggenwörter, und — wichtig für DOM-Knoten — `#[klein(N)]` für Sammlungen
   mit Inlinespeicher (`B1`: „kleine Vektoren mit Inline-Speicher").
5. **Umschaltbar zum Messen:** `SoaVec` und `Vec` sind austauschbar, also lässt
   sich die Frage „bringt SoA hier etwas?" durch Ändern **eines Worts** messen
   statt durch Umschreiben eines Moduls. Das ist der praktische Hauptgewinn.

### Zielkonflikt

* **Zeiger auf Elemente gibt es nicht.** Bei SoA existiert kein
  zusammenhängendes Element, auf das man zeigen könnte. Wer `*Knoten` braucht,
  kann `SoaVec` nicht nehmen. Das schließt SoA für `Gc[T]`-verwaltete DOM-Knoten
  faktisch aus — **relevante Einschränkung**, ehrlich benannt: SoA ist für den
  Rasterizer, den Layout-Baum und Kantenlisten, nicht für den GC-Heap.
* **Gegen §4 (stabiles ABI):** SoA-Layout ist per Definition nicht einfrierbar.
* **Compileraufwand:** Sichtwerte und generierte Spaltentypen sind echte Arbeit
  im Typprüfer.
* **Fehlmessung droht:** SoA ist nicht immer besser. Wer alle Felder eines
  Elements liest, ist mit AoS schneller. Deshalb ist AoS die Voreinstellung.

### Konsequenz für den Compiler HEUTE

* **Der Typprüfer muss Feldzugriff von Speicherort trennen können.** Solange
  `a.b` fest „Basisadresse plus Versatz" bedeutet, ist SoA nicht nachrüstbar.
  Nötig ist eine Zwischenschicht: „Feldzugriff auf einen Wert der Art X" mit
  einer Berechnungsvorschrift, die für AoS ein Versatz und für SoA eine
  Spaltenadresse ist. **Das ist eine Änderung im Lowering, die man heute für
  einen Zehntel der späteren Kosten haben kann.**
* **Layoutberechnung muss zentral sein**, nicht über Codegenerator und Sema
  verteilt. Heute liegt sie in `types.rs`/`abi.rs` — das ist die richtige Stelle
  und muss so bleiben.
* Sonst nichts. Es ist ein Bibliotheks-plus-Lowering-Thema, kein Syntaxthema.

### Priorität und Phase

**Fundament: die Trennung Feldzugriff ↔ Speicherort im Lowering** (Phase 2/3).
Umsetzung `SoaVec[T]` und `#[layout(soa)]`: **Phase 3/4**, wenn der Rasterizer
und der Layout-Baum entstehen und man messen kann.

---

## 9. Hot Reload — Code austauschen, ohne neu zu starten

*(Frage von Justin)*

### Problem und Reiz

Beim Entwickeln von Spielen, Oberflächen und Browsern ist der Zyklus
„ändern → bauen → starten → zum Fehler zurücknavigieren" der größte
Zeitfresser. Bei einem Browser bedeutet „zum Fehler zurück": Seite neu laden,
einloggen, zum richtigen Element scrollen, den Zustand wiederherstellen. Wer
eine Layoutregel um zwei Pixel korrigieren will, zahlt jedes Mal den vollen
Preis.

Wo es funktioniert, ist die Wirkung groß: Erlang/Elixir tauschen Module im
laufenden System (dafür gebaut, mit Prozessisolation und unveränderlichen
Daten). Flutters „Hot Reload" ist ein Verkaufsargument der Plattform.
Spieleengines (Unreal Live Coding, Unity Domain Reload, Jais `#run`-Umgebung)
haben es alle nachgerüstet, weil der Bedarf real ist.

### Was dafür technisch nötig ist — vollständig

Das wird oft unterschätzt. Es sind **vier** Anforderungen, und jede einzelne
kollidiert mit einem anderen Ziel dieses Dokuments:

1. **Trennung von Code und Zustand.** Der ausgetauschte Code darf keine Daten
   besitzen. Alles, was überleben soll, muss in einem Zustandsblock liegen, den
   die neue Fassung wiederfindet. In der Praxis heißt das: globale Variablen und
   Funktionszeiger in Datenstrukturen sind verboten oder müssen versioniert sein.
2. **Stabile Symbolauflösung.** Der Aufrufer muss nach dem Tausch die *neue*
   Funktion erreichen. Also entweder eine Indirektionstabelle bei jedem
   modulübergreifenden Aufruf (kostet Leistung, verhindert Inlining) oder
   Nachbinden aller Aufrufstellen zur Laufzeit (kompliziert und
   plattformabhängig).
3. **Zustandsmigration.** Ändert sich das Layout einer Struktur, sind die alten
   Daten falsch interpretiert. Entweder man verbietet Layoutänderungen beim
   Nachladen (dann ist es nur „halbes" Hot Reload) oder man braucht eine
   Migrationsfunktion pro Typ — die jemand schreiben muss.
4. **Dynamisches Nachladen.** Man braucht Ladeeinheiten (`.so`-artig), einen
   Lader, Relokationen — genau das, was Firn und Karstos ausdrücklich
   **abgeschafft** haben (`R5`: statisch linken, kein `dlopen`;
   `SPEC.md` §4.5: „keine dynamischen Bibliotheken").

**Der Kollisionsbefund ist eindeutig:** Punkt 2 verlangt **stabiles ABI** (§4)
oder Indirektion überall; Punkt 4 verlangt **dynamisches Laden**, das aus dem
Karstos-Entwurf bewusst entfernt wurde; und beide stehen gegen das
Leistungsziel ≤ 2× Rust (§10.3 der SPEC), weil Indirektion an Modulgrenzen
genau das Inlining verhindert, das `P1` fordert.

### Firns Ansatz — abgestuft, und der erste Schritt ist der wichtigste

**Stufe A (der eigentlich richtige Weg): schneller Compiler + schneller
Neustart.** Wenn ein Vollbau der Engine unter zehn Sekunden bleibt und der
Programmzustand ohnehin serialisierbar ist (was ein Browser braucht:
Sitzungswiederherstellung, Tab-Wiederherstellung), dann ist „neu starten und
Zustand laden" **fast so schnell wie Hot Reload — und immer korrekt.** Es gibt
keine halb migrierten Zustände, keine Geisterfehler durch alte Datenlayouts,
keine Zweifel, ob der Fehler echt ist oder vom Nachladen kommt. `W9` fordert
inkrementelle Übersetzung ohnehin.

**Stufe B (billig, hoher Nutzen): Daten neu laden statt Code.** Der weitaus
größte Teil des Iterationsbedarfs betrifft gar keinen Code: CSS-Regeln,
Layout-Parameter, Farben, Konstanten, Shader, Tabellen. Diese als Daten
auszulagern und beim Ändern neu einzulesen, kostet **keine** Sprachfähigkeit und
löst geschätzt 80 % des Problems. Für einen Browser ist das ohnehin der
Normalfall — er lädt Stylesheets schon zur Laufzeit.

**Stufe C (echtes Hot Reload, nur wenn Stufe A und B nicht reichen):**
eng begrenzt, nicht allgemein:
* nur für ausdrücklich markierte Module: `#[hot]`
* `#[hot]`-Module dürfen **keinen** eigenen Zustand halten — der Compiler prüft
  das (keine globalen Variablen, keine `static`-Daten)
* Aufrufe **in** ein `#[hot]`-Modul laufen über eine Indirektionstabelle; alle
  anderen Aufrufe bleiben direkt und einbettbar. Damit zahlt nur, wer bestellt
  (Leitsatz 4)
* Layoutänderungen an Typen, die Modulgrenzen überschreiten, werden beim
  Nachladen **abgelehnt** — mit klarer Meldung „Neustart nötig" statt stiller
  Datenverfälschung
* nur in `--dev`/`--dev-fast`, **nie** in Auslieferungsbauten

### Ehrliche Einschätzung — lohnt es sich?

**Nein, nicht als Sprachfähigkeit, und nicht in den nächsten Jahren.**

Begründung:

* Der **Nutzen-Kosten-Schnitt ist schlecht**: Stufe C verlangt dynamisches
  Laden, stabiles ABI und eine Zustandsdisziplin — drei große Baustellen — für
  einen Vorteil, den Stufe A und B zu ~80 % ohne jede Sprachänderung liefern.
* Es **kollidiert direkt** mit zwei Entscheidungen, die aus guten Gründen
  gefallen sind: statisches Linken (`R5`, spart Karstos den kompletten Lader)
  und Inlining über Modulgrenzen (`P1`, nötig für ≤ 2× Rust).
* Die Sprachen, bei denen Hot Reload wirklich gut funktioniert (Erlang, Elixir),
  haben es sich **teuer erkauft**: unveränderliche Daten, Prozessisolation,
  Nachrichtenaustausch statt geteiltem Speicher, eine schwergewichtige Laufzeit.
  Das ist das Gegenteil einer Systemsprache für einen Kernel.
* Wo es nachgerüstet wurde (Unreal, Unity), ist es berüchtigt für seltsame
  Fehler nach dem Nachladen — Entwickler starten im Zweifel doch neu.

**Was stattdessen getan wird:** Bauzeit als erstklassiges Ziel behandeln (`W9`,
`W10`) und Stufe B konsequent nutzen. **Was nicht getan wird:** irgendetwas im
Sprachdesign, das Hot Reload später *unmöglich* macht — die Tür bleibt über
`#[hot]` offen, weil das Konzept „Modul mit Indirektionstabelle" jederzeit
nachrüstbar ist, sobald es ein Modulsystem mit Symbolschema gibt (§4).

### Priorität und Phase

**NACHRÜSTBAR, niedrige Priorität.** Stufe B (Daten neu laden): Phase 4,
kostenlos. Stufe A (schneller Bau): Dauerthema ab Phase 3. Stufe C: **kein
Termin**, nur wenn A und B nachweislich nicht reichen.

---

## 10. Priorisierung — was MUSS jetzt ins Fundament

**Das ist der wichtigste Abschnitt dieses Dokuments.** Justin will nicht alles
sofort — er will nichts verbauen. Die Trennlinie verläuft zwischen Dingen, die
das *Fundament* betreffen (Typsystem, IR, Lowering, unumkehrbare Regeln), und
Dingen, die später obendrauf kommen.

### 10.1 Die Tabelle

| # | Thema | Was JETZT ins Fundament muss | Was später kommt | Stufe | Phase |
|---|---|---|---|---|---|
| 1 | **Funktionsfarben / `Io`** | **Kein `async`-Schlüsselwort einführen.** Codegen darf keine Stapelstetigkeit annehmen (Stapelwechsel muss möglich bleiben) | `Io`-Schnittstelle, `Future`, `Io.Threaded`, `Io.Evented` | **FUNDAMENT** (billig) | 3–4 |
| 2 | **Fehlbare Allokation** | `!T` + `#[must_consume]` im Sprachkern. **Regel: keine unfehlbare Allokationsfunktion, nie** | `Allocator`-Schnittstelle, Sammlungen, fehlbare GC-Allokation | **FUNDAMENT** (unumkehrbar) | 2–3 |
| 3 | **Capability-Module** | **Regel: keine Ambient-Autorität in der Bibliothek.** Modulsystem muss eine *Paket*grenze kennen | Deklaration in `firn.toml`, Prüfung, Bauskript-Sandbox | **FUNDAMENT** (als Regel) | 3 |
| 4 | **Stabiles ABI** | Nur ein **Symbol-Namensschema mit Versionsplatz**. `SPEC.md` muss sagen: Standardlayout ist *instabil* | `#[abi_stable]`, `#[frozen]`, resiliente Aufrufe | nachrüstbar | 3 → 7/8 |
| 5 | **Debug-Bau-Geschwindigkeit** | **Optimierungsdurchgänge einzeln schaltbar + Etikett „debugerhaltend"**. Zeileninfo überlebt jeden Durchgang | vier Baustufen `--dev` … `--release-fast` | **FUNDAMENT** (billig) | 2 → 3 |
| 6 | **In-Place-Initialisierung** | **Ergebnisort-Operand in FIR und im Lowering.** Aggregatrückgabe schreibt direkt ans Ziel | `init`-Ausdruck mit Teilaufräumung, `#[no_move]` | **FUNDAMENT** (teuer, aber jetzt am billigsten) | 2 → 3 |
| 7 | **Comptime + Reflexion** | **Prüfphasen wiedereintrittsfähig** (neu erzeugte Elemente nachträglich prüfbar). FIR bleibt interpretierbar | `comptime`-Interpreter, `reflect.*`, `emit`, Bauskripte | **FUNDAMENT** (Architektur) | 2 → 3 |
| 8 | **Datenlayout / SoA** | **Feldzugriff vom Speicherort trennen** im Lowering. Layoutberechnung an einer Stelle | `SoaVec[T]`, `#[layout(soa)]`, `#[bitfeld]`, `#[klein(N)]` | **FUNDAMENT** (Lowering) | 2/3 → 3/4 |
| 9 | **Hot Reload** | **nichts** — nur nicht ausschließen | Stufe B (Daten neu laden), evtl. `#[hot]` | nachrüstbar | 4 / kein Termin |
| — | *(bereits entschieden)* Opt-in-GC, WTF-16, Constant-Time, Abwicklung | siehe `SPEC.md` §3, §8, §9, §5.3 | — | **FUNDAMENT** | 2–4 |

### 10.2 Die Kurzfassung

**Sechs Dinge müssen jetzt ins Fundament** — und fünf davon kosten heute fast
nichts, weil sie Architekturentscheidungen sind und keine Merkmale:

1. **Kein `async`-Schlüsselwort.** (Punkt 1 — kostet nichts, spart alles)
2. **Fehlerunion `!T` + die Regel, dass jede Allokation fehlbar ist.**
   (Punkt 2 — nachweislich nicht nachrüstbar, siehe Linux)
3. **Keine Ambient-Autorität in der Standardbibliothek.**
   (Punkt 3 — reine Disziplin, aber unumkehrbar)
4. **Optimierungsdurchgänge einzeln schaltbar, mit Etikett, Zeileninfo
   erhaltend.** (Punkt 5 — heute billig, später ein Umbau jedes Durchgangs)
5. **Prüfphasen wiedereintrittsfähig, FIR interpretierbar.**
   (Punkt 7 — Architektur, keine Funktion)
6. **Feldzugriff vom Speicherort getrennt, Ergebnisort in der IR.**
   (Punkte 6 und 8 — **das ist die einzige teure Fundamentarbeit**, und genau
   deshalb muss sie jetzt passieren, solange das Lowering ~2.000 Zeilen hat und
   nicht 20.000)

**Vier Dinge können warten** — sie sind additiv:

* Stabiles ABI (Punkt 4) — braucht nur ein Symbolschema als Vorleistung
* Hot Reload (Punkt 9) — braucht gar nichts, lohnt sich vermutlich nie
* `comptime`/Reflexion als *Funktion* (Punkt 7b) — nur die Architektur ist
  Fundament, der Interpreter selbst nicht
* SoA als *Bibliothek* (Punkt 8b) — nur die Lowering-Trennung ist Fundament

### 10.3 Die Zielkonflikte auf einen Blick

Wo diese Ziele sich gegenseitig weh tun — vollständig, damit später niemand
überrascht ist:

| Konflikt | Auflösung in Firn |
|---|---|
| Stabiles ABI (4) ↔ Leistung ≤ 2× Rust | ABI-Stabilität ist **opt-in** pro Schnittstelle; überall sonst volle Freiheit |
| Stabiles ABI (4) ↔ SoA-Layout (8) | Ein SoA-Typ kann nicht eingefroren werden — schließt sich aus, wird dokumentiert |
| Hot Reload (9) ↔ statisches Linken (`R5`) + Inlining (`P1`) | Hot Reload wird **nicht** gebaut; Stufe A/B statt Stufe C |
| Constant-Time (`SPEC` §9) ↔ aggressiver Optimierer | `secret[T]` als Marke bis in die IR; `#[constant_time]` prüft im Codegenerator |
| Opt-in-GC (`SPEC` §3.5) ↔ heiße Pfade | `#[no_gc]` transitiv geprüft; keine Barriere außerhalb von `Gc[T]`-Feldern |
| Opt-in-GC ↔ SoA (8) | GC-Objekte brauchen einen zusammenhängenden Ort → **kein SoA für `gc class`** |
| Fehlbare Allokation (2) ↔ Ergonomie | `try` als Zeichen + Kapazität vorab + Arenen |
| Fehlbare Allokation (2) ↔ In-Place-Init (6) | Compiler räumt Teilaufbau selbst auf — das ist der schwierige Teil |
| `Io` durchreichen (1) ↔ Ergonomie | `Io` und `Allocator` gemeinsam in einem `Ctx` reichen |
| Debug-Stufen (5) ↔ Pflegeaufwand | `--dev-fast` ist eine **Teilmenge** der Release-Durchgänge, keine zweite Kette |
| Comptime `emit` (7) ↔ Compilerkomplexität | Nur wohlgeformte Elemente, **kein** freier Tokenstrom |
| Zweitklassige Referenzen (`SPEC` §3.3) ↔ SoA-Sichtwerte | **Kein Konflikt — ein Glücksfall:** Sichten können den Rahmen ohnehin nicht verlassen |

### 10.4 Was daraus für die nächste Bau-Runde folgt

Konkret und überprüfbar, in dieser Reihenfolge:

1. **Ergebnisort im Lowering und in FIR** (Punkt 6) — prüfen, ob die
   Aggregatrückgabe in `abi.rs` schon direkt ans Ziel schreibt; wenn nicht,
   umbauen. *Teuerste und dringendste Fundamentarbeit.*
2. **Feldzugriff vom Speicherort trennen** (Punkt 8) — Zwischenschicht im
   Lowering statt „Basis plus Versatz".
3. **`!T` + `#[must_consume]`** (Punkt 2) — steht ohnehin auf dem Plan für
   Phase 2.
4. **Durchgangsregister mit Etiketten** (Punkt 5) — jeder Optimierungsdurchgang
   bekommt Name, Schalter und die Angabe, ob er debugerhaltend ist.
5. **Wiedereintrittsfähige Prüfphasen** (Punkt 7) — beim Bau des Modulsystems
   berücksichtigen.
6. **Symbol-Namensschema mit Versionsplatz** (Punkt 4) — mit dem Modulsystem.

---

## Quellen

* Zig 0.16.0 Release Notes — `std.Io`, `io.async`/`io.concurrent`, `Io.Threaded`,
  `Io.Evented`, io_uring/Kqueue/Dispatch:
  <https://ziglang.org/download/0.16.0/release-notes.html>
* Zig 0.15.1 Release Notes — der I/O-Umbau („Writergate"), gepufferte
  Reader/Writer: <https://ziglang.org/download/0.15.1/release-notes.html>
* Zig `std.MultiArrayList` (SoA, Sortierung nach Ausrichtung):
  <https://github.com/ziglang/zig/blob/master/lib/std/multi_array_list.zig>
* Swift — ABI Stability and More (Swift 5, Apple-Plattformen):
  <https://www.swift.org/blog/abi-stability-and-more/>
* Swift — Library Evolution (Resilience, `@inlinable`, `@frozen`, Kosten
  indirekter Feldzugriffe): <https://www.swift.org/blog/library-evolution/>
* Swift — ABI Stability Manifesto:
  <https://github.com/apple/swift/blob/main/docs/ABIStabilityManifesto.md>
* C++26 Reflection P2996 (`^^`, `[: :]`, im Juni 2025 angenommen):
  <https://stephenberry.github.io/glaze/p2996-reflection/> ·
  <https://www.modernescpp.com/index.php/reflection-in-c26/>
* Rust-for-Linux `pin-init` (In-Place-Initialisierung, Stapelüberlauf bei großen
  Strukturen, `try_pin_init!`): <https://rust-for-linux.com/pin-init> ·
  <https://github.com/Rust-for-Linux/pin-init> ·
  <https://rust.docs.kernel.org/kernel/macro.try_pin_init.html>

---

*Dieses Dokument entscheidet Richtungen, nicht Termine. Was hier als „Fundament"
steht, muss beim nächsten Umbau des Compilers berücksichtigt werden — alles
andere darf warten, bis es weh tut.*
