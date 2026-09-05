# Runde B5 — TLS, Bilder und ein Fenster

Runde B4 hat `https://` mit Absicht abgelehnt und aufgeschrieben, was es
kosten würde: „TLS 1.3 ist eine Schicht für Datensätze, X25519, AES-GCM
oder ChaCha20-Poly1305, HKDF, SHA-256, ASN.1/DER, Kettenbau für
Zertifikate und ein Wurzelspeicher. Das ist eine eigene Runde, und alles
darunter ist eine Lüge in der Adresszeile."

Das hier ist diese Runde. Sie ist es geworden, und dazu noch das, was ohne
sie sinnlos gewesen wäre: ein Namensauflöser, ein JPEG-Dekoder, `<img>` im
Layout und ein Fenster, in das man eine Adresse tippen kann.

---

## 1. Die Zahlen

Jede Zahl unten stammt aus einem Lauf, der wirklich gefahren wurde
(`bash tools/tlsb5/run.sh`, AMD EPYC 7571, Linux x86_64, 26.08.2026). Die
Gegenstelle ist in **jedem** Fall etwas, das nicht in diesem Verzeichnis
steht: OpenSSL (über `openssl s_server`, Pythons `ssl` und das Paket
`cryptography`), libjpeg (über Pillow) und X (über `xwd`). Zwei Enden, die
denselben Standard auf dieselbe Weise missverstehen, sind sich perfekt
einig — deshalb ist eine selbstgeschriebene Gegenstelle nichts wert.

### 1.1 Die Bausteine

| | Fälle | davon **Gegenproben** |
|---|---|---|
| Ganzzahlen, X25519, ChaCha20-Poly1305, AES-GCM, SHA-384/512, HKDF, RSA, ECDSA | **647 / 647** | **49** |

Die 49 Gegenproben sind Fälle, die **scheitern müssen**: eine Signatur mit
einem gekippten Bit, ein PSS-Wert als PKCS#1 gelesen, ein AEAD-Etikett, an
dem jemand gedreht hat, ein öffentlicher Schlüssel neben der Kurve, `r`
oder `s` gleich null, ein RSA-Exponent von 1, und der Punkt kleiner
Ordnung, der bei X25519 das Nullgeheimnis erzwingt. Ohne diese Spalte
würde „647 von 647" auch für eine Bibliothek gelten, die immer `true`
zurückgibt.

Die drei Bau-Stufen (`opt`, `--no-opt`, `dev-fast`) liefern **dieselben**
647.

### 1.2 Zertifikate — und vor allem die Ablehnungen

| | Fälle | davon Ablehnungen |
|---|---|---|
| selbst erzeugte Ketten (Pythons `cryptography`) | **26 / 26** | **14** |
| echte Ketten aus dem Netz, dreimal geprüft | **18 / 18** | 12 |

Die 14 Ablehnungen sind je ein Fall, der scheitern **muss**, und sie
verlangen nicht nur „nein", sondern den **richtigen Grund**: abgelaufen,
noch nicht gültig, falscher Name (auch die drei Platzhalter-Fälle:
`*.wild.example` passt auf `a.wild.example` und **nicht** auf
`wild.example` und **nicht** auf `a.b.wild.example`), Aussteller nicht im
Speicher, Aussteller gar nicht in der Kette, Aussteller ist keine
Zertifizierungsstelle, eine Signatur, die von einem anderen Schlüssel
stammt, eine kritische Erweiterung, die niemand kennt, und eine Kurve, die
diese Runde nicht hat. Eine Kette, die als `EXPIRED` abgelehnt wird, wo
`NAME` richtig gewesen wäre, gilt als Fehler — ein `bool` hätte das
versteckt.

Die sechs echten Ketten (`tests/data/tls-chains/PROVENANCE.md`) werden
**dreimal** geprüft: wie sie sind (muss gegen `/etc/ssl/certs` gelten),
unter einem falschen Namen mit `.invalid` (muss `NAME` geben) und zu einer
Zeit nach ihrem `notAfter` (muss `EXPIRED` geben).

### 1.3 Der Handschlag

| | |
|---|---|
| Fälle gegen `openssl s_server` und das öffentliche Netz | **18 / 18** |
| davon **Ablehnungen** | **7** |
| 512 KiB über gut dreißig Datensätze, Oktett für Oktett | **identisch** |
| echte Rechner, deren Antwort Oktett für Oktett der von Pythons TLS gleicht | **3 / 3** |
| echte Rechner, die jedes Mal eine andere Seite liefern (nur Kopfzeilen verglichen) | 3 |

Die sieben Ablehnungen: ein abgelaufenes Serverzertifikat, ein falscher
Name, ein Aussteller, für den niemand im Speicher unterschrieben hat, ein
**leerer** Wurzelspeicher, ein Server, der nur TLS 1.2 spricht (er
antwortet mit Warnung 70, `protocol_version` — und genau diese Nummer wird
geprüft, damit der Fall nicht aus einem anderen Grund besteht), ein
Server, der nur eine Chiffre anbietet, die diese Runde nicht hat, und —
der wichtigste — **ein Mann in der Mitte, der genau ein Bit eines
Datensatzes umkippt**. Der muss als Entschlüsselungsfehler ankommen und
nicht als Seite.

