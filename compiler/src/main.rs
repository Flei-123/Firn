//! Treiber des Stufe-0-Compilers: Kommandozeile, Pipeline, Assemblieren/Linken.
//!
//! Pipeline: Quelle -> Lexer -> Parser -> AST -> Typpruefer -> FIR -> Optimierer
//!           -> x86_64-Assembler -> `as` -> `ld` -> ausfuehrbare Datei.
//! `as` und `ld` werden AUSSCHLIESSLICH als Assembler/Linker benutzt.

mod abi;
mod ast;
mod ast_kanon;
mod layout_kanon;
mod attrs;
mod codegen_switch;
mod codegen_x86;
mod comptime;
mod config;
mod ct;
mod diag;
mod dwarf;
mod errors;
mod fir;
mod gc;
mod gc_lower;
mod impls;
mod inline;
mod layout;
mod lexer;
mod licm;
mod lower;
mod lower_errors;
mod lower_match;
mod modules;
mod mono;
mod mem2reg;
mod nogc;
mod opt;
mod paket;
mod paketwelt;
mod parser;
mod regalloc;
mod sema;
mod sizeof;
mod sema_generic;
mod sema_match;
mod strings;
mod types;

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Emit {
    Exe,
    Asm,
    Tokens,
    AstKanon,
    LayoutKanon,
    TypenKanon,
    Ast,
    /// FIR nach dem Lowering (unoptimiert)
    FirRaw,
    /// FIR nach dem Optimierer
    FirOpt,
    /// nur den von `comptime` erzeugten Quelltext ausgeben
    Comptime,
}

struct Options {
    /// Quelldatei; entfaellt bei `--paket`.
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    /// `--paket <verzeichnis>`: Projekt anhand seines Manifests uebersetzen.
    paket: Option<String>,
    /// `--paket-info <verzeichnis>`: Manifest lesen und berichten.
    paket_info: Option<String>,
    emit: Emit,
    optimize: bool,
    keep_asm: bool,
    stats: bool,
    /// Baustufe und einzeln abgeschaltete Durchgaenge (DESIGNZIELE.md §5)
    optcfg: opt::OptConfig,
}

