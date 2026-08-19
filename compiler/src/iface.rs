//! `interface` und **dynamischer Versand** — Runde 46 (SPEC §6.2).
//!
//! ## Was diese Runde hinzufuegt
//!
//! Runde 45 brachte `impl Typ { fn m(*self) … }` — eine reine Schreibhilfe:
//! `x.m(a)` wurde zu `Typ__m(&x, a)`, entschieden allein vom **statischen**
//! Typ. Diese Runde ergaenzt das um den anderen Fall, den SPEC §6.2
//! ausdruecklich verlangt: **eine Aufrufstelle, viele Typen**.
//!
//! ```text
//! interface Flaeche {              // Vertrag: Methoden ohne Rumpf
//!     fn flaeche(*self) -> i64
//!     fn skaliere(*mut self, k: i64)
//! }
//!
//! impl Flaeche for Rechteck { … }  // Umsetzung, vollstaendig geprueft
//!
//! let f: dyn Flaeche = (&r) as dyn Flaeche   // Schnittstellenwert
//! f.flaeche()                                // ueber die Methodentafel
//! ```
//!
//! ## Die Darstellung — verbindlich
//!
//! ```text
//! interface I      -> Struct "dyn I" in types::TypeCtx, 16 Byte:
//!                       daten: *mut u8   (Versatz 0) — der Wert selbst
//!                       tafel: *mut u8   (Versatz 8) — die Methodentafel
//! dyn I            -> genau dieser Struct (ein FETTER ZEIGER, kein Zeiger)
//! impl I for T     -> Methodentafel `.L__iface.I.T` in `.rodata`:
//!                       .quad T__m1
//!                       .quad T__m2      (Reihenfolge = Reihenfolge in `I`)
//! ```
//!
//! Der interne Structname traegt ein **Leerzeichen** (`"dyn I"`) — dieselbe
//! Bauart wie `"gc C"` in `gc.rs` und `"methode m"` in `impls.rs`. Er kann aus
//! keinem Bezeichner des Quelltextes entstehen, und `TypeCtx::name_of` gibt
//! ihn unveraendert aus: in jeder Fehlermeldung steht `dyn I`, so wie es im
//! Quelltext geschrieben wird.
//!
//! ## Warum ein Struct und kein eigener `Type`
//!
//! Ein `Type::Dyn(..)` haette jede Fallunterscheidung ueber `Type` angefasst —
//! Layout, ABI, Monomorphisierung, Optimierer, Debuginfo. Als Struct ist der
//! Schnittstellenwert ein **gewoehnliches 16-Byte-Aggregat**: `abi::classify`
//! gibt ihm zwei INTEGER-Woerter, er wird kopiert, uebergeben, zurueckgegeben
//! und im Rahmen abgelegt wie jeder andere Struct. Die ganze Sprachaenderung
//! sitzt in Parser, Typpruefer und zwei FIR-Instruktionen.
//!
//! ## Warum der Sammler den Wert dahinter weiter findet
//!
//! Der Datenzeiger ist ein **gewoehnlicher Zeiger auf den Anfang** des Wertes
//! — nicht verschleiert (anders als `GcWeak`, `gc.rs`), nicht auf ein Feld
//! versetzt. Ein `dyn I` liegt im Rahmen oder in einem callee-saved Register;
//! beides durchsucht der Sammler konservativ (SPEC §3.5.3), und beim
//! Sammellauf rettet `Op::GcAddr { regs: true }` die Register vorher in den
//! Zustandsblock. Damit haelt ein Schnittstellenwert sein Objekt am Leben,
//! auch wenn es sonst keine Wurzel mehr gibt (`tests/823_iface_gc_core.fi`).
//!
//! Die **eine** Stelle, an der das nicht traegt, ist der Heap: dort verfolgt
//! der Sammler PRAEZISE anhand des Feldlayouts und wuerde den Datenzeiger in
//! einem `dyn I`-Feld nicht kennen. Deshalb ist `dyn I` als Feld einer
//! `gc class` ein Fehler (`iface_dyn_in_gc_class.fi`) — ein Loch in einer
//! Zusage waere schlimmer als eine fehlende Bequemlichkeit.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::ast::{Expr, TypeExpr};
use crate::diag::{Diags, Span};
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::{Type, TypeCtx};

/// Praefix des internen Structnamens eines Schnittstellenwertes.
/// Das Leerzeichen macht ihn unerreichbar fuer den Quelltext.
pub(crate) const P_DYN: &str = "dyn ";
/// Praefix der Methodentafeln im Assembler (dateilokal, `.L`).
const TAFEL_LABEL: &str = ".L__iface.";
/// Versatz des Datenzeigers im Schnittstellenwert.
pub(crate) const OFF_DATEN: u64 = 0;
/// Versatz der Methodentafel im Schnittstellenwert.
pub(crate) const OFF_TAFEL: u64 = 8;

// ---------------------------------------------------------------- Datenmodell

#[derive(Clone, Debug)]
struct Method {
    name: String,
    /// Parameter OHNE den Empfaenger.
    params: Vec<TypeExpr>,
    ret: Option<TypeExpr>,
    /// `*mut self` statt `*self` — heute Absicht, kein Zwang (impls.rs).
    mutable: bool,
    span: Span,
    /// nach `hook_check_impls`: aufgeloeste Typen
    ptypes: Vec<Type>,
    rtyp: Type,
    /// Typen bereits aufgeloest (Nachtraege aus `comptime` melden sonst doppelt)
    resolved: bool,
    /// Die Signatur nennt `Self` (Runde 50). Dann haengen `ptypen`/`rtyp` am
    /// umsetzenden Typ und werden ERST JE UMSETZUNG aufgeloest; global
    /// bleiben sie leer, und ueber `dyn I` ist die Methode nicht aufrufbar.
    has_self: bool,
}

#[derive(Clone, Debug)]
struct Interface {
    name: String,
    methods: Vec<Method>,
    /// Index des Structs `"dyn I"` in `TypeCtx`, `usize::MAX` bis zur Anmeldung
    struct_idx: usize,
}

#[derive(Clone, Debug)]
struct Impl {
    iface: String,
    /// Typname, wie er im Quelltext steht (VOR der Modulumbenennung).
    ty: String,
    span: Span,
    /// nach der Pruefung: Index des Structs, `usize::MAX` = nicht aufgeloest
    struct_idx: usize,
    /// nach der Pruefung: Name, unter dem die Methoden dieses Typs stehen
    /// (`T__m`) — also der ENDGUELTIGE Name nach der Modulumbenennung, ohne
    /// das interne `"gc "` einer Klasse. Der Codegenerator hat die Typtabelle
    /// nicht mehr; deshalb steht er hier.
    prefix: String,
    /// nach der Pruefung: `true`, wenn die Umsetzung vollstaendig ist
    ok: bool,
    /// schon geprueft (Nachtraege aus `comptime` melden sonst doppelt)
    checked: bool,
}