### 1.4 `https://` im Client aus Runde B4

| | Fälle | davon Ablehnungen |
|---|---|---|
| die Naht zwischen TLS und HTTP/1.1 | **14 / 14** | **5** |

Nichts an den 28 HTTP-Regeln aus B4 hat sich geändert; sie laufen
unverändert durch (`tools/liveb4/http_check.py`, 28 / 28). Was hier
gemessen wird, ist die Naht:

* fünf `https`-Abrufe öffnen **einen** Socket und schütteln **einmal** die
  Hand (`SOCKETS 1, HANDSHAKES 1, REUSED 4`);
* eine Umleitung `http:` → `https:` und zurück wird verfolgt und öffnet
  dabei **einen zweiten** Socket, weil das Schema Teil der Identität einer
  Verbindung ist;
* dieselbe Seite über beide Schemata liefert dieselben Oktette;
* ohne Wurzelspeicher wird über `https://` **gar nichts** geholt.

### 1.5 JPEG gegen libjpeg

Elf Bilder, und gemessen wird der **größte** Unterschied eines Kanals
eines Bildpunktes — nicht ein Mittelwert. (Runde K7B hat Bildpunkte gegen
die ganze Fläche gezählt, 87 % für richtig gehalten und dabei jeden
Buchstaben verloren; der Hintergrund trug die Zahl.)

| Bild | größter Unterschied | Mittel | über der Schranke |
|---|---|---|---|
| `gradient-444` / `-420` / `-422` / `q50` | 2 | 0,01–0,07 | 0 von 9216 |
| `edges-444` / `-420` | 2 | 0,01 / 0,03 | 0 von 4800 |
| `odd-size-444` / `-420` (37 × 23) | 2 | 0,03 / 0,04 | 0 von 2553 |
| `grey` | 1 | 0,01 | 0 von 3072 |
| `big-420` (200 × 150) | 3 | 0,02 | 0 von 90000 |
| `restart-420` (mit Restart-Markern) | 3 | 0,02 | 0 von 90000 |

Dazu fünf Gegenproben, die **abgelehnt** werden müssen: ein progressives
JPEG, ein CMYK-JPEG, eine halbe Datei, ein PNG und gar nichts.

Die Schranke ist nicht null und kann es nicht sein: T.81 schreibt keine
inverse DCT vor, sondern eine Genauigkeit für eine. libjpeg rechnet in
Festkomma, diese Runde in `f64`-Kosinus.

### 1.6 `<img>`, und was diese Zahl wert ist

| | Fälle | davon Gegenproben |
|---|---|---|
| Maße, Seitenverhältnis, verzögertes Laden, Bildpunkte | **15 / 15** | **3** |

**Zwei verschiedene Sorten Messung stecken darin, und sie sind
unterschiedlich viel wert.** Die *Geometrie* wird gegen die Regeln aus
CSS 2.1 10.3.2 geprüft, wie diese Runde sie liest — das ist eine Lesart
der Norm und keine unabhängige Umsetzung, also schwächer als die Zahlen
aus B2 und B3, die von den Web Platform Tests kommen. Die *Bildpunkte*
dagegen werden gegen Pillow geprüft: ein Bild in seiner eigenen Größe muss
**genau** so auf der Leinwand ankommen, wie es hineinging, und zweifach
vergrößert muss jeder Quellpunkt ein 2 × 2-Block sein. Beides: größter
Unterschied **0**.

Die Gegenproben: ein Bild ohne Größe und ohne Maßangaben wird **nicht**
zum ersetzten Element; die gezeichnete Fläche ist nicht eine einzige
Farbe (das ist die K7B-Falle); und `loading="lazy"` fragt wirklich nach
**weniger** Bildern als dieselbe Seite ohne (3 statt 10).

### 1.7 Das Fenster, von der Serverseite gesehen

| | Fälle | davon Gegenproben |
|---|---|---|
| X11 unter Xvfb, fotografiert mit `xwd` | **10 / 10** | **1** |

Und hier steht der ehrlichste Befund der ganzen Runde. Der **erste** Lauf
von `tools/tlsb5/ui_check.py` sah so aus:

```
8 / 10 window cases
  FAIL  the page: what the server has is not one flat colour   3 distinct colours
  FAIL  the page: and there is dark ink -- the text            0 dark pixels
```

Das Fenster war da, es hatte die richtige Größe, der blaue Block war da
(24 000 Bildpunkte `#0033aa`, auf ±300 genau), der rote auch — und **kein
einziger Buchstabe**. `window_main` hatte keine Schriftdatei geladen, `fm`
war 0, und der Zeichner hat jedes Rechteck gemalt und keine Glyphe. Von
der Clientseite aus hätte man das nicht gesehen: die Anzeigeliste war
richtig, die Leinwand war „gezeichnet", der Bericht sagte „ok". Nur weil
das Bild vom **X-Server** geholt und die dunklen Bildpunkte **gezählt**
wurden, ist es aufgefallen. Das ist genau die Lehre aus K7B, und sie hat
sich in derselben Runde noch einmal ausgezahlt.