fn usage() -> String {
    let c = config::compiler_name();
    format!(
        "{name} {ver} — Compiler fuer {lang} (.{ext})\n\
         \n\
         Aufruf: {c} [OPTIONEN] datei.{ext}\n\
         \n\
         Optionen:\n  \
         -o <pfad>          Ausgabedatei (Standard: Eingabename ohne Endung)\n  \
         --paket <verz>     Projekt aus <verz>/firn.paket uebersetzen\n  \
         --paket-info <verz> Manifest von <verz> lesen und berichten\n  \
         --emit=exe         ausfuehrbare Datei erzeugen (Standard, ruft as/ld)\n  \
         --emit=asm         x86_64-Assembler auf die Ausgabe schreiben\n  \
         --emit=fir         FIR-Textform (nach Optimierung, sofern aktiv)\n  \
         --emit=fir-raw     FIR direkt nach dem Lowering, ohne Optimierung\n  \
         --emit=fir-opt     FIR nach dem Optimierer\n  \
         --emit=comptime    nur den von comptime erzeugten Quelltext\n  \
         --emit=tokens      Tokenstrom (Fehlersuche)\n  \
         --emit=ast-kanon   AST in kanonischer, sprachneutraler Form\n  \
         --emit=layout      Speicherlayout und Aufrufkonvention (kanonisch)\n  \
         --emit=typen       AST mit dem Typ an jedem Ausdruck (kanonisch)\n  \
         --emit=ast         AST als Debug-Text (Fehlersuche)\n  \
         --no-opt           Optimierer abschalten (= --opt-level=dev)\n  \
         --opt-level=<stufe> dev | dev-fast | release-safe | release-fast\n  \
                              ('dev-fast' = nur debugerhaltende Durchgaenge)\n  \
         --no-pass=<name>   einzelnen Optimierungsdurchgang abschalten\n  \
         --list-passes      Durchgangsregister mit Etiketten ausgeben\n  \
         --list-attrs       bekannte Attribute und ihren Stand ausgeben\n  \
         --strlit=<lit>     zeichenkettenliteral entschluesseln (\"..\", b\"..\", u\"..\")\n  \
         --stats            Groesse der FIR ausgeben (Instruktionen/Bloecke)\n  \
         --keep-asm         erzeugte .s-Datei behalten\n  \
         --version          Version ausgeben\n  \
         -h, --help         diese Hilfe\n",
        name = c,
        c = c,
        ver = config::VERSION,
        lang = config::LANG_NAME,
        ext = config::FILE_EXT
    )
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut paket: Option<String> = None;
    let mut paket_info: Option<String> = None;
    let mut emit = Emit::Exe;
    let mut optimize = true;
    let mut keep_asm = false;
    let mut stats = false;
    let mut optcfg = opt::OptConfig::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            "--version" => {
                println!("{} {}", config::compiler_name(), config::VERSION);
                std::process::exit(0);
            }
            "--no-opt" => {
                optimize = false;
                optcfg.level = opt::Level::Dev;
            }
            "--list-passes" => {
                print!("{}", opt::passes_text());
                std::process::exit(0);
            }
            "--list-attrs" => {
                print!("{}", attrs::attrs_text());
                std::process::exit(0);
            }
            _ if a.starts_with("--opt-level=") => {
                let v = &a["--opt-level=".len()..];
                match opt::Level::from_str(v) {
                    Some(l) => {
                        optcfg.level = l;
                        optimize = l != opt::Level::Dev;
                    }
                    None => {
                        return Err(format!(
                            "unbekannte Baustufe '{}' (erlaubt: dev, dev-fast, release-safe, release-fast)",
                            v
                        ))
                    }
                }
            }
            _ if a.starts_with("--no-pass=") => {
                let v = &a["--no-pass=".len()..];
                if !opt::OptConfig::is_known(v) {
                    return Err(format!(
                        "unbekannter Optimierungsdurchgang '{}' — '--list-passes' zeigt alle",
                        v
                    ));
                }
                optcfg.disabled.push(v.to_string());
            }
            _ if a.starts_with("--strlit=") => {
                // Modul str: Literalpfad (Bytes/Str/Str16, Maskierungen, WTF-16)
                // ohne Quelldatei nachpruefbar machen.
                match strings::strlit_report(&a["--strlit=".len()..]) {
                    Ok(rep) => {
                        print!("{}", rep);
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            "--keep-asm" => keep_asm = true,
            "--stats" => stats = true,
            "-o" => {
                i += 1;
                match args.get(i) {
                    Some(p) => output = Some(PathBuf::from(p)),
                    None => return Err("-o erwartet einen Pfad".to_string()),
                }
            }
            // Runde 48: der Bau-Treiber. Beide Optionen nehmen ihr
            // Verzeichnis als EIGENES Argument — `firnc1` liest die
            // Kommandozeile genauso.
            "--paket" => {
                i += 1;
                match args.get(i) {
                    Some(p) => paket = Some(p.clone()),
                    None => return Err("--paket erwartet ein Verzeichnis".to_string()),
                }
            }
            "--paket-info" => {
                i += 1;
                match args.get(i) {
                    Some(p) => paket_info = Some(p.clone()),
                    None => return Err("--paket-info erwartet ein Verzeichnis".to_string()),
                }
            }
            _ => {
                if let Some(rest) = a.strip_prefix("--emit=") {
                    emit = match rest {
                        "exe" => Emit::Exe,
                        "asm" => Emit::Asm,
                        "fir" => Emit::FirOpt,
                        "fir-raw" => Emit::FirRaw,
                        "fir-opt" => Emit::FirOpt,
                        "comptime" => Emit::Comptime,
                        "tokens" => Emit::Tokens,
                        "ast-kanon" => Emit::AstKanon,
                        "layout" => Emit::LayoutKanon,
                        "typen" => Emit::TypenKanon,
                        "ast" => Emit::Ast,
                        other => return Err(format!("unbekanntes Ausgabeziel '{}'", other)),
                    };
                } else if let Some(p) = a.strip_prefix("-o") {
                    if !p.is_empty() {
                        output = Some(PathBuf::from(p));
                    }
                } else if a.starts_with('-') {
                    return Err(format!("unbekannte Option '{}'", a));
                } else if input.is_none() {
                    input = Some(PathBuf::from(a));
                } else {
                    return Err("mehr als eine Eingabedatei angegeben".to_string());
                }
            }
        }
        i += 1;
    }
    if input.is_none() && paket.is_none() && paket_info.is_none() {
        return Err(format!("keine Eingabedatei angegeben (.{})", config::FILE_EXT));
    }
    Ok(Options { input, output, paket, paket_info, emit, optimize, keep_asm, stats, optcfg })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print!("{}", usage());
        std::process::exit(2);
    }
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("hinweis: '{} --help' zeigt die Optionen", config::compiler_name());
            std::process::exit(2);
        }
    };
    std::process::exit(run(&opts));
}

