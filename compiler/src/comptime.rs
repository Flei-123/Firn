//! **`comptime`** — Auswertung zur Übersetzungszeit.
//!
//! SCHNITTSTELLE (fest):
//!   `pub(crate) fn ruf_auf(...) -> Result<i128, (Span, String)>`
//!
//! ## Wozu
//!
//! `FIRN-ANFORDERUNGEN.md` §6 nennt Kompilierzeit-Codeerzeugung als **Pflicht**
//! für den Browser: 697 Web-IDL-Dateien, die HTML-Entitätentabelle, die
//! CSS-Eigenschaften und die Unicode-Daten sind erzeugter Code. Abnahmepunkt 6
//! verlangt konkret eine Unicode-Tabelle, die zur Übersetzungszeit aus der UCD
//! entsteht.
//!
//! Der erste Schritt dahin ist die Fähigkeit, **eigene Funktionen zur
//! Übersetzungszeit auszuführen** — mit Schleifen, Verzweigungen und lokalen
//! Variablen. Genau das tut dieser Interpreter. Der zweite Schritt (`emit`,
//! also erzeugter Quelltext) baut darauf auf und braucht zusätzlich die
//! wiedereintrittsfähigen Prüfphasen, die seit der Fundamentarbeit stehen
//! (`sema::Checker::add_items`).
//!
//! ## Was ausgewertet werden kann
//!
//! Ganzzahlen und `bool`. Anweisungen: `let`/`var`, Zuweisung an eine lokale
//! Variable, `if`/`else`, `while`, `for`, `break`, `continue`, `return`,
//! Blöcke, Ausdrucksanweisungen. Ausdrücke: Literale, Konstanten, lokale
//! Namen, alle Operatoren, Umwandlungen und **Aufrufe weiterer Funktionen**
//! (auch rekursiv).
//!
//! ## Was bewusst NICHT geht
//!
//! Zeiger, Arrays, Structs, `syscall`, Gleitkomma, GC-Allokation. Alles davon
//! bräuchte einen Speicher zur Übersetzungszeit; der kommt mit `emit`. Ein
//! Versuch endet mit einer Meldung samt Quellposition, nicht mit falschem Code.
//!
//! ## Grenzen, die eingehalten werden
//!
//! Ein `comptime`-Lauf darf den Compiler nicht aufhängen. Deshalb: höchstens
//! `MAX_SCHRITTE` ausgeführte Anweisungen und `MAX_TIEFE` verschachtelte
//! Aufrufe. Beides endet mit einer klaren Meldung.

use crate::ast::{BinOp, Block, Expr, ExprKind, FnDecl, Program, Stmt, UnOp};
use crate::diag::Span;
use crate::types::Type;
use std::collections::HashMap;

/// Obergrenze ausgeführter Anweisungen je `comptime`-Auswertung.
const MAX_SCHRITTE: u64 = 2_000_000;
/// Obergrenze verschachtelter Aufrufe.
const MAX_TIEFE: u32 = 64;

type Error = (Span, String);

/// Ergebnis einer ausgeführten Anweisung.
enum Flow {
    /// weiter mit der nächsten Anweisung
    Next,
    /// `return` mit Wert (bzw. 0 bei `return` ohne Wert)
    Back(i128),
    Abort,
    Resume,
}

pub(crate) struct Execution<'a> {
    prog: &'a Program,
    /// Programmweite Konstanten, wie der Typprüfer sie kennt.
    consts: &'a HashMap<String, (Type, i128)>,
    /// Typ jedes Ausdrucks (für die Breite bei Umwandlungen).
    expr_types: &'a [Type],
    steps: u64,
    /// Von `emit_*` aufgebauter Quelltext.
    pub(crate) output: String,
    /// Verzeichnis der Wurzelquelldatei — der EINZIGE Ort, aus dem
    /// `datei_*` lesen darf.
    base: std::path::PathBuf,
    /// Einmal gelesene Dateien; eine Tabelle wird Byte fuer Byte abgefragt.
    files: HashMap<String, Vec<u8>>,
}