#[derive(Default)]
struct Registry {
    ifaces: Vec<Interface>,
    impls: Vec<Impl>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Setzt die Registrierung zurueck (eine je Uebersetzung, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

fn iface_index(name: &str) -> Option<usize> {
    REG.with(|r| r.borrow().ifaces.iter().position(|i| i.name == name))
}

/// Gibt es ueberhaupt eine Schnittstelle in dieser Uebersetzung?
pub(crate) fn has_interfaces() -> bool {
    REG.with(|r| !r.borrow().ifaces.is_empty())
}

/// Ist `sname` der interne Name eines Schnittstellenwertes? Liefert den
/// Schnittstellennamen.
pub(crate) fn interface_of(sname: &str) -> Option<&str> {
    sname.strip_prefix(P_DYN)
}

/// Name des Structs hinter `dyn I`.
fn dyn_name(iface: &str) -> String {
    format!("{}{}", P_DYN, iface)
}

/// Ist dieser Typ ein Schnittstellenwert?
pub(crate) fn is_dyn(tcx: &TypeCtx, t: &Type) -> bool {
    match t {
        Type::Struct(i) => tcx
            .structs
            .get(*i)
            .map(|s| s.name.starts_with(P_DYN))
            .unwrap_or(false),
        _ => false,
    }
}

/// Schluessel der Methodentafel: `<Schnittstelle>.<Typ>`.
fn table_key(iface: &str, ty_name: &str) -> String {
    format!("{}.{}", iface, ty_name)
}

/// Der eingebaute Typ hinter einem Namen — `None`, wenn es keiner ist.
///
/// Seit Runde 50 darf auch ein GRUNDTYP eine Schnittstelle umsetzen
/// (`impl Ord for i32`). Ohne das haette `vec_sortiere[T: Ord]` nur Structs
/// sortieren koennen, und die Standardbibliothek haette den fest verdrahteten
/// Vergleich behalten muessen.
pub(crate) fn base_ty_of_name(n: &str) -> Option<Type> {
    Some(match n {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        "isize" => Type::Isize,
        "bool" => Type::Bool,
        "f64" => Type::F64,
        _ => return None,
    })
}

/// Name eines Grundtyps fuer das Methodennamensschema (`i32__kleiner`).
pub(crate) fn base_ty_name(t: &Type) -> Option<&'static str> {
    Some(match t {
        Type::I8 => "i8",
        Type::I16 => "i16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::U32 => "u32",
        Type::U64 => "u64",
        Type::Usize => "usize",
        Type::Isize => "isize",
        Type::Bool => "bool",
        Type::F64 => "f64",
        _ => return None,
    })
}

/// Ist das der Name einer Methode auf einem GRUNDTYP (`i32__kleiner`)?
///
/// Solche Methoden gelten PROGRAMMWEIT und werden von `modules.rs` nicht
/// umbenannt — genauso wie Schnittstellen, gc-Klassen und generische
/// Vorlagen. Der Grund ist derselbe: der Typ `i32` gehoert keinem Modul.
/// Wuerde die Methode zu `vec__i32__kleiner`, suchte die Aufloesung weiter
/// `i32__kleiner` und faende nichts.
pub(crate) fn is_base_ty_method(name: &str) -> bool {
    match name.split_once(crate::impls::TRENNER) {
        Some((header, rest)) => !rest.is_empty() && base_ty_of_name(header).is_some(),
        None => false,
    }
}

/// Nennt dieser Typausdruck `Self`?
fn names_self(te: &TypeExpr) -> bool {
    match te {
        TypeExpr::Named(n, _) => n == "Self",
        TypeExpr::Ptr { inner, .. } => names_self(inner),
        TypeExpr::Array { elem, .. } => names_self(elem),
    }
}

// ------------------------------------------------- Schranken (Runde 50)
//
// `fn f[T: Ord](…)` — die Schranke wird bei der AUSPRAEGUNG geprueft
// (`mono.rs::bind_params`), also bevor der Typpruefer laeuft. Zu dem
// Zeitpunkt gibt es weder Structtabelle noch aufgeloeste Typen; was es gibt,
// sind die Namen: die Registrierung dieser Datei und die Liste aller
// Funktionsnamen des zusammengefuehrten Programms. Genau daraus wird die
// Meldung gebaut, und genau deshalb kann sie die FEHLENDE METHODE nennen,
// statt spaeter als „unbekannte methode" mitten in einer ausgepraegten
// Funktion aufzuschlagen.

/// Lesbare Form eines noch nicht aufgeloesten Typs (fuer Meldungen vor dem
/// Typpruefer). `Self` bleibt `Self` — genau so steht es in der Schnittstelle.
fn te_text(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            format!("*{}{}", if *mutable { "mut " } else { "" }, te_text(inner))
        }
        TypeExpr::Array { elem, len, .. } => format!("[{}; {}]", te_text(elem), len),
    }
}

/// Signatur einer Schnittstellenmethode aus dem UNAUFGELOESTEN Kopf.
/// (`signatur` weiter unten macht dasselbe mit aufgeloesten Typen; hier ist
/// noch kein Typpruefer gelaufen.)
fn header_signature(m: &Method) -> String {
    let mut s = String::from(if m.mutable { "*mut self" } else { "*self" });
    for t in &m.params {
        s.push_str(", ");
        s.push_str(&te_text(t));
    }
    match &m.ret {
        None => format!("fn {}({})", m.name, s),
        Some(r) => format!("fn {}({}) -> {}", m.name, s, te_text(r)),
    }
}

/// Setzt der Typ `typname` die Schnittstelle `iface` um?
///
/// Verglichen werden NAMEN, nicht Typen — den Typpruefer gibt es hier noch
/// nicht. Der Name in der Registrierung steht so da, wie er im Quelltext
/// geschrieben wurde; das Typargument traegt dagegen schon den Namen nach der
/// Modulumbenennung. Deshalb dieselben drei Schritte wie in `typ_struct`:
/// gleich, `gc <Name>`, oder auf `__<Name>` endend (Typ aus einem Modul).
fn impl_da(iface: &str, ty_name: &str) -> bool {
    REG.with(|r| {
        r.borrow().impls.iter().any(|u| {
            if u.iface != iface {
                return false;
            }
            if u.ty == ty_name || ty_name == format!("gc {}", u.ty) {
                return true;
            }
            // Die Endungsregel gilt NUR fuer benannte Typen aus einem Modul.
            // Fuer einen Grundtyp waere sie falsch: `Vec__i32` endet auf
            // `__i32`, ist aber der Struct `Vec[i32]` und nicht `i32`.
            base_ty_of_name(&u.ty).is_none()
                && ty_name.ends_with(&format!("__{}", u.ty))
        })
    })
}