Nach dem Laden von `DejaVuSans.ttf`: 10 von 10, darunter der Vergleich,
der am meisten sagt — **die Bildpunkte, die der X-Server hat, sind genau
die Leinwand, die der Browser geschrieben hat** (größter Unterschied 0
über 786 432 Bildpunkte).

Die eine Gegenprobe: eine **leere** Seite muss auf der Serverseite genau
eine Farbe ergeben. Ohne sie könnte „das Bild hat mehr als eine Farbe"
auch für einen Browser gelten, der Rauschen malt.

### 1.8 Was nicht schlechter geworden ist

| | vor dieser Runde | danach |
|---|---|---|
| html5lib Baumbau (Runde B1) | 1837 / 1936 | **1837 / 1936** (94,89 %) |
| WPT-Layout, Korpus B2 | 59 / 186 | **59 / 186**, Reflow 471 / 471 |
| WPT-Referenzbilder, Korpus B3 | 202 / 541 | **202 / 541** (37,34 %), **32 leere abgezogen** |
| WPT-DOM, Korpus B4 | 380 / 1429 | **380 / 1429** (26,59 %), 170 konnten nicht laufen |
| ganze Dateien darin (Runde B4) | 15 (Untergrenze) | **16 / 313** |
| HTTP-Regeln (Runde B4) | 28 / 28 | **28 / 28** |
| eingegrenzte Neuberechnung (Runde B4) | 20x (Untergrenze) | **103x**, 0 falsche Rechtecke |

Zur DOM-Zeile eine Anmerkung, die dazugehört: `docs/ROUNDB4.md` schreibt
390 / 1714, die Untergrenze in `tools/liveb4/minquota.txt` steht auf 380.
Der Lauf hier trifft die **Untergrenze** genau — 380 / 1429, mit 170 statt
169 Dateien, deren Prüfstand nicht bis zum Ende kam. Die Zahl im Nenner
ändert sich mit dieser einen Datei, weil ihre Untertests dann gar nicht
gezählt werden (dieselbe Regel, die B4 sich gegeben hat). Ich habe den
Unterschied **nicht** auf diese Runde zurückverfolgen können und behaupte
deshalb auch nicht, dass er von ihr kommt; was ich sagen kann, ist, dass
der Abnahmelauf von B4 vollständig durchgeht und keine seiner Grenzen
gefallen ist.

**Die 32 sind der Punkt.** Runde B3 hat gefunden, dass 32 Referenztests
nur deshalb bestanden, weil **beide Seiten leer waren**, und sie
ausdrücklich nicht mitgezählt. Diese Runde zählt sie weiterhin nicht mit:
`B3-REF: 202 / 541 reference tests (37.34 %), 32 vacuous`. Die Quote wäre
mit ihnen 234 / 541 = 43,25 % — und wäre gelogen.

Ich habe zusätzlich nach derselben Sorte Fall in den **neuen** Messungen
gesucht:

* **Bilder:** ein `<img>`, dessen Datei fehlt und das keine `width`/
  `height` trägt, bekommt keine Größe und wird nicht als ersetztes Element
  gezählt — das ist die Gegenprobe „a picture with no size and no
  attributes is not given one", und sie verlangt `IMAGES 0`. Ohne sie
  hätte jede Seite mit einem kaputten Bild als „Bild gelayoutet" gezählt.
* **JPEG:** eine halbe Datei ergab im ersten Lauf ein halbes Bild, das
  völlig plausibel aussah. Jetzt bricht der Entropie-Dekoder ab, sobald
  die Daten vor dem letzten MCU ausgehen (`JR_TRUNCATED`), und der Fall
  ist eine Gegenprobe.
* **Fenster:** siehe 1.7 — die leere Seite als Gegenprobe.
* **TLS:** die drei Rechner, deren Seite sich bei jedem Abruf ändert
  (github, Cloudflare, Google), werden **getrennt** ausgewiesen und nicht
  unter „Oktett für Oktett gleich" gezählt. Ob eine Seite überhaupt
  vergleichbar ist, entscheiden **fünf** Abrufe (dreimal Python, zweimal
  dieser Client); wären nur die Python-Abrufe gefragt worden, hätte
  Cloudflare zufällig als deterministisch gegolten und dieser Client wäre
  zu Unrecht als falsch gemeldet worden. Genau das ist im ersten Lauf
  passiert.

### 1.9 Was es kostet

Gemessen auf derselben Maschine, ehrlich niedrig:

| | |
|---|---|
| ein vollständiger TLS-1.3-Handschlag gegen `localhost`, inklusive Prozessstart | **24 ms** (5 Läufe in 120 ms) |
| X25519 einmal, inklusive Prozessstart | **7 ms** |
| AES-128-GCM, Nutzdaten | **2,9 MiB/s** |
| ChaCha20-Poly1305, Nutzdaten | **5,1 MiB/s** |

