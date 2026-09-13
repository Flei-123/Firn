# Einen Knopf anders aussehen lassen — Rand, Farbe, Hover

Justins Frage vom 13.09.2026:

> fUi hat auch Buttons ohne Rand — kann man das konfigurieren, Rand
> weglassen, andere Farbe usw.?

Kurz: **ja**, und zwar für jeden Zustand einzeln. Diese Seite zeigt
wie, mit den echten Funktionsnamen aus `lib/fui/style.fi` und einem
Beispiel, das sich übersetzen lässt.

Alles hier ist **gemessen** und nicht behauptet:
`tools/fui/rand_main.fi` malt jede Fassung und liest den Bildpunkt
danach zurück. Die Zahlen stehen weiter unten.

---

## Das Wichtigste zuerst: in diesem Thema ist „ohne Rand" der Normalfall

Ein Knopf von fUi hat in diesem Thema **von Haus aus keinen Rand**.
Das steht in `lib/fui/render.fi`:

```
fn def_border(kind: u32) -> u32 {
    if kind == widget.KIND_TEXTBOX {
        return style.color_token(style.TOK_BORDER)
    }
    // Buttons of this theme carry no border -- `tone=0` in
    // osum.shape: they are separated by spacing, not by a line.
    return style.color_transparent()
}
```

und `def_border_width` gibt für einen Knopf `0` zurück. Gemessen:

```
a Standard (Thema: ohne Rand)     rand ededf0  flae ededf0
```

Obere Kante und Fläche tragen **dieselbe** Farbe — da ist keine Linie.
Die eigentliche Frage ist also nicht, wie man den Rand loswird,
sondern wie man einen **hinbekommt**.

---

## Die drei Schrauben

Ein Stil ist ein Wert, kein Objekt. Jeder Setzer gibt den Stil
**zurück**, deshalb lässt sich das aneinanderhängen:

```firn
var z: style.StyleSet = style.styleset_new()
let s: *mut style.Style = style.styleset_base(&z)
*s = style.style_set_bg(*s, style.color_literal(0x0000FF))
*s = style.style_set_border(*s, style.color_literal(0xFF0000), 2)
*s = style.style_set_radius(*s, 8)
widget.w_set_styleset(&e, z)
```

### 1. Der Rand

| Was | Aufruf |
|---|---|
| Rand ganz weg | `style_set_border(s, color_transparent(), 0)` |
| eigene Farbe und Breite | `style_set_border(s, color_literal(0xFF0000), 6)` |
| nur die Breite ändern | `style_set_border_width(s, 3)` |
| Farbe aus dem Thema | `style_set_border(s, color_token(TOK_BORDER), 1)` |

Der Rand wird nur gemalt, wenn **beides** stimmt — sichtbare Farbe
**und** Breite größer null (`render.fi`, `draw_base`):

```
if style.color_is_visible(ra) && st > 0.0 {
    painter.round_ring(...)
}
```

`color_transparent()` heißt wirklich *nicht malen* und nicht *in der
Fensterfarbe übermalen*. Deshalb funktioniert auch ein Knopf über
einem Bild.

### 2. Die Fläche