/// `// HOOK iface` in `mono.rs::schranke_ok` — `T: I` bei der Auspraegung.
/// `true` = die Schranke ist erfuellt.
pub(crate) fn bound_check(
    dg: &mut Diags,
    fnames: &HashSet<String>,
    arg: &TypeExpr,
    iface: &str,
    pname: &str,
    base: &str,
    span: Span,
) -> bool {
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => {
            let known: Vec<String> =
                REG.with(|r| r.borrow().ifaces.iter().map(|s| s.name.clone()).collect());
            let note = if known.is_empty() {
                "no interface is declared in this compilation; \
                 built in are only Any, Int and Scalar"
                    .to_string()
            } else {
                format!("known are: {} (built in: Any, Int, Scalar)", known.join(", "))
            };
            dg.error_note(
                span,
                format!(
                    "unknown interface '{}' as bound on the type parameter '{}' of '{}'",
                    iface, pname, base
                ),
                note,
            );
            return false;
        }
    };
    // Eine Schnittstelle wird von einem BENANNTEN Typ umgesetzt. Ein Zeiger
    // oder ein Feld hat keinen Namen, unter dem eine Umsetzung stehen koennte.
    let ty_name = match arg {
        TypeExpr::Named(n, _) => n.clone(),
        _ => {
            dg.error_note(
                span,
                format!(
                    "type argument '{}' does not satisfy the bound '{}' of the type parameter '{}' of '{}'",
                    te_text(arg), iface, pname, base
                ),
                format!(
                    "an interface is implemented with 'impl {} for <type>'; \
                     a pointer or field type has no name under which that could stand",
                    iface
                ),
            );
            return false;
        }
    };
    if impl_da(iface, &ty_name) {
        return true;
    }
    // Kein `impl I for T`. Jetzt die nuetzliche Meldung: welche Methoden der
    // Schnittstelle hat der Typ ueberhaupt schon?
    let methods = REG.with(|r| r.borrow().ifaces[ii].methods.clone());
    let mut missing: Vec<String> = Vec::new();
    for m in &methods {
        let full = format!("{}{}{}", ty_name, crate::impls::TRENNER, m.name);
        if !fnames.contains(&full) {
            missing.push(header_signature(m));
        }
    }
    let note = if methods.is_empty() {
        format!("'{}' has no method; only 'impl {} for {}' is missing", iface, iface, ty_name)
    } else if missing.is_empty() {
        format!(
            "'{}' has all methods of '{}'; the block 'impl {} for {} {{ … }}' is missing",
            ty_name, iface, iface, ty_name
        )
    } else {
        format!(
            "{} is missing in 'impl {} for {} {{ … }}'",
            missing
                .iter()
                .map(|x| format!("'{}'", x))
                .collect::<Vec<_>>()
                .join(" and "),
            iface,
            ty_name
        )
    };
    dg.error_note(
        span,
        format!(
            "type '{}' does not implement the interface '{}' — bound on the type parameter '{}' of '{}'",
            ty_name, iface, pname, base
        ),
        note,
    );
    false
}

// ------------------------------------------------------------------- Parser

/// `// HOOK iface` in `parser.rs::program` — `interface Name { … }`.
///
/// `interface` ist KEIN Schluesselwort (der Tokenisierer kennt es nicht),
/// sondern ein Bezeichner in einer Stellung, in der sonst nichts stehen darf —
/// dieselbe Loesung wie `gc class` (gc.rs) und `impl` (impls.rs). Damit bleibt
/// `interface` als Variablenname gueltig.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if !matches!(p.kind(), TokKind::Ident(n) if n == "interface") {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(_))) {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBrace)) {
        return false;
    }
    interface_decl(p);
    true
}

fn interface_decl(p: &mut Parser) {
    p.bump(); // 'interface'
    if !p.pending_attrs.is_empty() {
        let sp = p.pending_attrs[0].span;
        p.dg.error_note(
            sp,
            "no attribute is allowed before 'interface'".to_string(),
            "attributes on interfaces do not exist in this stage".to_string(),
        );
        p.pending_attrs.clear();
    }
    let (name, nsp) = match p.ident("after 'interface'") {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return;
        }
    };
    if !p.expect(TokKind::LBrace, "after the name of the interface") {
        p.recovering = false;
        p.sync_item();
        return;
    }
    let mut methods: Vec<Method> = Vec::new();
    loop {
        while p.eat(&TokKind::Semi) {}
        if p.at(&TokKind::RBrace) || p.at_eof() {
            break;
        }
        if p.dg.is_full() {
            break;
        }
        let before = p.pos;
        if !p.at(&TokKind::KwFn) {
            p.error_here(format!(
                "expected 'fn' in an interface, found '{}'",
                p.kind().text()
            ));
            p.recovering = false;
            p.sync_item();
            return;
        }
        // Eine kaputte Methode bricht die GANZE Schnittstelle ab — dieselbe
        // Regel wie im `impl`-Block (impls.rs): die erste Meldung ist die
        // einzige, die etwas erklaert.
        match method_head(p, &name) {
            Some(m) => {
                if methods.iter().any(|x| x.name == m.name) {
                    p.dg.error(
                        m.span,
                        format!(
                            "the interface '{}' already has the method '{}'",
                            name, m.name
                        ),
                    );
                }
                methods.push(m);
            }
            None => {
                p.recovering = false;
                p.sync_item();
                return;
            }
        }
        if p.pos == before {
            p.bump();
        }
    }
    p.close(TokKind::RBrace, "at the end of the interface");
    p.recovering = false;
    if iface_index(&name).is_some() {
        p.dg.error(
            nsp,
            format!("the interface '{}' is already declared", name),
        );
        return;
    }
    REG.with(|r| {
        r.borrow_mut().ifaces.push(Interface {
            name,
            methods,
            struct_idx: usize::MAX,
        })
    });
}

