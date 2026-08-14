//! Treiber des Stufe-0-Compilers: Kommandozeile, Pipeline, Assemblieren/Linken.
//!
//! Pipeline: Quelle -> Lexer -> Parser -> AST -> Typpruefer -> FIR -> Optimierer
//!           -> x86_64-Assembler -> `as` -> `ld` -> ausfuehrbare Datei.
//! `as` und `ld` werden AUSSCHLIESSLICH als Assembler/Linker benutzt.

mod abi;
mod ast;
mod codegen_switch;
mod codegen_x86;
mod config;
mod diag;
mod dwarf;
mod fir;
mod inline;
mod lexer;
mod lower;
mod lower_match;
mod modules;
mod mono;
mod mem2reg;
mod opt;
mod parser;
mod regalloc;
mod sema;
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
    Ast,
    /// FIR nach dem Lowering (unoptimiert)
    FirRaw,
    /// FIR nach dem Optimierer
    FirOpt,
}

struct Options {
    input: PathBuf,
    output: Option<PathBuf>,
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
         --emit=exe         ausfuehrbare Datei erzeugen (Standard, ruft as/ld)\n  \
         --emit=asm         x86_64-Assembler auf die Ausgabe schreiben\n  \
         --emit=fir         FIR-Textform (nach Optimierung, sofern aktiv)\n  \
         --emit=fir-raw     FIR direkt nach dem Lowering, ohne Optimierung\n  \
         --emit=fir-opt     FIR nach dem Optimierer\n  \
         --emit=tokens      Tokenstrom (Fehlersuche)\n  \
         --emit=ast         AST als Debug-Text (Fehlersuche)\n  \
         --no-opt           Optimierer abschalten (= --opt-level=dev)\n  \
         --opt-level=<stufe> dev | dev-fast | release-safe | release-fast\n  \
                              ('dev-fast' = nur debugerhaltende Durchgaenge)\n  \
         --no-pass=<name>   einzelnen Optimierungsdurchgang abschalten\n  \
         --list-passes      Durchgangsregister mit Etiketten ausgeben\n  \
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
            _ => {
                if let Some(rest) = a.strip_prefix("--emit=") {
                    emit = match rest {
                        "exe" => Emit::Exe,
                        "asm" => Emit::Asm,
                        "fir" => Emit::FirOpt,
                        "fir-raw" => Emit::FirRaw,
                        "fir-opt" => Emit::FirOpt,
                        "tokens" => Emit::Tokens,
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
    match input {
        Some(input) => Ok(Options { input, output, emit, optimize, keep_asm, stats, optcfg }),
        None => Err(format!("keine Eingabedatei angegeben (.{})", config::FILE_EXT)),
    }
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
    let path = &opts.input;
    // --- Module aufloesen (Wurzeldatei + alle 'import'-Module) ---
    let files = match modules::resolve(path) {
        Ok(f) => f,
        Err(d) => {
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

    let out = opts.output.clone().unwrap_or_else(|| default_output(path));
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