fn run(opts: &Options) -> i32 {
    // Der Satz steht hier und nicht in `parse_args`, weil `firnc1` ihn
    // ZEICHENGLEICH schreiben muss und dort keine `--help`-Nachbemerkung
    // hat (Runde 48).
    if opts.paket.is_some() && opts.input.is_some() {
        eprint!("error: --paket und eine eingabedatei schliessen einander aus\n");
        return 2;
    }
    // --- `--paket-info`: Manifest lesen, pruefen, berichten (Runde 48) ---
    if let Some(verz) = &opts.paket_info {
        match paketwelt::Welt::ab_wurzel(verz) {
            Ok(w) => {
                print!("{}", paket::info_text(&w.pakete[0].manifest, verz));
                return 0;
            }
            Err(t) => {
                eprint!("{}", t);
                return 2;
            }
        }
    }
    // --- Paketwelt: mit `--paket` das genannte Projekt, sonst das Manifest
    // ueber der Quelldatei (fehlt eins, ist die Welt leer und nichts aendert
    // sich gegenueber Runde 47).
    let (welt, eingabe, ziel_aus_manifest) = match &opts.paket {
        Some(verz) => {
            let w = match paketwelt::Welt::ab_wurzel(verz) {
                Ok(w) => w,
                Err(t) => {
                    eprint!("{}", t);
                    return 2;
                }
            };
            let m = &w.pakete[0].manifest;
            if m.start.is_empty() {
                eprintln!(
                    "error: {}: das manifest hat keinen einstiegspunkt ('start <pfad>')",
                    w.pakete[0].manifestpfad
                );
                return 2;
            }
            let start = PathBuf::from(paket::verbinde(verz, &m.start));
            let ziel = PathBuf::from(paket::verbinde(verz, &m.name));
            (w, start, Some(ziel))
        }
        None => {
            let p = match &opts.input {
                Some(p) => p.clone(),
                None => {
                    eprintln!("error: keine Eingabedatei angegeben (.{})", config::FILE_EXT);
                    return 2;
                }
            };
            let w = match paketwelt::Welt::ab_datei(&p.display().to_string()) {
                Ok(w) => w,
                Err(t) => {
                    eprint!("{}", t);
                    return 2;
                }
            };
            (w, p, None)
        }
    };
    let path = &eingabe;
    // --- Module aufloesen (Wurzeldatei + alle 'import'-Module) ---
    let files = match modules::resolve(path, &welt) {
        Ok(f) => f,
        Err(modules::Fehler::Paket(t)) => {
            eprint!("{}", t);
            return 2;
        }
        Err(modules::Fehler::Diag(d)) => {
            // Fehler der Modulaufloesung im ueblichen Format ausgeben.
            let src = std::fs::read_to_string(path).unwrap_or_default();
            let mut dg = diag::Diags::new(&path.display().to_string(), &src);
            dg.report(d);
            return report(&dg);
        }
    };
    let root = match files.first() {
        Some(f) => f,
        None => {
            eprintln!("error: keine Quelldatei");
            return 2;
        }
    };
    let mut dg = diag::Diags::new(&root.path.display().to_string(), &root.src);
    for f in files.iter().skip(1) {
        dg.add_file(&f.path.display().to_string(), &f.src);
    }
    // Zeilentabelle fuer .debug_line: anweisungsgenau nur ohne Optimierer.
    dwarf::reset(
        files.iter().map(|f| f.path.display().to_string()).collect(),
        !opts.optimize,
    );

    if opts.emit == Emit::TypenKanon {
        let toks = lexer::lex(&root.src, &mut dg);
        let prog = parser::parse(&toks, &mut dg);
        if dg.has_errors() {
            dg.print();
            return 1;
        }
        match sema::check(&prog, &mut dg) {
            Some(info) => {
                print!("{}", ast_kanon::render_typed(&prog, &info));
                0
            }
            None => {
                dg.print();
                1
            }
        };
        return if dg.has_errors() { 1 } else { 0 };
    }

    if opts.emit == Emit::LayoutKanon {
        let toks = lexer::lex(&root.src, &mut dg);
        let prog = parser::parse(&toks, &mut dg);
        if dg.has_errors() {
            dg.print();
            return 1;
        }
        print!("{}", layout_kanon::render(&prog));
        return 0;
    }

    if opts.emit == Emit::AstKanon {
        // NUR die Wurzeldatei, VOR dem Zusammenfuehren der Module und vor der
        // Monomorphisierung: der Parser in Firn sieht ebenfalls genau eine
        // Datei. Alles andere waere kein Vergleich, sondern ein Vergleich mit
        // etwas anderem.
        let toks = lexer::lex(&root.src, &mut dg);
        let prog = parser::parse(&toks, &mut dg);
        if dg.has_errors() {
            dg.print();
            return 1;
        }
        print!("{}", ast_kanon::render(&prog));
        return 0;
    }

    if opts.emit == Emit::Tokens {
        let toks = lexer::lex(&root.src, &mut dg);
        for t in &toks {
            println!("{:>4}:{:<4} {:?}", t.span.line, t.span.col, t.kind);
        }
        dg.print();
        return if dg.has_errors() { 1 } else { 0 };
    }

    // --- Lexer + Parser je Modul, danach zusammenfuehren ---
    let mut prog = match modules::build_program(&files, &mut dg) {
        Some(p) => p,
        None => return report(&dg),
    };
    // --- comptime: erzeugten Quelltext im SELBEN Lauf uebersetzen (SPEC §6.4)
    //
    // Die `comptime { … }`-Bloecke laufen VOR der Typpruefung. Was sie per
    // `emit_*` schreiben, wird hier gelext, geparst und ans Programm
    // angehaengt — danach sieht der Typpruefer keinen Unterschied zu von Hand
    // geschriebenem Quelltext. Genau das verlangt Abnahmepunkt 6 fuer die
    // Unicode-, Web-IDL- und CSS-Tabellen eines Browsers.
    let basis = root
        .path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let erzeugt = comptime::fuehre_bloecke_aus(&prog, &mut dg, &basis);
    if !erzeugt.is_empty() && !dg.has_errors() {
        let datei = dg.add_file("<comptime>", &erzeugt);
        // Dieselbe Datei muss auch die Zeilentabelle kennen, sonst erzeugt der
        // Codegenerator `.loc`-Direktiven mit einer Nummer, die `as` nicht
        // kennt ("unassigned file number").
        dwarf::add_file("<comptime>");
        let toks = lexer::lex_file(&erzeugt, datei, &mut dg);
        let mut zusatz = parser::parse(&toks, &mut dg);
        // Die Ausdrucks-Ids des Zusatzes beginnen bei 0 und muessen hinter
        // die des Hauptprogramms wandern.
        let mut naechste = prog.expr_count;
        for f in zusatz.funcs.iter_mut() {
            crate::mono::renumber_block(&mut f.body, &mut naechste);
        }
        for c in zusatz.consts.iter_mut() {
            crate::mono::renumber_expr(&mut c.value, &mut naechste);
        }
        prog.expr_count = naechste;
        prog.funcs.extend(zusatz.funcs);
        prog.structs.extend(zusatz.structs);
        prog.consts.extend(zusatz.consts);
        if opts.emit == Emit::Comptime {
            print!("{}", erzeugt);
            return if dg.has_errors() { report(&dg) } else { 0 };
        }
    }

    // --- Monomorphisierung generischer Vorlagen (Modul types) ---
    mono::expand(&mut prog, &mut dg);
    if opts.emit == Emit::Ast && !dg.has_errors() {
        println!("{:#?}", prog);
        println!("\n// Anweisungsuebersicht (Zeile:Spalte Art)");
        for f in &prog.funcs {
            println!("fn {}:", f.name);
            for s in &f.body.stmts {
                let sp = s.span();
                println!("  {}:{} {}", sp.line, sp.col, s.kind_name());
            }
        }
        return 0;
    }
    if dg.has_errors() {
        return report(&dg);
    }

    // --- Typpruefer ---
    let info = match sema::check(&prog, &mut dg) {
        Some(i) => i,
        None => {
            if !dg.has_errors() {
                eprintln!("error: interner Fehler im Typpruefer ohne Meldung");
                return 1;
            }
            return report(&dg);
        }
    };
    if dg.has_errors() {
        return report(&dg);
    }

    // --- Lowering nach FIR ---
    let mut module = match lower::lower(&prog, &info, &mut dg) {
        Some(m) => m,
        None => {
            if !dg.has_errors() {
                eprintln!("error: interner Fehler beim Lowering ohne Meldung");
                return 1;
            }
            return report(&dg);
        }
    };
    if dg.has_errors() {
        return report(&dg);
    }

    if opts.stats {
        eprintln!(
            "fir (roh):  {} Funktionen, {} Bloecke, {} Instruktionen",
            module.funcs.len(),
            module.block_count(),
            module.inst_count()
        );
    }

    if opts.emit == Emit::FirRaw {
        print!("{}", module.to_text());
        return 0;
    }

    // --- Optimierer ---
    if opts.optimize {
        let st = opt::optimize_with(&mut module, &opts.optcfg);
        if std::env::var(format!("{}_OPT_STATS", config::compiler_name().to_uppercase())).is_ok() {
            eprintln!(
                "opt: {} Konstanten gefaltet, {} Instruktionen entfernt, {} Bloecke entfernt",
                st.folded, st.removed_insts, st.removed_blocks
            );
        }
    }

    if opts.stats {
        eprintln!(
            "fir (opt):  {} Funktionen, {} Bloecke, {} Instruktionen",
            module.funcs.len(),
            module.block_count(),
            module.inst_count()
        );
    }

    if opts.emit == Emit::FirOpt {
        print!("{}", module.to_text());
        return 0;
    }

    // --- Codegen ---
    let asm = match codegen_x86::emit(&module) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {}", e);
            return 1;
        }
    };

    let out = opts
        .output
        .clone()
        .or_else(|| ziel_aus_manifest.clone())
        .unwrap_or_else(|| default_output(path));
    if opts.emit == Emit::Asm {
        if let Err(e) = std::fs::write(&out, asm.as_bytes()) {
            eprintln!("error: kann '{}' nicht schreiben: {}", out.display(), e);
            return 2;
        }
        return 0;
    }

    // --- Assemblieren und Linken ---
    let asm_path = out.with_extension("s");
    let obj_path = out.with_extension("o");
    if let Err(e) = std::fs::write(&asm_path, asm.as_bytes()) {
        eprintln!("error: kann '{}' nicht schreiben: {}", asm_path.display(), e);
        return 2;
    }
    if let Err(code) = assemble_and_link(&asm_path, &obj_path, &out) {
        return code;
    }
    let _ = std::fs::remove_file(&obj_path);
    if !opts.keep_asm {
        let _ = std::fs::remove_file(&asm_path);
    }
    0
}

