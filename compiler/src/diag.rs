//! Diagnostics: collecting and printing error messages with file, line, column,
//! source line and marker.
//!
//! Output format (binding, the negative tests depend on it):
//!
//! ```text
//! error: expected ')' after the argument list
//!   --> tests/neg/bad_call.fi:7:22
//!    |
//!  7 |     let x = add(1, 2 ;
//!    |                      ^ here
//! ```

/// Source position. `line`/`col` are 1-based, `col` and `len` count
/// CHARACTERS (not bytes), so that the marker sits right under UTF-8.
/// `file` is the number of the source file inside the source map of `Diags`
/// (0 = root file). Programs made of a single file always use 0.
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
    /// Position inside a particular source file (module system, `modules.rs`).
    pub fn in_file(file: u32, line: u32, col: u32, len: u32) -> Span {
        Span {
            file,
            line,
            col,
            len: if len == 0 { 1 } else { len },
        }
    }
    /// Placeholder position for diagnostics without a real location.
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

/// One source file of the source map.
struct SourceFileEntry {
    name: String,
    lines: Vec<String>,
}

/// Collects diagnostics for one compilation. Since the module system a
/// compilation may span several files; `Span::file` picks the file.
pub struct Diags {
    files: Vec<SourceFileEntry>,
    items: Vec<Diag>,
    /// Upper bound, so that broken input cannot produce a flood of errors.
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

    /// Takes one more source file into the map and yields its number.
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

    /// Label of the source file carrying the number `file`.
    pub fn file_name(&self, file: u32) -> &str {
        self.files
            .get(file as usize)
            .map(|f| f.name.as_str())
            .unwrap_or("<unknown>")
    }

    /// Error with the standard marker ("here").
    pub fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: "here".to_string(),
            note: None,
        });
    }

    /// Error plus one extra explanation line ("note: ...").
    pub fn error_note(&mut self, span: Span, msg: impl Into<String>, note: impl Into<String>) {
        self.push(Diag {
            msg: msg.into(),
            span,
            label: "here".to_string(),
            note: Some(note.into()),
        });
    }

    /// Takes up a diagnostic built elsewhere (e.g. from module resolution).
    pub fn report(&mut self, d: Diag) {
        self.push(d);
    }

    fn push(&mut self, d: Diag) {
        // Suppress duplicate messages at the same spot (error recovery).
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

    /// The source line (1-based) of a file of the map, without the line ending.
    pub fn source_line_in(&self, file: u32, line: u32) -> &str {
        if line == 0 {
            return "";
        }
        match self.files.get(file as usize) {
            Some(f) => f.lines.get((line - 1) as usize).map(|s| s.as_str()).unwrap_or(""),
            None => "",
        }
    }

    /// All collected diagnostics as text.
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
        // Column offset that accounts for expanded tabs.
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

    /// Print to stderr.
    pub fn print(&self) {
        if self.has_errors() {
            eprint!("{}", self.render());
        }
    }
}