| Was | Aufruf |
|---|---|
| eigene Farbe | `style_set_bg(s, color_literal(0x0000FF))` |
| keine Fläche („ghost") | `style_set_bg(s, color_transparent())` |
| Farbe aus dem Thema | `style_set_bg(s, color_token(TOK_BUTTON))` |

Ein **Ghost-Knopf** ist beides zusammen: Fläche transparent, Rand
gesetzt. Gemessen:

```
c Ghost: Rand rot, Flaeche leer   rand ff0000  flae ff00ff
```

`ff00ff` ist die grelle Messunterlage — die Fläche ist also wirklich
leer geblieben und nicht mit irgendeiner Ersatzfarbe zugemalt.

### 3. Die Ecken

`style_set_radius(s, 0)` macht einen eckigen Knopf,
`style_set_radius(s, 20)` einen runden. Gemessen wird die Ecke bei
(x+2, y+2):

```
f Radius 0:  Ecke ist gefuellt    0000ff   (der Knopf)
f Radius 20: Ecke bleibt frei     ff00ff   (die Unterlage)
```

---

## Ein eigener Hover-Stil

Das ist der Teil, der am meisten bringt und am wenigsten bekannt ist.
Neben dem Grundstil trägt ein `StyleSet` für jeden Zustand ein
**Override**:

```
STATE_NORMAL  STATE_HOVER  STATE_ACTIVE
STATE_FOCUS   STATE_DISABLED  STATE_SELECTED
```

Nur was im Override gesetzt ist, wird ersetzt; alles andere kommt aus
dem Grundstil. Ein Knopf, der beim Überfahren orange wird:

```firn
var z: style.StyleSet = style.styleset_new()
let s: *mut style.Style = style.styleset_base(&z)
*s = style.style_set_bg(*s, style.color_literal(0x0000FF))
*s = style.style_set_border(*s, style.color_transparent(), 0)

style.styleset_set_override(&z, style.STATE_HOVER,
    style.override_set_bg(style.override_new(),
        style.color_literal(0xFF9900)))
```

Gemessen:

```
g Hover-Stil: ruhend ist blau      flae 0000ff
g Hover-Stil: hover ist orange     flae ff9900
g ruhend und hover unterscheidbar  OK
```

Es geht auch **nur der Rand** — die Fläche bleibt leer, die Linie
wechselt die Farbe:

```firn
*s = style.style_set_bg(*s, style.color_transparent())
*s = style.style_set_border(*s, style.color_literal(0x0000FF), 3)
style.styleset_set_override(&z, style.STATE_HOVER,
    style.override_set_border(style.override_new(),
        style.color_literal(0xFF0000)))
```

```
h Randfarbe wechselt bei Hover   ruhend 0000ff -> hover ff0000  OK
```

Ein Override kann `bg`, `border`, `fg` und `alpha` — mehr nicht. Wer
beim Überfahren die **Breite** oder den **Radius** ändern will, kann
das über diesen Weg nicht; siehe „Was nicht geht".

---

## Farbwert oder Themenmarke?

Zwei Sorten Farbe, und der Unterschied ist Absicht:

* `color_literal(0xFF0000)` — **genau dieses Rot**, in jedem Thema.
  Für Markenfarben und für Messungen.
* `color_token(TOK_ACCENT)` — **die Rolle**, das Thema sucht die Farbe
  aus. Wechselt man von hell auf dunkel, wechselt sie mit.

Gemessen (`tools/fui/style_main.fi`):

```
3 red stays red in the dark theme   ff0000
4 token light = ededf0, dark = 42414d, und beide sind verschieden
```

Wer eine Oberfläche baut, die hell **und** dunkel können soll, nimmt
Marken. Wer eine Markenfarbe durchsetzen will, nimmt Literale.

---

## Ein vollständiges, lauffähiges Beispiel

```firn
import fui.style
import fui.widget
import fui.render

fn mach_ghost_knopf(x: f64, y: f64, txt: u64, n: usize)
    -> widget.Widget {
    var e: widget.Widget = widget.widget_new(widget.KIND_BUTTON)
    widget.w_place(&e, x, y, 150.0, 44.0)
    widget.w_set_text(&e, txt, n)

    var z: style.StyleSet = style.styleset_new()
    let s: *mut style.Style = style.styleset_base(&z)
    // keine Fläche, nur eine Linie
    *s = style.style_set_bg(*s, style.color_transparent())
    *s = style.style_set_border(*s, style.color_token(style.TOK_ACCENT), 2)
    *s = style.style_set_radius(*s, 8)
    // beim Überfahren füllt er sich
    style.styleset_set_override(&z, style.STATE_HOVER,
        style.override_set_bg(style.override_new(),
            style.color_token(style.TOK_ACCENT)))
    style.styleset_set_override(&z, style.STATE_HOVER,
        style.override_set_fg(style.override_new(),
            style.color_token(style.TOK_TEXT_ON_ACCENT)))
    widget.w_set_styleset(&e, z)
    return e
}
```

Der Knopf wird gemalt wie jeder andere:

```firn
render.draw_widget(&c, &e)
```

Den Zustand setzt der, der die Maus kennt:

```firn
widget.w_set_hover(&e, true)     // Zeiger darüber
widget.w_set_active(&e, true)    // gedrückt
widget.w_set_focus(&e, true)     // Tastaturfokus
widget.w_set_disabled(&e, true)  // ausgegraut
```

---

## Selber nachmessen

```
export FIRNLIB=$(pwd)/lib
compiler/target/release/firnc --opt-level=dev \
    -o /tmp/fuirand tools/fui/rand_main.fi
/tmp/fuirand /tmp/rand-hell.png light
/tmp/fuirand /tmp/rand-dunkel.png dark
```

Das Programm endet mit 1, wenn eine Fassung **nicht** durchschlägt —
es taugt damit als Abnahme und nicht nur als Schaubild.

---

## Was nicht geht

Ehrlich benannt, statt darum herumzureden:

| Wunsch | Lage |
|---|---|
| **Rand nur unten** (oder pro Seite) | geht **nicht**. `Style` hat ein einziges `border_w`, und `painter.round_ring` malt einen geschlossenen Ring. Wäre ein eigener Umbau: vier Breiten im `Style`, vier Farben, und ein Malweg, der offene Kanten kann. |
| **Gestrichelter Rand** | geht **nicht**. `round_ring` kennt kein Strichmuster. |
| **Randbreite/Radius im Hover ändern** | geht **nicht**. `Override` trägt nur `bg`, `border`, `fg`, `alpha` — die Zahlenfelder kommen immer aus dem Grundstil. Ein Knopf, der beim Überfahren dicker wird, braucht dafür eine Erweiterung des Override. |
| **Farbverlauf als Fläche** | `style_set_gradient` **existiert**, wirkt aber auf den **Text** (die TextMeshPro-Seite), nicht auf die Knopffläche. |
| **Fokusring getrennt einstellen** | geht **nicht**. `draw_focus_ring` nimmt fest `theme.Shape.focus_w` und `Colors.accent`. Wer ihn anders will, ändert das Thema — nicht den Knopf. |

Alle vier fehlenden Punkte sind **kleine, aber echte Umbauten an
`Style`/`Override`** und keine Zeile, die man nebenbei einschiebt.
Sie sind hier benannt, damit niemand sie im Quelltext sucht.