/// Gibt alle gesammelten Fehler aus und liefert den Exit-Code.
fn report(dg: &diag::Diags) -> i32 {
    dg.print();
    if dg.is_full() {
        eprintln!(
            "hinweis: weitere Fehler in '{}' wurden unterdrueckt ({} angezeigt)",
            dg.file(),
            dg.count()
        );
    }
    1
}

fn default_output(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    p.set_extension("");
    if p.as_os_str().is_empty() {
        p = PathBuf::from("a.out");
    }
    p
}

fn assemble_and_link(asm: &Path, obj: &Path, out: &Path) -> Result<(), i32> {
    let st = Command::new("as").arg("--64").arg("-o").arg(obj).arg(asm).status();
    match st {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("error: 'as' schlug fehl ({})", s);
            return Err(3);
        }
        Err(e) => {
            eprintln!("error: 'as' nicht ausfuehrbar: {} (binutils installiert?)", e);
            return Err(3);
        }
    }
    let st = Command::new("ld").arg("-n").arg("-o").arg(out).arg(obj).status();
    match st {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("error: 'ld' schlug fehl ({})", s);
            Err(3)
        }
        Err(e) => {
            eprintln!("error: 'ld' nicht ausfuehrbar: {} (binutils installiert?)", e);
            Err(3)
        }
    }
}