/// Eine Methode der Schnittstelle: `fn name(<empfaenger>[, param…]) [-> T]`
/// — ohne Rumpf.
fn method_head(p: &mut Parser, iface: &str) -> Option<Method> {
    p.bump(); // 'fn'
    let (name, nsp) = match p.ident("after 'fn' in an interface") {
        Some(x) => x,
        None => return None,
    };
    if p.at(&TokKind::LBracket) {
        p.error_here("an interface method cannot be generic in this stage");
        return None;
    }
    if !p.expect(TokKind::LParen, "after the method name") {
        return None;
    }
    // DER EMPFAENGER MUSS EIN ZEIGER SEIN. Ueber die Methodentafel steht nur
    // der Datenzeiger zur Verfuegung — eine Kopie des Wertes koennte der
    // Aufrufer gar nicht bilden, er kennt den konkreten Typ nicht.
    let mutable = if p.at(&TokKind::Star) {
        match crate::impls::ptr_self(p) {
            Some((m, _)) => m,
            None => {
                p.error_here(format!(
                    "the receiver of an interface method is '*self' or '*mut self' ('{}.{}')",
                    iface, name
                ));
                return None;
            }
        }
    } else {
        p.error_here(format!(
            "the receiver of an interface method is '*self' or '*mut self' ('{}.{}')",
            iface, name
        ));
        return None;
    };
    let mut params: Vec<TypeExpr> = Vec::new();
    if p.eat(&TokKind::Comma) {
        params = p.params().into_iter().map(|x| x.ty).collect();
    }
    p.close(TokKind::RParen, "after the parameter list");
    p.recovering = false;
    let ret = if p.eat(&TokKind::Arrow) {
        match p.parse_type() {
            Some(t) => Some(t),
            None => return None,
        }
    } else {
        None
    };
    if p.at(&TokKind::LBrace) {
        p.error_here(format!(
            "an interface method has no body ('{}.{}')",
            iface, name
        ));
        return None;
    }
    // `Self` (Runde 50): der Typ, der die Schnittstelle umsetzt. Erst damit
    // laesst sich eine Ordnung aufschreiben — `fn kleiner(*self, b: *Self)`.
    // Ohne `Self` muesste in der Schnittstelle ein KONKRETER Typ stehen, und
    // eine allgemeine `Ord` waere unmoeglich.
    let has_self =
        params.iter().any(names_self) || ret.as_ref().map(names_self).unwrap_or(false);
    Some(Method {
        name,
        params,
        ret,
        mutable,
        span: nsp,
        ptypes: Vec::new(),
        rtyp: Type::Void,
        resolved: false,
        has_self,
    })
}

/// `// HOOK iface` in `parser.rs::parse_type_inner` — `dyn I`.
///
/// Der Typname wird zu EINEM Namen mit Leerzeichen zusammengezogen; aufgeloest
/// wird er im Typpruefer, der die Structtabelle kennt.
pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    if name != "dyn" {
        return None;
    }
    let n = match p.kind() {
        TokKind::Ident(n) => n.clone(),
        _ => return None,
    };
    let end = p.bump();
    Some(TypeExpr::Named(dyn_name(&n), Parser::join(sp, end)))
}

/// `// HOOK iface` in `impls.rs::impl_decl` — `impl I for T { … }` anmelden.
pub(crate) fn remember_impl(iface: String, ty: String, span: Span) {
    REG.with(|r| {
        r.borrow_mut().impls.push(Impl {
            iface,
            ty,
            span,
            struct_idx: usize::MAX,
            prefix: String::new(),
            ok: false,
            checked: false,
        })
    });
}

// -------------------------------------------------------------- Typpruefung

/// `// HOOK iface` in `sema::add_items_inner` (VOR `collect_structs`): fuer
/// jede Schnittstelle den Struct `"dyn I"` anlegen.
///
/// Zweimal aufgerufen zu werden ist erlaubt (`comptime`-Nachtraege): eine
/// bereits angemeldete Schnittstelle wird uebersprungen.
pub(crate) fn declare_interfaces(ck: &mut Checker) {
    let n = REG.with(|r| r.borrow().ifaces.len());
    for i in 0..n {
        let (name, done) = REG.with(|r| {
            let reg = r.borrow();
            let s = &reg.ifaces[i];
            (s.name.clone(), s.struct_idx != usize::MAX)
        });
        if done {
            continue;
        }
        let sidx = ck.tcx.declare(&dyn_name(&name));
        ck.tcx.set_fields(
            sidx,
            vec![
                ("data".to_string(), Type::ptr(Type::U8, true)),
                ("table".to_string(), Type::ptr(Type::U8, true)),
            ],
        );
        REG.with(|r| r.borrow_mut().ifaces[i].struct_idx = sidx);
    }
}

/// `// HOOK iface` in `sema::resolve_ty_d` — `dyn I` mit unbekanntem `I`.
pub(crate) fn hook_resolve_ty(ck: &mut Checker, te: &TypeExpr) -> Option<Type> {
    let (name, span) = match te {
        TypeExpr::Named(n, s) => (n.as_str(), *s),
        _ => return None,
    };
    let iface = interface_of(name)?;
    if ck.tcx.lookup(name).is_some() {
        return None; // die gewoehnliche Aufloesung findet den Struct
    }
    ck.dg.error_note(
        span,
        format!("unknown interface '{}'", iface),
        format!("an interface is declared with 'interface {} {{ … }}'", iface),
    );
    Some(Type::Error)
}

/// Der Name, unter dem die Methoden eines Typs stehen: `T__m`.
/// Fuer eine `gc class` ist das der Klassenname OHNE das interne `"gc "`.
pub(crate) fn method_prefix(tcx: &TypeCtx, idx: usize) -> String {
    match tcx.structs.get(idx) {
        Some(s) => s.name.strip_prefix("gc ").unwrap_or(&s.name).to_string(),
        None => String::new(),
    }
}

/// Vergleich zweier Typen an einer Signaturgrenze — dieselbe Regel wie
/// `sema::compatible`: Zeiger werden OHNE die Veraenderlichkeit verglichen
/// (`*T` und `*mut T` sind in dieser Sprache fuereinander einsetzbar).
fn fits(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => fits(x, y),
        _ => a == b,
    }
}

