//! Diagnosen: Sammeln und Ausgeben von Fehlermeldungen mit Datei, Zeile, Spalte,
//! Quelltextzeile und Markierung.
//!
//! Ausgabeformat (verbindlich, Negativtests haengen daran):
//!
//! ```text
//! error: erwartet ')' nach Argumentliste
//!   --> tests/neg/bad_call.fi:7:22
//!    |
//!  7 |     let x = add(1, 2 ;
//!    |                      ^ hier
//! ```

/// Quelltextposition. `line`/`col` sind 1-basiert, `col` und `len` zaehlen
/// ZEICHEN (nicht Bytes), damit die Markierung unter UTF-8 stimmt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

impl Span {
    pub fn new(line: u32, col: u32, len: u32) -> Span {
        Span {
            line,
            col,
            len: if len == 0 { 1 } else { len },
        }
    }
    /// Platzhalterposition fuer Diagnosen ohne echten Ort.
    pub fn none() -> Span {
        Span {
            line: 0,
            col: 0,
            len: 1,
        }
    }
    pub fn is_none(&self) -> bool {
        self.line == 0
    }
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub msg: String,
    pub span: Span,
    pub label: String,
    pub note: Option<String>,
}

/// Sammelt Diagnosen fuer EINE Uebersetzungseinheit.
pub struct Diags {
    file: String,
    lines: Vec<String>,
    items: Vec<Diag>,
    /// Obergrenze, damit kaputte Eingaben keine Fehlerlawine erzeugen.
    max: usize,
}

const TABWIDTH: usize = 4;

fn expand_tabs(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c == '\t' {
            for _ in 0..TABWIDTH {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Diags {
    pub fn new(file: &str, src: &str) -> Diags {
        Diags {
            file: file.to_string(),
            lines: src.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect(),
            items: Vec::new(),
            max: 40,
        }
    }

    /// Fehler mit Standardmarkierung ("hier").
    pub fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: "hier".to_string(),
            note: None,
        });
    }

    /// Fehler mit eigener Beschriftung unter der Markierung.
    pub fn error_label(&mut self, span: Span, msg: impl Into<String>, label: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: label.into(),
            note: None,
        });
    }

    /// Fehler mit zusaetzlicher Erklaerungszeile ("hinweis: ...").
    pub fn error_note(&mut self, span: Span, msg: impl Into<String>, note: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: "hier".to_string(),
            note: Some(note.into()),
        });
    }

    fn push(&mut self, d: Diag) {
        // Doppelte Meldungen an derselben Stelle unterdruecken (Fehlerwiederherstellung).
        if self
            .items
            .iter()
            .any(|o| o.span == d.span && o.msg == d.msg)
        {
            return;
        }
        if self.items.len() < self.max {
            self.items.push(d);
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.items.is_empty()
    }
    pub fn count(&self) -> usize {
        self.items.len()
    }
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.max
    }
    pub fn file(&self) -> &str {
        &self.file
    }
    pub fn items(&self) -> &[Diag] {
        &self.items
    }

    /// Die Quelltextzeile (1-basiert) ohne Zeilenende, oder "".
    pub fn source_line(&self, line: u32) -> &str {
        if line == 0 {
            return "";
        }
        self.lines.get((line - 1) as usize).map(|s| s.as_str()).unwrap_or("")
    }

    /// Alle gesammelten Diagnosen als Text.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for d in &self.items {
            out.push_str(&self.render_one(d));
        }
        if self.items.len() > 1 {
            out.push_str(&format!("{} Fehler gefunden\n", self.items.len()));
        }
        out
    }

    fn render_one(&self, d: &Diag) -> String {
        let mut out = String::new();
        out.push_str(&format!("error: {}\n", d.msg));
        if d.span.is_none() {
            out.push_str(&format!("  --> {}\n", self.file));
            if let Some(n) = &d.note {
                out.push_str(&format!("  hinweis: {}\n", n));
            }
            return out;
        }
        let nstr = d.span.line.to_string();
        let w = nstr.len();
        let pad = " ".repeat(w + 1);
        out.push_str(&format!(
            "{}--> {}:{}:{}\n",
            pad, self.file, d.span.line, d.span.col
        ));
        out.push_str(&format!("{} |\n", pad));
        let raw = self.source_line(d.span.line);
        let shown = expand_tabs(raw);
        out.push_str(&format!("{:>w$} | {}\n", nstr, shown, w = w + 1));
        // Spaltenversatz unter Beruecksichtigung expandierter Tabulatoren.
        let mut vis = 0usize;
        for (i, c) in raw.chars().enumerate() {
            if i + 1 >= d.span.col as usize {
                break;
            }
            vis += if c == '\t' { TABWIDTH } else { 1 };
        }
        let carets = "^".repeat(d.span.len.max(1) as usize);
        out.push_str(&format!(
            "{} | {}{} {}\n",
            pad,
            " ".repeat(vis),
            carets,
            d.label
        ));
        if let Some(n) = &d.note {
            out.push_str(&format!("{} = hinweis: {}\n", pad, n));
        }
        out
    }

    /// Ausgabe nach stderr.
    pub fn print(&self) {
        if self.has_errors() {
            eprint!("{}", self.render());
        }
    }
}
