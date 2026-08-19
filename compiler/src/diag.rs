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
/// `file` ist die Nummer der Quelldatei in der Quelltextkarte der `Diags`
/// (0 = Wurzeldatei). Programme aus einer einzigen Datei benutzen immer 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: u32,
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

impl Span {
    pub fn new(line: u32, col: u32, len: u32) -> Span {
        Span {
            file: 0,
            line,
            col,
            len: if len == 0 { 1 } else { len },
        }
    }
    /// Position in einer bestimmten Quelldatei (Modulsystem, `modules.rs`).
    pub fn in_file(file: u32, line: u32, col: u32, len: u32) -> Span {
        Span {
            file,
            line,
            col,
            len: if len == 0 { 1 } else { len },
        }
    }
    /// Platzhalterposition fuer Diagnosen ohne echten Ort.
    pub fn none() -> Span {
        Span {
            file: 0,
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

/// Eine Quelldatei in der Quelltextkarte.
struct SourceFileEntry {
    name: String,
    lines: Vec<String>,
}

/// Sammelt Diagnosen fuer eine Uebersetzung. Seit dem Modulsystem kann eine
/// Uebersetzung aus mehreren Dateien bestehen; `Span::file` waehlt die Datei.
pub struct Diags {
    files: Vec<SourceFileEntry>,
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
        let mut d = Diags {
            files: Vec::new(),
            items: Vec::new(),
            max: 40,
        };
        d.add_file(file, src);
        d
    }

    /// Nimmt eine weitere Quelldatei in die Karte auf und liefert ihre Nummer.
    pub fn add_file(&mut self, file: &str, src: &str) -> u32 {
        let id = self.files.len() as u32;
        self.files.push(SourceFileEntry {
            name: file.to_string(),
            lines: src
                .split('\n')
                .map(|l| l.trim_end_matches('\r').to_string())
                .collect(),
        });
        id
    }

    /// Name der Quelldatei mit der Nummer `file`.
    pub fn file_name(&self, file: u32) -> &str {
        self.files
            .get(file as usize)
            .map(|f| f.name.as_str())
            .unwrap_or("<unknown>")
    }

    /// Fehler mit Standardmarkierung ("here").
    pub fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: "here".to_string(),
            note: None,
        });
    }

    /// Fehler mit zusaetzlicher Erklaerungszeile ("hinweis: ...").
    pub fn error_note(&mut self, span: Span, msg: impl Into<String>, note: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: "here".to_string(),
            note: Some(note.into()),
        });
    }

    /// Nimmt eine anderswo gebaute Diagnose auf (z. B. aus der Modulaufloesung).
    pub fn report(&mut self, d: Diag) {
        self.push(d);
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
        self.file_name(0)
    }

    /// Die Quelltextzeile (1-basiert) einer Datei der Karte, ohne Zeilenende.
    pub fn source_line_in(&self, file: u32, line: u32) -> &str {
        if line == 0 {
            return "";
        }
        match self.files.get(file as usize) {
            Some(f) => f.lines.get((line - 1) as usize).map(|s| s.as_str()).unwrap_or(""),
            None => "",
        }
    }

    /// Alle gesammelten Diagnosen als Text.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for d in &self.items {
            out.push_str(&self.render_one(d));
        }
        if self.items.len() > 1 {
            out.push_str(&format!("{} errors found\n", self.items.len()));
        }
        out
    }

    fn render_one(&self, d: &Diag) -> String {
        let mut out = String::new();
        out.push_str(&format!("error: {}\n", d.msg));
        let fname = self.file_name(d.span.file);
        if d.span.is_none() {
            out.push_str(&format!("  --> {}\n", fname));
            if let Some(n) = &d.note {
                out.push_str(&format!("  note: {}\n", n));
            }
            return out;
        }
        let nstr = d.span.line.to_string();
        let w = nstr.len();
        let pad = " ".repeat(w + 1);
        out.push_str(&format!(
            "{}--> {}:{}:{}\n",
            pad, fname, d.span.line, d.span.col
        ));
        out.push_str(&format!("{} |\n", pad));
        let raw = self.source_line_in(d.span.file, d.span.line);
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
            out.push_str(&format!("{} = note: {}\n", pad, n));
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