/// Struct hinter dem Typnamen einer Umsetzung.
///
/// Der Name steht so in der Registrierung, wie er im Quelltext geschrieben
/// wurde — die Modulumbenennung (`modules.rs`) laeuft NACH dem Parsen und
/// fasst die Registrierung nicht an. Deshalb wird hier in drei Schritten
/// gesucht: der Name selbst, die `gc class` desselben Namens, und zuletzt
/// GENAU EIN Struct, dessen Name auf `__<Name>` endet (das ist der Fall
/// „Typ in einem Modul"). Mehrere Treffer sind ein Fehler — raten waere die
/// gefaehrlichere Wahl.
fn ty_struct(ck: &Checker, name: &str) -> Result<usize, bool> {
    if let Some(i) = ck.tcx.lookup(name) {
        return Ok(i);
    }
    if let Some(i) = ck.tcx.lookup(&format!("gc {}", name)) {
        return Ok(i);
    }
    let suffix = format!("__{}", name);
    let hit: Vec<usize> = ck
        .tcx
        .structs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.ends_with(&suffix))
        .map(|(i, _)| i)
        .collect();
    match hit.len() {
        1 => Ok(hit[0]),
        0 => Err(false),
        _ => Err(true),
    }
}

/// Lesbare Signatur einer Schnittstellenmethode (fuer die Fehlermeldung).
///
/// Die Typen werden ausdruecklich uebergeben: bei einer Signatur mit `Self`
/// stehen sie nicht in der Schnittstelle, sondern haengen an der Umsetzung.
fn signature(ck: &Checker, m: &Method, ptypes: &[Type], rtyp: &Type) -> String {
    let mut s = String::from(if m.mutable { "*mut self" } else { "*self" });
    for t in ptypes {
        s.push_str(", ");
        s.push_str(&ck.tcx.name_of(t));
    }
    match rtyp {
        Type::Void => format!("fn {}({})", m.name, s),
        r => format!("fn {}({}) -> {}", m.name, s, ck.tcx.name_of(r)),
    }
}

/// Loest einen Typ der Schnittstelle auf und setzt dabei `Self` ein.
fn resolve_with_self(ck: &mut Checker, te: &TypeExpr, slf: &Type) -> Type {
    match te {
        TypeExpr::Named(n, _) if n == "Self" => slf.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            Type::ptr(resolve_with_self(ck, inner, slf), *mutable)
        }
        TypeExpr::Array { elem, len, .. } => {
            Type::Array(Box::new(resolve_with_self(ck, elem, slf)), *len)
        }
        _ => ck.resolve_ty(te),
    }
}

/// `// HOOK iface` in `sema::add_items_inner` (NACH `collect_fns`): prueft
/// jede Umsetzung vollstaendig — alle Methoden da, alle Signaturen passend.
pub(crate) fn hook_check_impls(ck: &mut Checker) {
    // 1. Typen der Schnittstellenmethoden aufloesen (einmal je Methode).
    let n = REG.with(|r| r.borrow().ifaces.len());
    for i in 0..n {
        let cnt = REG.with(|r| r.borrow().ifaces[i].methods.len());
        for k in 0..cnt {
            let (done, params, ret, has_self) = REG.with(|r| {
                let reg = r.borrow();
                let m = &reg.ifaces[i].methods[k];
                (m.resolved, m.params.clone(), m.ret.clone(), m.has_self)
            });
            if done {
                continue;
            }
            // Eine Signatur mit `Self` hat GLOBAL keine Typen — sie bekommt
            // sie erst je Umsetzung (`pruefe_umsetzung`). Hier aufzuloesen
            // hiesse, `Self` als gewoehnlichen Typnamen zu suchen, und das
            // waere eine Fehlermeldung ueber einen Typ, den niemand
            // deklarieren wollte.
            if has_self {
                REG.with(|r| r.borrow_mut().ifaces[i].methods[k].resolved = true);
                continue;
            }
            let pt: Vec<Type> = params.iter().map(|t| ck.resolve_ty(t)).collect();
            let rt = match &ret {
                Some(t) => ck.resolve_ty(t),
                None => Type::Void,
            };
            REG.with(|r| {
                let mut reg = r.borrow_mut();
                let m = &mut reg.ifaces[i].methods[k];
                m.ptypes = pt;
                m.rtyp = rt;
                m.resolved = true;
            });
        }
    }
    // 2. Jede Umsetzung pruefen.
    let cnt = REG.with(|r| r.borrow().impls.len());
    for u in 0..cnt {
        let (iface, ty, span, checked) = REG.with(|r| {
            let reg = r.borrow();
            let x = &reg.impls[u];
            (x.iface.clone(), x.ty.clone(), x.span, x.checked)
        });
        if checked {
            continue;
        }
        REG.with(|r| r.borrow_mut().impls[u].checked = true);
        check_impl(ck, u, &iface, &ty, span);
    }
    // 3. Kein Schnittstellenwert im Heap: der Sammler verfolgt dort PRAEZISE
    //    (SPEC §3.5.3) und kennt den Datenzeiger in einem `dyn I` nicht.
    check_gc_fields(ck);
}

fn check_impl(ck: &mut Checker, u: usize, iface: &str, ty: &str, span: Span) {
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => {
            let known: Vec<String> =
                REG.with(|r| r.borrow().ifaces.iter().map(|s| s.name.clone()).collect());
            let note = if known.is_empty() {
                "no interface is declared in this compilation".to_string()
            } else {
                format!("known are: {}", known.join(", "))
            };
            ck.dg.error_note(
                span,
                format!("unknown interface '{}'", iface),
                note,
            );
            return;
        }
    };
    // TRAEGER DER UMSETZUNG: ein Struct oder — seit Runde 50 — ein eingebauter
    // GRUNDTYP. Ein Grundtyp steht nicht in der Structtabelle; `struct_idx`
    // bleibt dann `usize::MAX`, und alles, was diesen Index braucht
    // (Methodentafel, `as dyn I`), gilt fuer ihn nicht. Der dynamische Versand
    // ueber einen Grundtyp ist damit ausgeschlossen, der statische nicht — und
    // genau der wird gebraucht (`vec_sortiere[i32]`).
    // GRUNDTYP ZUERST. `i32` heisst immer der eingebaute Typ; die
    // Endungssuche in `typ_struct` (drittes Feld: „genau ein Struct, dessen
    // Name auf `__<Name>` endet") wuerde sonst `Vec__i32` finden — der ist
    // `Vec[i32]` und nicht `i32`.
    if let Some(t) = base_ty_of_name(ty) {
        return check_impl_am(ck, u, iface, ii, ty, span, usize::MAX, t);
    }
    let (sidx, self_ty) = match ty_struct(ck, ty) {
        Ok(i) => (i, Type::Struct(i)),
        Err(true) => {
            ck.dg.error_note(
                span,
                format!("the type '{}' is ambiguous", ty),
                "several modules declare a type of this name".to_string(),
            );
            return;
        }
        Err(false) => {
            ck.dg.error(span, format!("unknown type '{}'", ty));
            return;
        }
    };
    check_impl_am(ck, u, iface, ii, ty, span, sidx, self_ty)
}