Beides ist langsam, und beides hat einen benannten Grund. Das GHASH von
`gcm.fi` ist die bitweise Fassung aus SP 800-38D: 128 Durchläufe pro
16 Oktetten. Eine Tabelle über 4 Bit ist etwa achtmal schneller, und der
`__pclmulqdq`-Befehlssatz, den dieser Übersetzer seit Runde 82 kennt, noch
einmal etwa zwanzigmal. Keines von beidem steht hier, weil ein falscher
MAC ein Sicherheitsloch ist und ein langsamer eine Zahl in einer Tabelle.
ChaCha20 ist ohne SIMD geschrieben und deshalb dort, wo es ist.

---

## 2. Was gebaut wurde

### 2.1 Eine Arithmetik für drei Kurven und ein RSA

`lib/std/crypto/big.fi`. X25519, P-256, P-384 und RSA brauchen dasselbe:
„multipliziere zwei Zahlen von ein paar hundert Bit und reduziere modulo
einer dritten". Also gibt es das **einmal**.

Zahlen sind Glieder von 32 Bit in einem festen Feld — fest und nicht
wachsend, weil diese Datei mitten aus einem TLS-Handschlag erreicht wird
und ein Allokator an dieser Stelle eine Fehlerart ist, die niemand
debuggen will. Die Glieder stehen als `u32` in der Struktur und werden in
`u64` gerechnet, und das ist keine Bequemlichkeit, sondern **die** Schranke,
die die ganze Datei sicher macht:

```
(2^32-1) * (2^32-1) + (2^32-1) + (2^32-1) = 2^64 - 1
```

genau. Ein Gliedprodukt plus ein Sammelglied plus ein Übertragsglied passt
in ein `u64` ohne Rest und ohne Reserve. Firn bricht bei
Ganzzahlüberlauf ab (`u64 + u64`, das umläuft, ist ein Absturz und keine
stille 0) — eine Umsetzung, die diese Schranke verfehlt, würde also keine
falsche Signatur berechnen, sondern stehen bleiben.

**Warum Montgomery.** Der offensichtliche Weg wäre Division. Knuths
Algorithmus D ist eine Seite Code mit einem Korrekturschritt, den ungefähr
eine von 2^32 Eingaben auslöst — was eine andere Art ist zu sagen: eine
Seite Code, die nie getestet wird. Montgomery ersetzt die Division durch
eine Multiplikation mit einer Konstanten und eine Verschiebung, hat außer
einer abschließenden bedingten Subtraktion keinen datenabhängigen Zweig
und ist für jeden ungeraden Modul derselbe Algorithmus. `R^2 mod m` wird
**ohne** Division gebaut: bei 1 anfangen und 2·32·n mal verdoppeln.

Die abschließende Subtraktion ist **zweigfrei** geschrieben (Maske statt
Sprung), damit die X25519-Leiter darauf laufen darf.

### 2.2 X25519

`lib/std/crypto/x25519.fi`. Die einzige Funktion der ganzen Runde, deren
Eingabe **geheim** ist. Die Montgomery-Leiter aus RFC 7748 5, mit einem
`cswap`, das mit einer Maske und nicht mit einem Sprung arbeitet:

```
t = mask & (a ^ b);  a = a ^ t;  b = b ^ t
```

Kein Sprung und keine Speicheradresse hängt vom Skalar ab. Das Klemmen
(die unteren drei Bit löschen, Bit 254 setzen, Bit 255 löschen) ist nicht
Kosmetik: eine Umsetzung, die es weglässt, arbeitet mit jedem Gegenüber
perfekt zusammen und ist kaputt — die schlimmste Kombination, die es gibt.
Das Null-Ergebnis ist ein Fehler und wird als solcher gemeldet (RFC 7748
6.1).

Die Feldarithmetik ist die allgemeine Montgomery-Multiplikation aus 2.1.
Die Alternative — zehn Glieder zu 25,5 Bit und eine von Hand für diese
eine Primzahl geschriebene Reduktion — ist etwa dreimal schneller und eine
Seite Indexrechnung, die nur ihr Autor prüfen kann. Diese Runde hat die
Fassung gewählt, die ihre Multiplikation mit RSA und P-256 teilt und
deshalb dreifach geprüft ist.

### 2.3 P-384, weil das echte Netz es verlangt

`lib/std/crypto/ecdsa.fi` kann P-256 **und** P-384, und das ist keine
Vollständigkeitsübung. Die allererste echte Kette, die diese Runde probiert
hat — `example.com`, hinter Cloudflare — hat ein P-384-Zwischenzertifikat
und eine P-384-Wurzel, und mit P-256 allein kam die ganze Kette als
`PARSE` zurück. Ein Browser ohne P-384 kann einen großen Teil des Netzes
nicht öffnen.

Beide Kurven haben `a = -3`, deshalb dient **ein** Stück Arithmetik
beiden; alles, was sich unterscheidet, ist eine Zahl in einer `Curve` und
keine Zeile Rechnung. Die Konstanten wurden nicht abgeschrieben, sondern
**geprüft**: der Erzeuger hat für beide Kurven nachgerechnet, dass `G` auf
`y² = x³ − 3x + b` liegt und dass `n·G` der unendlich ferne Punkt ist, mit
einer unabhängigen affinen Umsetzung.

