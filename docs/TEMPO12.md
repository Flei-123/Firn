# TEMPO 12 -- zwei Versuche, beide gemessen und NICHT uebernommen

Stand vorher: TEMPO 11, MP3-Dekoder 8 s Ton = 128,5 Mio Befehle.
Der Code liegt auf dem Zweig `tempo12-versuch` (Commit 67548aa4).

## Die Frage

`l3_huffman` braucht 20,0 Mio Befehle, C 12,1 Mio. Der Zustand des
Bitlesers (`HuffLage { cache, sh, next }`) liegt die ganze Funktion ueber im
Rahmen, weil der Dekoder einen Zeiger darauf an die Helfer gibt.
`FIRN_RA_STATS`: 958 Werte, 163 in Registern, **maxlive = 86**, 92 Werte
kreuzen einen Aufruf.

## Versuch 1: SROA (`sroa.rs`)

Ein `alloca`, dessen Adresse nur als `load`/`store`-Adresse benutzt wird
(direkt oder ueber `ptradd` mit konstantem Versatz), wird in eine Zelle je
Feld zerlegt; `mem2reg` befoerdert die Felder danach.

Er greift: die Speicherzugriffe in `l3_huffman` fallen im FIR von 110 auf 42.
Und das Programm wird **langsamer: 128,5 -> 132,1 Mio (+2,8 %)**,
`l3_huffman` 20,0 -> 23,6 Mio. Die drei Felder werden zu Werten, die ueber
alle Schleifen leben; bei 86 gleichzeitig lebenden Werten auf 14 Registern
bekommen sie keins, und statt eines Speicheroperanden stehen jetzt
Rahmenkopien an jeder Rueckwaertskante. Auf der Benchmark-Bank: keine
Aenderung.

## Versuch 2: caller-saved Register ueber einen Aufruf retten

Bisher: ein Wert, der einen Aufruf kreuzt, bekommt nur eines der fuenf
callee-saved Register, eine Gleitzahl gar keins. Neu: jedes Register, und
am Aufruf wird abgelegt/zurueckgeholt, wenn das billiger ist als der Wert im
Rahmen (Kosten = 2 x Gewicht der gekreuzten Aufrufe).

| Variante | MP3 (Mio) |
|---|---|
| aus | 128,5 |
| Ganzzahl + Gleitzahl, Faktor 1..100 | 129,3 -- 134,0 |
| nur Gleitzahl | +0,01 % |

Warum es verliert: Die Zuteilung ist ein linearer Durchlauf nach Beginn.
Werte, die frueher gar kein Register bekamen, nehmen jetzt die callee-saved
Register zuerst, und die Werte, die sie vorher hatten, landen in
caller-saved Registern und muessen an JEDEM Aufruf gesichert werden
(`l3_imdct36`: fuenf Sicherungen um zwei Aufrufe von `l3_dct3_9` je Band).
`fib` gewann in der ersten Fassung 5,4 %, nach der Korrektur nichts mehr.

## Was daraus folgt

Beide Versuche scheitern an derselben Stelle: die Intervalle haben keine
**Luecken**. Ein Wert lebt vom ersten bis zum letzten Beruehren am Stueck;
in `l3_huffman` sind das 86 gleichzeitig, obwohl in der heissen
count1-Schleife vielleicht zwoelf wirklich gebraucht werden. Solange das so
ist, verschiebt jede Aenderung nur, WER im Rahmen landet. Der naechste
echte Schritt ist ein Zuteiler mit Lebensdauer-Luecken (Intervall = Liste von
Stuecken, wie bei Wimmer/Franz) -- danach lohnen sich SROA und die
Aufrufsicherung vermutlich von selbst, und sie liegen fertig auf dem Zweig.