/// Der zweite Teil: die Umsetzung gegen einen BEKANNTEN Traeger pruefen.
/// `sidx == usize::MAX` heisst „Grundtyp" — dann gibt es keinen Struct und
/// damit weder Methodentafel noch `as dyn I`.
fn check_impl_am(
    ck: &mut Checker,
    u: usize,
    iface: &str,
    ii: usize,
    ty: &str,
    span: Span,
    sidx: usize,
    self_ty: Type,
) {
    if sidx != usize::MAX && ck.tcx.structs[sidx].name.starts_with(P_DYN) {
        ck.dg.error_note(
            span,
            format!("'{}' is an interface and not a type", ty),
            "an interface does not implement an interface".to_string(),
        );
        return;
    }
    let prefix = if sidx == usize::MAX {
        ty.to_string()
    } else {
        method_prefix(&ck.tcx, sidx)
    };
    let display = if sidx == usize::MAX {
        ty.to_string()
    } else {
        ck.tcx.structs[sidx].name.clone()
    };
    // Doppelte Umsetzung — verglichen wird der METHODENPRAEFIX des
    // aufgeloesten Traegers, damit `impl I for T` und `impl I for modul.T`
    // als dasselbe erkannt werden und Grundtypen mitzaehlen. (Ein leerer
    // Praefix heisst: jene Umsetzung war schon fehlerhaft.)
    let duplicate = REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .take(u)
            .any(|x| x.iface == iface && !x.prefix.is_empty() && x.prefix == prefix)
    });
    if duplicate {
        ck.dg.error(
            span,
            format!("'{}' already implements the interface '{}'", ty, iface),
        );
        return;
    }
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        reg.impls[u].struct_idx = sidx;
        reg.impls[u].prefix = prefix.clone();
    });
    let methods = REG.with(|r| r.borrow().ifaces[ii].methods.clone());
    let mut complete = true;
    for m in &methods {
        // Bei `Self` haengen die Typen an DIESER Umsetzung, nicht an der
        // Schnittstelle — deshalb hier aufgeloest und nicht in Schritt 1.
        let (ptypes, rtyp): (Vec<Type>, Type) = if m.has_self {
            (
                m.params
                    .iter()
                    .map(|t| resolve_with_self(ck, t, &self_ty))
                    .collect(),
                match &m.ret {
                    Some(t) => resolve_with_self(ck, t, &self_ty),
                    None => Type::Void,
                },
            )
        } else {
            (m.ptypes.clone(), m.rtyp.clone())
        };
        let full = format!("{}{}{}", prefix, crate::impls::TRENNER, m.name);
        let sig = match ck.fns.get(&full) {
            Some(s) => s.clone(),
            None => {
                complete = false;
                ck.dg.error_note(
                    span,
                    format!(
                        "'{}' does not implement the method '{}.{}'",
                        display, iface, m.name
                    ),
                    format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
                );
                continue;
            }
        };
        // Empfaenger: ein Zeiger auf GENAU diesen Typ.
        let recv_ok = match sig.params.first() {
            Some(Type::Ptr { inner, .. }) => **inner == self_ty,
            _ => false,
        };
        if !recv_ok {
            complete = false;
            ck.dg.error_note(
                span,
                format!(
                    "the receiver of '{}.{}' does not fit '{}'",
                    display, m.name, iface
                ),
                format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
            );
            continue;
        }
        if sig.params.len() != ptypes.len() + 1 {
            complete = false;
            ck.dg.error_note(
                span,
                format!(
                    "'{}.{}' has {} parameters, the interface '{}' requires {}",
                    display,
                    m.name,
                    sig.params.len() - 1,
                    iface,
                    ptypes.len()
                ),
                format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
            );
            continue;
        }
        let mut matching = true;
        for (k, expected) in ptypes.iter().enumerate() {
            let actual = &sig.params[k + 1];
            if !fits(actual, expected) {
                matching = false;
                ck.dg.error_note(
                    span,
                    format!(
                        "parameter {} of '{}.{}' has type {}, the interface '{}' requires {}",
                        k + 1,
                        display,
                        m.name,
                        ck.tcx.name_of(actual),
                        iface,
                        ck.tcx.name_of(expected)
                    ),
                    format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
                );
                break;
            }
        }
        if !matching {
            complete = false;
            continue;
        }
        if !fits(&sig.ret, &rtyp) {
            complete = false;
            ck.dg.error_note(
                span,
                format!(
                    "'{}.{}' returns {}, the interface '{}' requires {}",
                    display,
                    m.name,
                    ck.tcx.name_of(&sig.ret),
                    iface,
                    ck.tcx.name_of(&rtyp)
                ),
                format!("'{}' is expected in the block", signature(ck, m, &ptypes, &rtyp)),
            );
        }
    }
    REG.with(|r| r.borrow_mut().impls[u].ok = complete);
}

/// Enthaelt `t` DEM WERT NACH einen Schnittstellenwert?
fn contains_dyn(tcx: &TypeCtx, t: &Type, depth: u32) -> bool {
    if depth > 32 {
        return false;
    }
    match t {
        Type::Struct(i) => match tcx.structs.get(*i) {
            Some(s) if s.name.starts_with(P_DYN) => true,
            Some(s) => s
                .fields
                .iter()
                .any(|f| contains_dyn(tcx, &f.ty, depth + 1)),
            None => false,
        },
        Type::Array(e, _) => contains_dyn(tcx, e, depth + 1),
        _ => false,
    }
}

/// `dyn I` im Heap waere ein Loch in der Sammlerzusage (siehe Kopf der Datei).
fn check_gc_fields(ck: &mut Checker) {
    if !has_interfaces() {
        return;
    }
    let mut findings: Vec<(String, String)> = Vec::new();
    for s in ck.tcx.structs.iter() {
        let class = match s.name.strip_prefix("gc ") {
            Some(k) => k,
            None => continue,
        };
        for f in &s.fields {
            if contains_dyn(&ck.tcx, &f.ty, 0) {
                findings.push((class.to_string(), f.name.clone()));
            }
        }
    }
    for (class, field) in findings {
        ck.dg.error_note(
            Span::none(),
            format!(
                "field '{}' of the gc class '{}' contains an interface value",
                field, class
            ),
            "the collector traces the heap precisely and does not know the pointer behind 'dyn' (SPEC 3.5.3)".to_string(),
        );
    }
}