Und alle **vier** Bedingungen aus SEC 1, 4.1.4 werden geprüft, weil jede
einzelne für sich eine Fälschung ist: `r` und `s` in `[1, n-1]`, der
öffentliche Schlüssel wirklich auf der Kurve, `R` nicht der unendlich
ferne Punkt, `R.x mod n == r`.

### 2.4 Zwei Polsterungen für RSA

`lib/std/crypto/rsa.fi` prüft nur — nicht signieren, nicht entschlüsseln.
Das ist die sichere Hälfte: alles hier arbeitet auf Daten, die ein
Angreifer ohnehin hat.

PKCS#1 v1.5 wird **gebaut und verglichen**, nicht zerlegt. Ein Prüfer, der
die Signatur zerlegt und den Hash herausklaubt, hat den
Bleichenbacher-Fehler von 2006 — er akzeptiert Signaturen, die nie mit dem
Schlüssel gemacht wurden, weil er hinter dem Hash weiterliest. Ein
erwarteter Block, der gebaut und Oktett für Oktett verglichen wird, kann
diesen Fehler nicht haben, weil es nichts zu überlesen gibt.

RSA-PSS ist zufällig und muss zerlegt werden, und dabei ist der Fehler
passiert, den die Messung gefunden hat: RFC 8017 9.1.2 Schritt 6 und
Schritt 9 sprechen über **verschiedene** Oktette. Schritt 6 verlangt, dass
die obersten Bits von **maskedDB** (also von `EM` selbst) null sind;
Schritt 9 setzt die obersten Bits des **entmaskierten DB** auf null. Die
erste Fassung hat beides am entmaskierten geprüft — und damit **jede**
gültige Signatur abgelehnt. Vier Fälle von 647 haben es gezeigt.

### 2.5 Der Schlüsselplan

`lib/tls/keys.fi`. Elf Werte aus einem, und der ganze Sinn ist, dass die
Kenntnis eines Werts über die anderen nichts verrät. **Jeder Schritt nimmt
einen Abschriftshash.** Das ist es, was die Schlüssel an genau die
Nachrichten bindet, die ausgetauscht wurden: wer ein Oktett des
ClientHello ändert — eine Chiffre herausnimmt, etwa — ändert jeden
Schlüssel danach, und die Verbindung scheitert, statt still mit der
schwächeren Wahl zu laufen. Das ist die Reparatur für die ganze Familie
der Rückstufungsangriffe, für die TLS 1.2 einen eigenen Mechanismus
brauchte.

### 2.6 Der Handschlag

`lib/tls/tls.fi`, rund 1 100 Zeilen. Angeboten werden **zwei** Chiffren
(`TLS_AES_128_GCM_SHA256`, das RFC 8446 9.1 verpflichtend macht, und
`TLS_CHACHA20_POLY1305_SHA256`) und **eine** Gruppe (X25519). Das ist ein
bewusst schmales Angebot: jede Gruppe und jede Chiffre in einem
ClientHello ist ein Codepfad, der stimmen muss, und drei Gruppen mal fünf
Chiffren wären fünfzehn Kombinationen, von denen eine falsch wäre.

Zwei Signaturen tun verschiedene Arbeit und beide werden geprüft: die
Zertifikatskette sagt, **wem** der Schlüssel gehört; `CertificateVerify`
sagt, dass das Gegenüber ihn **hat**, indem es die Abschrift unterschreibt.
Ohne das zweite könnte jeder ein Zertifikat wiederholen, das er irgendwo
aufgesammelt hat. `rsa_pkcs1_*` wird in `CertificateVerify` **abgelehnt**
(RFC 8446 4.2.3 verbietet es dort), auch wenn es in Zertifikaten erlaubt
ist.

### 2.7 Der Zeiger, der ins Leere zeigte

Der erste Handschlag gegen `openssl s_server` kam mit `ERRSignature`
zurück, und die Signatur war völlig in Ordnung. Die `Cert`-Strukturen
zeigen in den Puffer, in dem die Handschlagnachrichten liegen — und dieser
Puffer wird **verdichtet und neu zugeteilt**, während der Rest des
Handschlags eintrifft. Bis `CertificateVerify` ankam, zeigte der
öffentliche Schlüssel des Blattzertifikats auf Speicher, der inzwischen
etwas anderes war.

Die Zertifikatsnachricht wird jetzt in einen eigenen, stillen Puffer
kopiert, bevor sie zerlegt wird (`Tls.certbuf`). Das ist der Sorte Fehler,
die ausgeliefert wird: er tritt nur auf, wenn die Nachrichten in einer
bestimmten Reihenfolge und Größe eintreffen, und er sieht wie ein
Kryptofehler aus.

### 2.8 Der Wurzelspeicher und die Kette