impl<'a> Execution<'a> {
    pub(crate) fn new(
        prog: &'a Program,
        consts: &'a HashMap<String, (Type, i128)>,
        expr_types: &'a [Type],
    ) -> Execution<'a> {
        Execution {
            prog,
            consts,
            expr_types,
            steps: 0,
            output: String::new(),
            base: std::path::PathBuf::from("."),
            files: HashMap::new(),
        }
    }

    /// Ruft `name` mit bereits ausgewerteten Argumenten auf.
    pub(crate) fn call_on(
        &mut self,
        name: &str,
        args: &[i128],
        span: Span,
        depth: u32,
    ) -> Result<i128, Error> {
        if depth >= MAX_TIEFE {
            return Err((
                span,
                format!("comptime: mehr als {} verschachtelte aufrufe", MAX_TIEFE),
            ));
        }
        let f: &FnDecl = match self.prog.funcs.iter().find(|f| f.name == name) {
            Some(f) => f,
            None => {
                return Err((
                    span,
                    format!("comptime: '{}' ist keine funktion dieses programms", name),
                ))
            }
        };
        if f.params.len() != args.len() {
            return Err((
                span,
                format!(
                    "comptime: '{}' erwartet {} argumente, gefunden {}",
                    name,
                    f.params.len(),
                    args.len()
                ),
            ));
        }
        let mut env: Vec<HashMap<String, i128>> = vec![HashMap::new()];
        for (p, v) in f.params.iter().zip(args.iter()) {
            env[0].insert(p.name.clone(), *v);
        }
        match self.block(&f.body, &mut env, depth)? {
            Flow::Back(v) => Ok(v),
            // Eine Funktion ohne `return` liefert 0 — der Typprüfer hat
            // vorher sichergestellt, dass das nur bei `-> void` vorkommt.
            _ => Ok(0),
        }
    }

    fn block(
        &mut self,
        b: &Block,
        env: &mut Vec<HashMap<String, i128>>,
        depth: u32,
    ) -> Result<Flow, Error> {
        env.push(HashMap::new());
        let mut r = Flow::Next;
        for s in &b.stmts {
            r = self.stmt(s, env, depth)?;
            if !matches!(r, Flow::Next) {
                break;
            }
        }
        env.pop();
        Ok(r)
    }

    fn stmt(
        &mut self,
        s: &Stmt,
        env: &mut Vec<HashMap<String, i128>>,
        depth: u32,
    ) -> Result<Flow, Error> {
        self.steps += 1;
        if self.steps > MAX_SCHRITTE {
            return Err((
                s.span(),
                format!(
                    "comptime: mehr als {} schritte — endlosschleife?",
                    MAX_SCHRITTE
                ),
            ));
        }
        match s {
            Stmt::Error(_) => Ok(Flow::Next),
            Stmt::Let { name, init, .. } => {
                let v = self.expr(init, env, depth)?;
                if let Some(top) = env.last_mut() {
                    top.insert(name.clone(), v);
                }
                Ok(Flow::Next)
            }
            Stmt::Assign { target, value, span } => {
                let v = self.expr(value, env, depth)?;
                let name = match &target.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => {
                        return Err((
                            *span,
                            "comptime: nur zuweisungen an eine lokale variable (kein feld, kein index, kein zeiger)"
                                .to_string(),
                        ))
                    }
                };
                for level in env.iter_mut().rev() {
                    if let Some(slot) = level.get_mut(&name) {
                        *slot = v;
                        return Ok(Flow::Next);
                    }
                }
                Err((*span, format!("comptime: '{}' ist keine lokale variable", name)))
            }
            Stmt::Expr(e) => {
                self.expr(e, env, depth)?;
                Ok(Flow::Next)
            }
            Stmt::Block(b) => self.block(b, env, depth),
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.expr(e, env, depth)?,
                    None => 0,
                };
                Ok(Flow::Back(v))
            }
            Stmt::If { cond, then, els, .. } => {
                if self.expr(cond, env, depth)? != 0 {
                    self.block(then, env, depth)
                } else {
                    match els {
                        Some(e) => self.stmt(e, env, depth),
                        None => Ok(Flow::Next),
                    }
                }
            }
            Stmt::While { cond, body, .. } => {
                loop {
                    self.steps += 1;
                    if self.steps > MAX_SCHRITTE {
                        return Err((
                            s.span(),
                            format!(
                                "comptime: mehr als {} schritte — endlosschleife?",
                                MAX_SCHRITTE
                            ),
                        ));
                    }
                    if self.expr(cond, env, depth)? == 0 {
                        return Ok(Flow::Next);
                    }
                    match self.block(body, env, depth)? {
                        Flow::Next | Flow::Resume => {}
                        Flow::Abort => return Ok(Flow::Next),
                        Flow::Back(v) => return Ok(Flow::Back(v)),
                    }
                }
            }
            Stmt::For { name, start, end, body, .. } => {
                let of = self.expr(start, env, depth)?;
                let to = self.expr(end, env, depth)?;
                let mut i = of;
                while i < to {
                    self.steps += 1;
                    if self.steps > MAX_SCHRITTE {
                        return Err((
                            s.span(),
                            format!(
                                "comptime: mehr als {} schritte — endlosschleife?",
                                MAX_SCHRITTE
                            ),
                        ));
                    }
                    env.push(HashMap::new());
                    if let Some(top) = env.last_mut() {
                        top.insert(name.clone(), i);
                    }
                    let r = self.block(body, env, depth);
                    env.pop();
                    match r? {
                        Flow::Next | Flow::Resume => {}
                        Flow::Abort => return Ok(Flow::Next),
                        Flow::Back(v) => return Ok(Flow::Back(v)),
                    }
                    i += 1;
                }
                Ok(Flow::Next)
            }
            Stmt::Break(_) => Ok(Flow::Abort),
            Stmt::Continue(_) => Ok(Flow::Resume),
            // Aufgeschobene Anweisungen haetten in einer reinen Rechnung keine
            // Wirkung; sie werden abgelehnt statt still uebergangen.
            Stmt::Defer(_, _, span) => Err((
                *span,
                "comptime: 'defer' und 'errdefer' sind zur uebersetzungszeit nicht erlaubt"
                    .to_string(),
            )),
        }
    }

    /// Liest eine Datendatei — EINMAL, danach aus dem Zwischenspeicher.
    ///
    /// SICHERHEIT (DESIGNZIELE §3): Uebersetzungszeit-Dateizugriff ist ein
    /// Einfallstor fuer Lieferketten-Angriffe — eine eingebundene Bibliothek
    /// koennte sonst beim Bauen `/etc/passwd` lesen und in den erzeugten Code
    /// schreiben. Deshalb gilt hier eine harte Regel:
    ///
    ///   * nur RELATIV zur Wurzelquelldatei,
    ///   * kein `..` an irgendeiner Stelle,
    ///   * kein absoluter Pfad, kein Laufwerks- oder Wurzelpraefix.
    ///
    /// Das ist bewusst enger als noetig. Wenn Firn ein Modulsystem mit
    /// Faehigkeiten bekommt (DESIGNZIELE §3), wird daraus eine Erlaubnis, die
    /// ein Modul ausdruecklich anfordern muss.
    fn read_file(&mut self, path: &str, span: Span) -> Result<&Vec<u8>, Error> {
        if !self.files.contains_key(path) {
            if path.is_empty() {
                return Err((span, "comptime: leerer dateiname".to_string()));
            }
            let p = std::path::Path::new(path);
            if p.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
                return Err((
                    span,
                    format!("comptime: '{}' ist ein absoluter pfad — erlaubt sind nur pfade relativ zur quelldatei", path),
                ));
            }
            if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err((
                    span,
                    format!("comptime: '{}' enthaelt '..' — der zugriff bleibt im verzeichnis der quelldatei", path),
                ));
            }
            let full = self.base.join(p);
            let content = std::fs::read(&full).map_err(|e| {
                (
                    span,
                    format!("comptime: '{}' ist nicht lesbar: {}", full.display(), e),
                )
            })?;
            self.files.insert(path.to_string(), content);
        }
        Ok(&self.files[path])
    }

    fn expr(
        &mut self,
        e: &Expr,
        env: &mut Vec<HashMap<String, i128>>,
        depth: u32,
    ) -> Result<i128, Error> {
        let no = |msg: &str| Err((e.span, format!("comptime: {}", msg)));
        match &e.kind {
            ExprKind::Int(v) => Ok(*v),
            ExprKind::Bool(b) => Ok(if *b { 1 } else { 0 }),
            ExprKind::Float(_) => no("gleitkomma ist zur uebersetzungszeit noch nicht moeglich"),
            ExprKind::Ident(n) => {
                for level in env.iter().rev() {
                    if let Some(v) = level.get(n) {
                        return Ok(*v);
                    }
                }
                match self.consts.get(n) {
                    Some((_, v)) => Ok(*v),
                    None => Err((e.span, format!("comptime: '{}' ist hier nicht bekannt", n))),
                }
            }
            ExprKind::Unary(op, inner) => {
                let v = self.expr(inner, env, depth)?;
                match op {
                    UnOp::Neg => Ok(-v),
                    UnOp::Not => Ok(if v == 0 { 1 } else { 0 }),
                    _ => no("zeigeroperationen gibt es zur uebersetzungszeit nicht"),
                }
            }
            ExprKind::Binary(op, l, r) => {
                let a = self.expr(l, env, depth)?;
                // Kurzschluss beibehalten: `false && f()` ruft `f` nicht.
                if matches!(op, BinOp::LAnd) && a == 0 {
                    return Ok(0);
                }
                if matches!(op, BinOp::LOr) && a != 0 {
                    return Ok(1);
                }
                let b = self.expr(r, env, depth)?;
                compute(*op, a, b, e.span)
            }
            ExprKind::Cast(inner, _) => {
                let v = self.expr(inner, env, depth)?;
                let target = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                Ok(crate::sema::comptime_wrap(v, &target))
            }
            ExprKind::Call(name, args, _) => {
                // EMIT: die einzigen Nebenwirkungen, die ein `comptime` haben
                // darf — sie schreiben in den Quelltextpuffer (SPEC §6.4).
                if name == "emit_raw" {
                    let text = literal_text(args, e.span)?;
                    self.output.push_str(&text);
                    return Ok(0);
                }
                // DATENZUGRIFF ZUR UEBERSETZUNGSZEIT (SPEC §6.4).
                //
                // Genau dafuer verlangt Abnahmepunkt 6 die Unicode-Tabelle
                // „aus der UCD": eine Datendatei wird gelesen und daraus
                // entsteht Quelltext. Die Datei wird byteweise abgefragt —
                // damit braucht der Interpreter weder Zeichenketten noch
                // Arrays.
                if name == "file_size" {
                    let path = literal_text(args, e.span)?;
                    let content = self.read_file(&path, e.span)?;
                    return Ok(content.len() as i128);
                }
                if name == "file_byte" {
                    if args.len() != 2 {
                        return no("'datei_byte' erwartet pfad und index");
                    }
                    let path = literal_text(&args[..1], e.span)?;
                    let idx = self.expr(&args[1], env, depth)?;
                    let content = self.read_file(&path, e.span)?;
                    if idx < 0 || idx >= content.len() as i128 {
                        return Ok(-1);
                    }
                    return Ok(content[idx as usize] as i128);
                }
                if name == "emit_number" {
                    if args.len() != 1 {
                        return no("'emit_zahl' erwartet genau ein argument");
                    }
                    let v = self.expr(&args[0], env, depth)?;
                    self.output.push_str(&v.to_string());
                    return Ok(v);
                }
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.expr(a, env, depth)?);
                }
                self.call_on(name, &values, e.span, depth + 1)
            }
            _ => no(
                "hier sind nur literale, namen, operatoren, umwandlungen und aufrufe erlaubt",
            ),
        }
    }
}

