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

type Fehler = (Span, String);

/// Ergebnis einer ausgeführten Anweisung.
enum Fluss {
    /// weiter mit der nächsten Anweisung
    Weiter,
    /// `return` mit Wert (bzw. 0 bei `return` ohne Wert)
    Zurueck(i128),
    Abbruch,
    Weitermachen,
}

pub(crate) struct Ausfuehrung<'a> {
    prog: &'a Program,
    /// Programmweite Konstanten, wie der Typprüfer sie kennt.
    consts: &'a HashMap<String, (Type, i128)>,
    /// Typ jedes Ausdrucks (für die Breite bei Umwandlungen).
    expr_types: &'a [Type],
    schritte: u64,
    /// Von `emit_*` aufgebauter Quelltext.
    pub(crate) ausgabe: String,
    /// Verzeichnis der Wurzelquelldatei — der EINZIGE Ort, aus dem
    /// `datei_*` lesen darf.
    basis: std::path::PathBuf,
    /// Einmal gelesene Dateien; eine Tabelle wird Byte fuer Byte abgefragt.
    dateien: HashMap<String, Vec<u8>>,
}

impl<'a> Ausfuehrung<'a> {
    pub(crate) fn neu(
        prog: &'a Program,
        consts: &'a HashMap<String, (Type, i128)>,
        expr_types: &'a [Type],
    ) -> Ausfuehrung<'a> {
        Ausfuehrung {
            prog,
            consts,
            expr_types,
            schritte: 0,
            ausgabe: String::new(),
            basis: std::path::PathBuf::from("."),
            dateien: HashMap::new(),
        }
    }

    /// Ruft `name` mit bereits ausgewerteten Argumenten auf.
    pub(crate) fn ruf_auf(
        &mut self,
        name: &str,
        args: &[i128],
        span: Span,
        tiefe: u32,
    ) -> Result<i128, Fehler> {
        if tiefe >= MAX_TIEFE {
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
        let mut umgebung: Vec<HashMap<String, i128>> = vec![HashMap::new()];
        for (p, v) in f.params.iter().zip(args.iter()) {
            umgebung[0].insert(p.name.clone(), *v);
        }
        match self.block(&f.body, &mut umgebung, tiefe)? {
            Fluss::Zurueck(v) => Ok(v),
            // Eine Funktion ohne `return` liefert 0 — der Typprüfer hat
            // vorher sichergestellt, dass das nur bei `-> void` vorkommt.
            _ => Ok(0),
        }
    }

    fn block(
        &mut self,
        b: &Block,
        umg: &mut Vec<HashMap<String, i128>>,
        tiefe: u32,
    ) -> Result<Fluss, Fehler> {
        umg.push(HashMap::new());
        let mut r = Fluss::Weiter;
        for s in &b.stmts {
            r = self.stmt(s, umg, tiefe)?;
            if !matches!(r, Fluss::Weiter) {
                break;
            }
        }
        umg.pop();
        Ok(r)
    }

    fn stmt(
        &mut self,
        s: &Stmt,
        umg: &mut Vec<HashMap<String, i128>>,
        tiefe: u32,
    ) -> Result<Fluss, Fehler> {
        self.schritte += 1;
        if self.schritte > MAX_SCHRITTE {
            return Err((
                s.span(),
                format!(
                    "comptime: mehr als {} schritte — endlosschleife?",
                    MAX_SCHRITTE
                ),
            ));
        }
        match s {
            Stmt::Error(_) => Ok(Fluss::Weiter),
            Stmt::Let { name, init, .. } => {
                let v = self.expr(init, umg, tiefe)?;
                if let Some(top) = umg.last_mut() {
                    top.insert(name.clone(), v);
                }
                Ok(Fluss::Weiter)
            }
            Stmt::Assign { target, value, span } => {
                let v = self.expr(value, umg, tiefe)?;
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
                for ebene in umg.iter_mut().rev() {
                    if let Some(slot) = ebene.get_mut(&name) {
                        *slot = v;
                        return Ok(Fluss::Weiter);
                    }
                }
                Err((*span, format!("comptime: '{}' ist keine lokale variable", name)))
            }
            Stmt::Expr(e) => {
                self.expr(e, umg, tiefe)?;
                Ok(Fluss::Weiter)
            }
            Stmt::Block(b) => self.block(b, umg, tiefe),
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.expr(e, umg, tiefe)?,
                    None => 0,
                };
                Ok(Fluss::Zurueck(v))
            }
            Stmt::If { cond, then, els, .. } => {
                if self.expr(cond, umg, tiefe)? != 0 {
                    self.block(then, umg, tiefe)
                } else {
                    match els {
                        Some(e) => self.stmt(e, umg, tiefe),
                        None => Ok(Fluss::Weiter),
                    }
                }
            }
            Stmt::While { cond, body, .. } => {
                loop {
                    self.schritte += 1;
                    if self.schritte > MAX_SCHRITTE {
                        return Err((
                            s.span(),
                            format!(
                                "comptime: mehr als {} schritte — endlosschleife?",
                                MAX_SCHRITTE
                            ),
                        ));
                    }
                    if self.expr(cond, umg, tiefe)? == 0 {
                        return Ok(Fluss::Weiter);
                    }
                    match self.block(body, umg, tiefe)? {
                        Fluss::Weiter | Fluss::Weitermachen => {}
                        Fluss::Abbruch => return Ok(Fluss::Weiter),
                        Fluss::Zurueck(v) => return Ok(Fluss::Zurueck(v)),
                    }
                }
            }
            Stmt::For { name, start, end, body, .. } => {
                let von = self.expr(start, umg, tiefe)?;
                let bis = self.expr(end, umg, tiefe)?;
                let mut i = von;
                while i < bis {
                    self.schritte += 1;
                    if self.schritte > MAX_SCHRITTE {
                        return Err((
                            s.span(),
                            format!(
                                "comptime: mehr als {} schritte — endlosschleife?",
                                MAX_SCHRITTE
                            ),
                        ));
                    }
                    umg.push(HashMap::new());
                    if let Some(top) = umg.last_mut() {
                        top.insert(name.clone(), i);
                    }
                    let r = self.block(body, umg, tiefe);
                    umg.pop();
                    match r? {
                        Fluss::Weiter | Fluss::Weitermachen => {}
                        Fluss::Abbruch => return Ok(Fluss::Weiter),
                        Fluss::Zurueck(v) => return Ok(Fluss::Zurueck(v)),
                    }
                    i += 1;
                }
                Ok(Fluss::Weiter)
            }
            Stmt::Break(_) => Ok(Fluss::Abbruch),
            Stmt::Continue(_) => Ok(Fluss::Weitermachen),
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
    fn lies_datei(&mut self, pfad: &str, span: Span) -> Result<&Vec<u8>, Fehler> {
        if !self.dateien.contains_key(pfad) {
            if pfad.is_empty() {
                return Err((span, "comptime: leerer dateiname".to_string()));
            }
            let p = std::path::Path::new(pfad);
            if p.is_absolute() || pfad.starts_with('/') || pfad.starts_with('\\') {
                return Err((
                    span,
                    format!("comptime: '{}' ist ein absoluter pfad — erlaubt sind nur pfade relativ zur quelldatei", pfad),
                ));
            }
            if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err((
                    span,
                    format!("comptime: '{}' enthaelt '..' — der zugriff bleibt im verzeichnis der quelldatei", pfad),
                ));
            }
            let voll = self.basis.join(p);
            let inhalt = std::fs::read(&voll).map_err(|e| {
                (
                    span,
                    format!("comptime: '{}' ist nicht lesbar: {}", voll.display(), e),
                )
            })?;
            self.dateien.insert(pfad.to_string(), inhalt);
        }
        Ok(&self.dateien[pfad])
    }

    fn expr(
        &mut self,
        e: &Expr,
        umg: &mut Vec<HashMap<String, i128>>,
        tiefe: u32,
    ) -> Result<i128, Fehler> {
        let nein = |msg: &str| Err((e.span, format!("comptime: {}", msg)));
        match &e.kind {
            ExprKind::Int(v) => Ok(*v),
            ExprKind::Bool(b) => Ok(if *b { 1 } else { 0 }),
            ExprKind::Float(_) => nein("gleitkomma ist zur uebersetzungszeit noch nicht moeglich"),
            ExprKind::Ident(n) => {
                for ebene in umg.iter().rev() {
                    if let Some(v) = ebene.get(n) {
                        return Ok(*v);
                    }
                }
                match self.consts.get(n) {
                    Some((_, v)) => Ok(*v),
                    None => Err((e.span, format!("comptime: '{}' ist hier nicht bekannt", n))),
                }
            }
            ExprKind::Unary(op, inner) => {
                let v = self.expr(inner, umg, tiefe)?;
                match op {
                    UnOp::Neg => Ok(-v),
                    UnOp::Not => Ok(if v == 0 { 1 } else { 0 }),
                    _ => nein("zeigeroperationen gibt es zur uebersetzungszeit nicht"),
                }
            }
            ExprKind::Binary(op, l, r) => {
                let a = self.expr(l, umg, tiefe)?;
                // Kurzschluss beibehalten: `false && f()` ruft `f` nicht.
                if matches!(op, BinOp::LAnd) && a == 0 {
                    return Ok(0);
                }
                if matches!(op, BinOp::LOr) && a != 0 {
                    return Ok(1);
                }
                let b = self.expr(r, umg, tiefe)?;
                rechne(*op, a, b, e.span)
            }
            ExprKind::Cast(inner, _) => {
                let v = self.expr(inner, umg, tiefe)?;
                let ziel = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                Ok(crate::sema::comptime_wrap(v, &ziel))
            }
            ExprKind::Call(name, args, _) => {
                // EMIT: die einzigen Nebenwirkungen, die ein `comptime` haben
                // darf — sie schreiben in den Quelltextpuffer (SPEC §6.4).
                if name == "emit_roh" {
                    let text = literal_text(args, e.span)?;
                    self.ausgabe.push_str(&text);
                    return Ok(0);
                }
                // DATENZUGRIFF ZUR UEBERSETZUNGSZEIT (SPEC §6.4).
                //
                // Genau dafuer verlangt Abnahmepunkt 6 die Unicode-Tabelle
                // „aus der UCD": eine Datendatei wird gelesen und daraus
                // entsteht Quelltext. Die Datei wird byteweise abgefragt —
                // damit braucht der Interpreter weder Zeichenketten noch
                // Arrays.
                if name == "datei_groesse" {
                    let pfad = literal_text(args, e.span)?;
                    let inhalt = self.lies_datei(&pfad, e.span)?;
                    return Ok(inhalt.len() as i128);
                }
                if name == "datei_byte" {
                    if args.len() != 2 {
                        return nein("'datei_byte' erwartet pfad und index");
                    }
                    let pfad = literal_text(&args[..1], e.span)?;
                    let idx = self.expr(&args[1], umg, tiefe)?;
                    let inhalt = self.lies_datei(&pfad, e.span)?;
                    if idx < 0 || idx >= inhalt.len() as i128 {
                        return Ok(-1);
                    }
                    return Ok(inhalt[idx as usize] as i128);
                }
                if name == "emit_zahl" {
                    if args.len() != 1 {
                        return nein("'emit_zahl' erwartet genau ein argument");
                    }
                    let v = self.expr(&args[0], umg, tiefe)?;
                    self.ausgabe.push_str(&v.to_string());
                    return Ok(v);
                }
                let mut werte = Vec::with_capacity(args.len());
                for a in args {
                    werte.push(self.expr(a, umg, tiefe)?);
                }
                self.ruf_auf(name, &werte, e.span, tiefe + 1)
            }
            _ => nein(
                "hier sind nur literale, namen, operatoren, umwandlungen und aufrufe erlaubt",
            ),
        }
    }
}

fn rechne(op: BinOp, a: i128, b: i128, span: Span) -> Result<i128, Fehler> {
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
fn literal_text(args: &[Expr], span: Span) -> Result<String, Fehler> {
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
pub(crate) fn fuehre_bloecke_aus(
    prog: &Program,
    dg: &mut crate::diag::Diags,
    basis: &std::path::Path,
) -> String {
    if prog.comptime_bloecke.is_empty() {
        return String::new();
    }
    let leer_consts: HashMap<String, (Type, i128)> = HashMap::new();
    let leer_typen: Vec<Type> = Vec::new();
    let mut gesamt = String::new();
    for (b, _span) in &prog.comptime_bloecke {
        let mut lauf = Ausfuehrung::neu(prog, &leer_consts, &leer_typen);
        lauf.basis = basis.to_path_buf();
        let mut umg: Vec<HashMap<String, i128>> = vec![HashMap::new()];
        match lauf.block(b, &mut umg, 0) {
            Ok(_) => gesamt.push_str(&lauf.ausgabe),
            Err((span, msg)) => dg.error(span, msg),
        }
    }
    gesamt
}