`lib/tls/x509.fi` baut die Kette vom Blatt aus: an jedem Schritt wird das
Zertifikat gesucht, dessen **Betreff** dem **Aussteller** des aktuellen
gleicht — erst unter denen, die das Gegenüber geschickt hat, dann im
Wurzelspeicher. Verglichen werden die **rohen DER-Oktette** des Namens.
Das ist die strenge Lesart (RFC 5280 7.1 erlaubt einen normalisierenden
Vergleich) und die, bei der zwei Umsetzungen sich nicht uneinig sein
können. Die Reihenfolge, in der das Gegenüber die Zertifikate schickt,
wird **nicht** geglaubt (RFC 8446 4.4.2 sagt ausdrücklich, ein Empfänger
dürfe sich nicht darauf verlassen).

`lib/tls/der.fi` ist die größte Angriffsfläche der Runde: sie läuft, bevor
irgendeine Signatur geprüft ist, auf Daten, die ein Angreifer geschickt
hat. Deshalb: keine Zuteilung, keine vom Eingang gesteuerte Rekursion,
jede Länge gegen das wirklich Vorhandene geprüft, und die unbestimmte
Längenform (`0x80`) sowie nicht-minimale Längen **abgelehnt** — genau die
Stellen, an denen zwei Zerleger uneins werden können, wo eine
Certificate-Kette endet.

### 2.9 Ein Namensauflöser über TCP

`lib/net/dns.fi`. Runde B4 hat das als zweite Grenze benannt; mit TLS war
es das Letzte, was den Browser noch von einer echten Seite trennte.

DNS über **TCP** statt UDP, und das ist ein bewusster Tausch: es steht seit
RFC 1035 4.2.2 in der Norm, jeder brauchbare Auflöser antwortet darauf,
und es braucht **keinen neuen Systemaufruf** — `lib/std/net.fi` hat
`connect_tcp`, `read` und `write_all`, und das ist der ganze Transport. Es
kostet eine Umlaufzeit mehr, und ein Auflöser, der TCP sperrt, ist einer,
den dieser Browser nicht nutzen kann. Beides steht in der Datei.

Der Zerleger ist der klassische feindliche Eingang: DNS-Namen haben
**Verdichtungszeiger**, und ein Zeiger darf rückwärts, vorwärts oder auf
sich selbst zeigen. Ein Zerleger, der ihnen naiv folgt, läuft ewig auf
einer Nachricht, die ein Angreifer in vier Oktetten schreibt. Hier muss
jeder Zeiger **streng rückwärts** zeigen **und** es werden höchstens
`MAX_JUMPS` verfolgt — beides, nicht eines: die Rückwärtsregel allein
erlaubt noch eine Kette von 16 000 Ein-Oktett-Sprüngen durch eine 64-KiB-
Nachricht.

### 2.10 Der JPEG-Dekoder

`lib/paint/jpeg.fi`, Baseline (SOF0/SOF1) bei acht Bit. Alles andere wird
**mit Namen** abgelehnt statt zu einem grauen Rechteck zu werden.

Die Aufwärtsabtastung ist der Teil, der die Zahlen macht. Der
naheliegende Weg — jede Farbprobe zweimal wiederholen — weicht von libjpeg
an einer scharfen Farbkante um bis zu dreißig Stufen ab, und die Messung
hätte dann den Aufwärtsabtaster gemessen und sonst nichts. Also steht hier
dasselbe Dreiecksfilter (`(3·nah + fern + 1) / 4`) für die beiden
Verhältnisse, die in der Praxis vorkommen. Ergebnis: die 4:2:0-Bilder
weichen genauso wenig ab wie die 4:4:4-Bilder, nämlich um 2 bis 3 Stufen —
und das ist die inverse DCT und nichts weiter.

### 2.11 `<img>` als ersetztes Element

Die Regeln aus CSS 2.1 10.3.2/10.4 stehen in `box.replaced_size`, vier
Fälle, und der dritte und vierte sind die, die vergessen werden: Breite
gegeben → Höhe folgt dem Verhältnis; Höhe gegeben → Breite folgt; beides
`auto` → die eigene Größe.

Die Bilder werden **vor** der Geometrie angehängt
(`lib/browser/images.fi`), und wenn das Bild noch nicht da ist, kommt die
Größe aus den `width`/`height`-**Attributen** (HTML 4.10.20.7). Das ist
nicht Bequemlichkeit: eine Seite, deren Bilder keine Größe haben, wird mit
Nullhöhe gelayoutet und springt, sobald jedes Bild eintrifft — der
sichtbarste Layoutfehler, den es im Netz gibt.

### 2.12 Das Fenster

`lib/browser/x11.fi` spricht das X11-Protokoll direkt über einen
Unix-Socket — kein Xlib, kein xcb, keine C-Bibliothek, weil Firn keine hat
und der Sinn der Sprache ist, dass sie keine braucht. Acht Anfragen und
vier Ereignisse reichen für ein Fenster mit Adresszeile,
Zurück/Vorwärts/Neuladen, Bildlauf und Verlauf.

`lib/std/net.fi` hat dafür `connect_unix` bekommen: ein Socket im
Dateisystem statt im Netz. Der X-Server lauscht auf einem modernen
Rechner auf nichts anderes.

---

## 3. Was nicht geht, und warum

Offen aufgeschrieben, weil eine Liste von Grenzen mehr wert ist als eine
Quote, die sie versteckt.