/// `// HOOK iface` in `sema::expr_inner`, Zweig `Cast` — `x as dyn I`.
///
/// Der Schnittstellenwert entsteht AUSDRUECKLICH (SPEC §6.2: „dynamische
/// Aufloesung ueber `dyn Interface`, ausdruecklich hingeschrieben"). Es gibt
/// keine stille Umwandlung an einer Zuweisung oder an einem Argument — Firn
/// hat keine impliziten Umwandlungen (SPEC §4.5), und fuer eine, die eine
/// Methodentafel anhaengt, waere das die schlechteste Stelle anzufangen.
pub(crate) fn hook_cast(ck: &mut Checker, span: Span, src: &Type, dst: &Type) -> Option<Type> {
    let sidx = match dst {
        Type::Struct(i) => *i,
        _ => return None,
    };
    let iface = ck
        .tcx
        .structs
        .get(sidx)
        .and_then(|s| interface_of(&s.name))?
        .to_string();
    let source = match src {
        Type::Ptr { inner, .. } => match &**inner {
            Type::Struct(j) => *j,
            _ => {
                ck.dg.error_note(
                    span,
                    format!(
                        "an interface value is made from a pointer to a struct, found {}",
                        ck.tcx.name_of(src)
                    ),
                    format!("write '(&x) as dyn {}'", iface),
                );
                return Some(Type::Error);
            }
        },
        _ => {
            ck.dg.error_note(
                span,
                format!(
                    "an interface value is made from a pointer, found {}",
                    ck.tcx.name_of(src)
                ),
                format!("write '(&x) as dyn {}'", iface),
            );
            return Some(Type::Error);
        }
    };
    if impl_ok(&iface, source) {
        return Some(dst.clone());
    }
    let name = ck.tcx.name_of(&Type::Struct(source));
    let known = REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .filter(|x| x.iface == iface && x.struct_idx != usize::MAX)
            .map(|x| x.ty.clone())
            .collect::<Vec<_>>()
    });
    let note = if known.is_empty() {
        format!("no type implements '{}'", iface)
    } else {
        format!("'{}' implement: {}", iface, known.join(", "))
    };
    ck.dg.error_note(
        span,
        format!("'{}' does not implement the interface '{}'", name, iface),
        note,
    );
    Some(Type::Error)
}

/// Setzt der Struct `sidx` die Schnittstelle `iface` vollstaendig um?
fn impl_ok(iface: &str, sidx: usize) -> bool {
    REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .any(|x| x.iface == iface && x.struct_idx == sidx && x.ok)
    })
}

/// Nummer der Methode in der Schnittstelle (= Platz in der Methodentafel).
pub(crate) fn slot_of(iface: &str, method: &str) -> Option<usize> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        r.borrow().ifaces[i]
            .methods
            .iter()
            .position(|m| m.name == method)
    })
}

/// Rueckgabetyp einer Schnittstellenmethode — auch fuer `sema::probe`, damit
/// ein Literal daneben seinen Typ bekommt (`f.flaeche() != 42`).
pub(crate) fn ret_of(iface: &str, method: &str) -> Option<Type> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        r.borrow().ifaces[i]
            .methods
            .iter()
            .find(|m| m.name == method)
            .map(|m| m.rtyp.clone())
    })
}

/// `// HOOK iface` in `impls::hook_call` — `f.m(args)` auf einem `dyn I`.
///
/// Der Empfaenger ist ein WERT (der fette Zeiger selbst). Ein `*dyn I` wird
/// bewusst nicht angenommen: `(*z).m(…)` sagt dasselbe und macht sichtbar,
/// dass zwei Woerter gelesen werden.
pub(crate) fn hook_method(
    ck: &mut Checker,
    iface: &str,
    method: &str,
    args: &[Expr],
    et: &Type,
    is_ptr: bool,
    nspan: Span,
    espan: Span,
) -> Type {
    let display = format!("dyn {}.{}", iface, method);
    let ii = match iface_index(iface) {
        Some(i) => i,
        None => return Type::Error,
    };
    let m = match REG.with(|r| {
        r.borrow().ifaces[ii]
            .methods
            .iter()
            .find(|m| m.name == method)
            .cloned()
    }) {
        Some(m) => m,
        None => {
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            let present: Vec<String> = REG.with(|r| {
                r.borrow().ifaces[ii]
                    .methods
                    .iter()
                    .map(|m| m.name.clone())
                    .collect()
            });
            let note = if present.is_empty() {
                format!("the interface '{}' has no methods", iface)
            } else {
                format!("'{}' has: {}", iface, present.join(", "))
            };
            ck.dg.error_note(
                nspan,
                format!("the interface '{}' has no method '{}'", iface, method),
                note,
            );
            return Type::Error;
        }
    };
    // OBJEKTSICHERHEIT (Runde 50): eine Signatur mit `Self` kennt der Aufrufer
    // ueber `dyn I` nicht — welcher Typ dahintersteckt, steht erst zur
    // Laufzeit fest, und `*Self` waere fuer jeden ein anderer Typ. Solche
    // Methoden gibt es nur STATISCH, ueber eine Schranke.
    if m.has_self {
        for a in &args[1..] {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            nspan,
            format!(
                "'{}.{}' mentions 'Self' and is therefore not callable via 'dyn {}'",
                iface, method, iface
            ),
            format!(
                "call it via a bound: 'fn f[T: {}](x: *T)' — there the type is fixed",
                iface
            ),
        );
        return Type::Error;
    }
    if is_ptr {
        if let Some(recv) = args.first() {
            ck.dg.error_note(
                recv.span,
                format!(
                    "'{}' expects the interface value itself, found {}",
                    display,
                    ck.tcx.name_of(et)
                ),
                format!("write (*x).{}(…)", method),
            );
        }
    }
    let expected = m.ptypes.len();
    let found = args.len().saturating_sub(1);
    if found != expected {
        ck.dg.error(
            espan,
            format!(
                "method '{}' expects {} argument(s), found {}",
                display, expected, found
            ),
        );
    }
    for (i, a) in args[1..].iter().enumerate() {
        match m.ptypes.get(i) {
            Some(p) => ck.check_argument(&display, i + 1, a, p),
            None => ck.type_out_expr(a),
        }
    }
    m.rtyp
}

// ------------------------------------------------------------------ Lowering