fn compute(op: BinOp, a: i128, b: i128, span: Span) -> Result<i128, Error> {
    let bit = |x: bool| if x { 1 } else { 0 };
    Ok(match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0 {
                return Err((span, "comptime: division durch null".to_string()));
            }
            a / b
        }
        BinOp::Rem => {
            if b == 0 {
                return Err((span, "comptime: rest bei division durch null".to_string()));
            }
            a % b
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => {
            if !(0..128).contains(&b) {
                return Err((span, "comptime: verschiebeweite ausserhalb 0..127".to_string()));
            }
            a << b
        }
        BinOp::Shr => {
            if !(0..128).contains(&b) {
                return Err((span, "comptime: verschiebeweite ausserhalb 0..127".to_string()));
            }
            a >> b
        }
        BinOp::Eq => bit(a == b),
        BinOp::Ne => bit(a != b),
        BinOp::Lt => bit(a < b),
        BinOp::Le => bit(a <= b),
        BinOp::Gt => bit(a > b),
        BinOp::Ge => bit(a >= b),
        BinOp::LAnd => bit(a != 0 && b != 0),
        BinOp::LOr => bit(a != 0 || b != 0),
    })
}

/// Der Text eines Zeichenkettenliterals. Der Parser hat `"abc"` bereits in ein
/// Array-Literal aus Oktetten verwandelt (SPEC §14.1.str) — hier wird es
/// zurueckgelesen. Damit braucht `emit_roh` keine Zeichenkettenunterstuetzung
/// im Interpreter.
fn literal_text(args: &[Expr], span: Span) -> Result<String, Error> {
    if args.len() != 1 {
        return Err((span, "comptime: 'emit_roh' erwartet genau ein argument".to_string()));
    }
    let elems = match &args[0].kind {
        ExprKind::ArrayLit(v) => v,
        _ => {
            return Err((
                args[0].span,
                "comptime: 'emit_roh' erwartet ein zeichenkettenliteral".to_string(),
            ))
        }
    };
    let mut bytes = Vec::with_capacity(elems.len());
    for el in elems {
        match &el.kind {
            ExprKind::Int(v) if (0..256).contains(v) => bytes.push(*v as u8),
            _ => {
                return Err((
                    args[0].span,
                    "comptime: 'emit_roh' erwartet ein zeichenkettenliteral".to_string(),
                ))
            }
        }
    }
    String::from_utf8(bytes)
        .map_err(|_| (args[0].span, "comptime: 'emit_roh' braucht gueltiges UTF-8".to_string()))
}

/// Fuehrt alle `comptime { … }`-Bloecke des Programms aus und liefert den dabei
/// erzeugten Quelltext.
///
/// Der Lauf findet VOR der Typpruefung statt: die Bloecke duerfen deshalb keine
/// programmweiten Konstanten benutzen, wohl aber jede Funktion des Programms
/// aufrufen. Ehrlich benannt in SPEC §14.1.comptime.
pub(crate) fn run_blocks_out(
    prog: &Program,
    dg: &mut crate::diag::Diags,
    base: &std::path::Path,
) -> String {
    if prog.comptime_blocks.is_empty() {
        return String::new();
    }
    let empty_consts: HashMap<String, (Type, i128)> = HashMap::new();
    let empty_types: Vec<Type> = Vec::new();
    let mut total = String::new();
    for (b, _span) in &prog.comptime_blocks {
        let mut run = Execution::new(prog, &empty_consts, &empty_types);
        run.base = base.to_path_buf();
        let mut env: Vec<HashMap<String, i128>> = vec![HashMap::new()];
        match run.block(b, &mut env, 0) {
            Ok(_) => total.push_str(&run.output),
            Err((span, msg)) => dg.error(span, msg),
        }
    }
    total
}