**TLS**

* **Keine Wiederaufnahme, kein 0-RTT, kein PSK.** Jede Verbindung ist ein
  voller Handschlag; ein `NewSessionTicket` wird gelesen und weggeworfen.
* **Kein Key Update** (RFC 8446 4.6.3). Ein `key_update` vom Server
  beendet die Verbindung mit einer Warnung, statt ignoriert zu werden —
  ignorieren hieße, ab da mit dem falschen Schlüssel zu entschlüsseln und
  eine kaputte Seite zu melden.
* **Kein HelloRetryRequest.** Er wird **erkannt** (am magischen Zufallswert
  aus RFC 8446 4.1.3) und als `Retry` gemeldet, nicht als ServerHello
  missverstanden.
* **Kein Clientzertifikat**, kein `TLS_AES_256_GCM_SHA384` (weil
  `lib/std/crypto/aes.fi` seit Runde 81 nur AES-128 kann), keine anderen
  Gruppen als X25519.
* **Keine Sperrprüfung**: kein CRL, kein OCSP, kein Stapling. Ein
  Zertifikat, das heute früh zurückgezogen wurde, gilt hier noch. Das ist
  ein echtes Loch.
* **Keine Namensbeschränkungen** (`nameConstraints`), keine
  Richtlinienbeschränkungen. Eine Zertifizierungsstelle, die nur für
  `.example` ausstellen darf, gilt hier als eine, die für alles darf.
* **Nur P-256 und P-384**, kein P-521, kein Ed25519, kein DSA.

**Bilder**

* **Kein progressives JPEG** (SOF2). Etwa ein Drittel der Fotos im Netz
  ist progressiv; das ist der größte einzelne Posten dieser Liste.
* Keine arithmetische Kodierung, keine 12 Bit, kein CMYK, keine
  EXIF-Drehung.
* Der Bildzeichner nimmt den **nächsten Nachbarn**. Ein Bild, das kleiner
  gezeigt wird als es ist, flimmert sichtbar.
* Kein `srcset`, kein `<picture>`, kein GIF, kein WebP, kein SVG.

**Netz**

* **Kein Bildabruf im Fenster.** `window_main` holt das Dokument, aber
  nicht seine Bilder — `images.collect_pending` sagt, welche fehlen, und
  niemand ruft sie ab. `b5_main` bekommt die Bilder mit dem Auftrag, weil
  dort das Layout und das Zeichnen gemessen werden. Das ist die
  auffälligste Lücke der Runde: das Fenster zeigt Text und Kästen und
  keine Fotos.
* **Kein Auflöser-Zwischenspeicher**, keine TTL. Jeder Abruf löst neu auf.
* Kein IPv6 (`lib/std/net.fi` hat keinen), kein HTTP/2, kein `fetch`.

**Fenster**

* Kein Fensterverwaltungsprotokoll über den Titel hinaus; das Schließen
  mit der Maus beendet den Prozess mit einem Ein-/Ausgabefehler.
* Die ganze Leinwand wird bei jedem Zeichnen geschickt (bei 1024 × 768
  sind das 3 MB) — ein Schadensrechteck wäre der nächste Schritt.
* Der Bildlauf ist ein **Ausschnitt**, kein zweites Layout: die Seite wird
  einmal in eine Leinwand so hoch wie das Dokument gemalt. Deshalb kostet
  Bildlauf nichts und eine sehr lange Seite kostet Speicher.
* Keine Verweise: man kann nichts anklicken. Die Adresszeile ist der
  einzige Weg zu einer neuen Seite.
* Die Tastatur ist die US-Belegung, für den ASCII-Bereich.

**Und das Offensichtliche:** es gibt keine Skripte im Fenster. `b4_main`
kann JavaScript, `window_main` nicht — die beiden Treiber sind noch nicht
derselbe.

---

## 4. Wie geprüft wurde, und warum nicht anders

### 4.1 Die Gegenstelle darf nicht von hier sein

Das ist dieselbe Regel wie in B4 und sie hat wieder etwas gefunden. Ein
selbstgeschriebener TLS-Server hätte dieselben Annahmen gemacht wie dieser
Client, und beide wären sich perfekt einig gewesen. `openssl s_server`
war es nicht: der Zeiger aus 2.7 wäre gegen eine eigene Attrappe nie
aufgefallen, weil die Nachrichten dort in einer anderen Reihenfolge und
Größe eingetroffen wären.

### 4.2 Die Gegenproben sind die Messung

Über die ganze Runde: **49 + 14 + 7 + 5 + 5 + 3 + 1 = 84 Fälle, die
scheitern müssen**, von 746 Fällen insgesamt. Ein Prüfer, der alles
durchlässt, besteht jede positive Prüfung, die es gibt.