/// Methodentafel einer Umsetzung — der Schluessel fuer `Op::VtabAddr`.
/// `None`, wenn dieser Typ die Schnittstelle nicht (vollstaendig) umsetzt.
pub(crate) fn table_of(iface: &str, sidx: usize) -> Option<String> {
    REG.with(|r| {
        r.borrow()
            .impls
            .iter()
            .find(|x| x.iface == iface && x.struct_idx == sidx && x.ok)
            .map(|x| table_key(iface, &x.prefix))
    })
}

// ------------------------------------------------------------- Codegenerator

/// Alle Methodentafeln als `.rodata`-Block.
///
/// Eine Tafel je vollstaendiger Umsetzung — auch fuer eine, die nie in einem
/// `as dyn` vorkommt. Das ist Absicht: der Inhalt haengt allein an der
/// Deklaration, nicht an den Aufrufstellen; eine Tafel, die nur manchmal
/// entsteht, waere die Sorte Zustand, die man beim Fehlersuchen nicht sehen
/// will.
pub(crate) fn tables_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let tables: Vec<(String, Vec<String>)> = REG.with(|r| {
        let reg = r.borrow();
        reg.impls
            .iter()
            .filter(|u| u.ok && u.struct_idx != usize::MAX)
            .filter_map(|u| {
                let ii = reg.ifaces.iter().position(|s| s.name == u.iface)?;
                let targets: Vec<String> = reg.ifaces[ii]
                    .methods
                    .iter()
                    .map(|m| format!("{}{}{}", u.prefix, crate::impls::TRENNER, m.name))
                    .collect();
                Some((table_key(&u.iface, &u.prefix), targets))
            })
            .collect()
    });
    if tables.is_empty() {
        return out;
    }
    let _ = writeln!(out, ".section .rodata");
    let _ = writeln!(out, ".align 8");
    for (key, targets) in tables {
        let _ = writeln!(out, "{}{}:", TAFEL_LABEL, key);
        for z in targets {
            let _ = writeln!(out, "    .quad {}", crate::codegen_x86::label(&z));
        }
    }
    out
}

/// Signatur einer Schnittstellenmethode fuer das Lowering: der Empfaenger
/// zaehlt als erster Parameter (ein Zeiger), danach die Parameter aus der
/// Deklaration. Damit sieht der Aufruf fuer `lower_call` genauso aus wie
/// jeder andere.
fn methods_sig(iface: &str, method: &str) -> Option<crate::sema::FnSig> {
    let i = iface_index(iface)?;
    REG.with(|r| {
        let reg = r.borrow();
        let m = reg.ifaces[i].methods.iter().find(|m| m.name == method)?;
        let mut params = vec![Type::ptr(Type::U8, true)];
        params.extend(m.ptypes.iter().cloned());
        Some(crate::sema::FnSig { params, ret: m.rtyp.clone() })
    })
}

/// `// HOOK iface` in `lower::lower_call` — den dynamischen Versand
/// vorbereiten: Datenzeiger lesen, Methodentafel lesen, Eintrag lesen.
///
/// ```text
/// %b = <adresse des schnittstellenwertes>
/// %d = load.ptr [%b + 0]      ; der Wert selbst
/// %t = load.ptr [%b + 8]      ; die Methodentafel
/// %z = load.ptr [%t + 8*k]    ; die k-te Methode der Schnittstelle
/// ```
///
/// Drei Ladebefehle je Aufruf — das ist der Preis des dynamischen Versands,
/// und er steht hier sichtbar (SPEC §1, Leitsatz 1: nichts versteckt).
pub(crate) fn lower_dispatch(
    lo: &mut crate::lower::Lower,
    iface: &str,
    method: &str,
    recv: &Expr,
    span: Span,
) -> Option<(crate::fir::Val, crate::fir::Val, crate::sema::FnSig)> {
    use crate::fir::{FTy, Op};
    let slot = match slot_of(iface, method) {
        Some(s) => s,
        None => return lo.ice(span, "unknown interface method in lowering"),
    };
    let sig = match methods_sig(iface, method) {
        Some(s) => s,
        None => return lo.ice(span, "interface method without signature in lowering"),
    };
    let base = lo.lower_addr(recv)?;
    let dadr = lo.field_addr_at(base, OFF_DATEN);
    let data = lo.load(FTy::Ptr, dadr);
    let tadr = lo.field_addr_at(base, OFF_TAFEL);
    let table = lo.load(FTy::Ptr, tadr);
    let eadr = lo.field_addr_at(table, 8 * slot as u64);
    let target = lo.load(FTy::Ptr, eadr);
    Some((target, data, sig))
}

/// `// HOOK iface` in `lower::write_into_inner` — `p as dyn I`.
///
/// Zwei Woerter: der Zeiger, wie er ist, und die Adresse der Methodentafel.
/// Der Datenzeiger wird NICHT veraendert (kein Versatz, keine Verschleierung)
/// — nur so findet der konservative Stapelscan des Sammlers das Objekt
/// dahinter (SPEC §3.5.3, siehe Kopf dieser Datei).
pub(crate) fn lower_cast_into(
    lo: &mut crate::lower::Lower,
    addr: crate::fir::Val,
    inner: &Expr,
    t: &Type,
    span: Span,
) -> Option<()> {
    use crate::fir::{FTy, Op};
    let sidx = match t {
        Type::Struct(i) => *i,
        _ => return lo.ice(span, "interface value without struct type"),
    };
    let iface = match lo.info.tcx.structs.get(sidx).and_then(|s| interface_of(&s.name)) {
        Some(i) => i.to_string(),
        None => return lo.ice(span, "conversion into a non-interface type"),
    };
    let source = match lo.ty_of(inner) {
        Type::Ptr { inner, .. } => match *inner {
            Type::Struct(j) => j,
            _ => return lo.ice(span, "interface value from a pointer without struct"),
        },
        _ => return lo.ice(span, "interface value from a non-pointer"),
    };
    let key = match table_of(&iface, source) {
        Some(k) => k,
        None => return lo.ice(span, "implementation without method table in lowering"),
    };
    let pv = lo.lower_expr(inner)?;
    let dadr = lo.field_addr_at(addr, OFF_DATEN);
    lo.store(FTy::Ptr, dadr, pv);
    let tv = lo.push(FTy::Ptr, Op::VtabAddr { table: key });
    let tadr = lo.field_addr_at(addr, OFF_TAFEL);
    lo.store(FTy::Ptr, tadr, tv);
    Some(())
}

/// Assemblername einer Methodentafel.
pub(crate) fn table_label(key: &str) -> String {
    format!("{}{}", TAFEL_LABEL, key)
}
