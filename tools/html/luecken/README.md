# tools/html/luecken/ — was die Baumkonstruktion (noch) NICHT kann

DIESE FAELLE SCHLAGEN FEHL — MIT ABSICHT.

Sie halten fest, was die Baumkonstruktion aus Runde 54 NICHT kann. Die
erwarteten Baeume sind die RICHTIGEN (aus dem WHATWG-Standard, gegen
html5lib 1.1 geprueft). Der Laeufer faehrt sie getrennt von der Hauptquote
(tools/html/harness_baum.py --luecken) und weist sie eigens aus.

Warum ueberhaupt: eine Testsuite, die nur enthaelt, was schon geht, sagt
nichts ueber das, was fehlt. Diese Datei ist die Gegenrechnung.

1..5  Fremdinhalt (SVG/MathML): Namensraum am Wurzelelement gibt es, der
Regelsatz fuer den INHALT fehlt (Namenskorrektur, Attributanpassung,
Integrationspunkte, Ausbruchtags).
6..9  `<template>`: der 23. Einfuegemodus und der eigene Inhaltsbaum.
10    Fragmentzerlegung mit Kontextelement (`innerHTML`).

Die erwarteten Baeume in `bekannte_luecken.dat` sind die RICHTIGEN (aus dem
WHATWG-Standard). Der Laeufer faehrt sie getrennt von der Hauptquote:

    python3 tools/html/harness_baum.py <binary> --luecken

Sie werden eigens ausgewiesen und gehen NICHT in die Quote von
`tools/html/cases/` ein. Eine Testsuite, die nur enthaelt, was schon geht,
sagt nichts ueber das, was fehlt — diese Datei ist die Gegenrechnung.

Die vier `<template>`-Faelle traegt `#orakel-abweichung`: html5lib 1.1 legt
den Template-Inhalt nicht in einem eigenen Inhaltsbaum ab, kann die Erwartung
also nicht bestaetigen. Sie ist von Hand aus dem Standard geschrieben.