| Behauptung | die Gegenprobe |
|---|---|
| die Kette wird geprüft | abgelaufen / falscher Name / unbekannter Aussteller / gefälschte Signatur, je ein Fall, und der **Grund** muss stimmen |
| der AEAD schützt | ein Mann in der Mitte kippt ein Bit eines Datensatzes |
| ohne Wurzeln kein `https` | leerer Wurzelspeicher, und nichts wird geholt |
| der Client spricht wirklich TLS 1.3 | ein Server, der nur 1.2 kann, muss mit Warnung 70 abgelehnt werden |
| das Bild wird gezeichnet | die gezeichnete Fläche ist nicht eine einzige Farbe |
| das Fenster zeigt die Seite | eine **leere** Seite muss auf der Serverseite eine einzige Farbe sein |
| `loading="lazy"` wird gelesen | dieselbe Seite ohne `lazy` fragt nach **mehr** Bildern |
| der JPEG-Dekoder erkennt Schrott | halbe Datei, PNG, CMYK, progressiv, nichts |
| B1–B4 sind unberührt | die vier Korpora, unverändert |

### 4.3 Zwei Stilprüfer, und was sie gefunden haben

Der **Längenprüfer** (`tools/english/check_lengths.py`) hat wieder
zugeschlagen, genau wie in B4:

```
LAENGE lib/browser/window_main.fi:338  lbl uebergibt 1, Text ist 3 ('<>R')
```

Drei Beschriftungen zu je einem Zeichen in einem Feld von drei, mit
`(&lbl[0]) as u64, 1` und einem Index darauf. Das *war* gewollt — und der
Prüfer hat recht: das ist die Form eines echten Fehlers, und eine
Beschriftung, die zufällig ein Zeichen lang ist, ist keine Ausnahme im
Prüfer wert. Jetzt sind es drei Felder zu je einem Zeichen.

Der **Englischprüfer** hat einen Treffer gemeldet: `tests` im Pfad
`tests/data/fonts/FirnSans.ttf`. Das ist der Verzeichnisname dieses
Repositoriums und kein deutsches Wort — als Ausnahme eingetragen, mit
Begründung, wie die Einträge davor.

`firnfmt -c` über alle neuen und geänderten Dateien: **elf** standen nicht
in kanonischer Form (`sha512.fi`, `x25519.fi`, `rsa.fi`, `keys.fi`,
`tls_main.fi`, `dns.fi`, `jpeg.fi`, `painter.fi`, `box.fi`, `x11.fi`,
`window_main.fi`). Nach `firnfmt -w`: keine mehr, und alle sieben
Wurzeldateien übersetzen weiter.

---

## 5. Wo der Code liegt

```
lib/std/crypto/big.fi        Ganzzahlen und Montgomery
lib/std/crypto/x25519.fi     der Schlüsseltausch
lib/std/crypto/chacha.fi     ChaCha20, Poly1305, der AEAD
lib/std/crypto/gcm.fi        GHASH und AES-128-GCM
lib/std/crypto/sha512.fi     SHA-384/512
lib/std/crypto/hkdf.fi       HKDF (RFC 5869)
lib/std/crypto/rsa.fi        RSA-Prüfung, PKCS#1 und PSS
lib/std/crypto/ecdsa.fi      P-256 und P-384, ECDSA-Prüfung
lib/tls/der.fi               ASN.1/DER
lib/tls/x509.fi              Zertifikate, Kette, Wurzelspeicher
lib/tls/keys.fi              der Schlüsselplan aus RFC 8446 7.1
lib/tls/tls.fi               Datensatzschicht und Handschlag
lib/net/dns.fi               Namen zu Adressen
lib/net/http.fi              der Client aus B4, jetzt mit https
lib/paint/jpeg.fi            der JPEG-Dekoder
lib/browser/images.fi        der Bildspeicher und das Anhängen
lib/browser/x11.fi           das X11-Protokoll
lib/browser/b5_main.fi       die Seite mit ihren Bildern, als Bild
lib/browser/window_main.fi   der Browser als Fenster
tools/tlsb5/                 der Abnahmelauf und die acht Messungen
tests/data/tls-chains/       sechs echte Ketten (PROVENANCE.md)
```

Anderswo geändert, und nur das: drei Felder auf `layout.box.Box`
(`img`, `iw`, `ih`), ein Zweig in `layout/flow.fi` für ersetzte Elemente,
ein Befehl in `paint/display.fi` und seine Zeichenfunktion in
`paint/painter.fi`, `connect_unix` in `std/net.fi`, und in `net/http.fi`
die TLS-Naht.

---

## 6. Was als Nächstes dran wäre

In der Reihenfolge, in der es weh tut:

1. **Bilder im Fenster abrufen.** Alles dafür ist da — der Speicher weiß,
   was fehlt, der Client kann `https`, der Dekoder kann PNG und JPEG.
   Es fehlt die Schleife.
2. **Progressives JPEG.** Ein Drittel der Fotos.
3. **Verweise anklicken.** Der Browser hat einen Kastenbaum mit
   Koordinaten und einen DOM daneben; ein Treffer-Test ist eine Suche im
   Baum.
4. **Sperrprüfung** (OCSP-Stapling ist eine TLS-Erweiterung und der
   billigste Weg).
5. **Das GHASH beschleunigen** — 4-Bit-Tabelle, dann `pclmulqdq`.
6. **`window_main` und `b4_main` zusammenführen**, damit im Fenster
   Skripte laufen.
