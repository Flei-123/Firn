//! Treiber des Stufe-0-Compilers: Kommandozeile, Pipeline, Assemblieren/Linken.
//!
//! Pipeline: Quelle -> Lexer -> Parser -> AST -> Typpruefer -> FIR -> Optimierer
//!           -> x86_64-Assembler -> `as` -> `ld` -> ausfuehrbare Datei.
//! `as` und `ld` werden AUSSCHLIESSLICH als Assembler/Linker benutzt.

mod ast;
mod codegen_x86;
mod config;
mod diag;
mod fir;
mod lexer;
mod lower;
mod opt;
mod parser;
mod sema;
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
         --no-opt           Optimierer abschalten\n  \
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
            "--no-opt" => optimize = false,
            "--keep-asm" => keep_asm = true,
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
        Some(input) => Ok(Options { input, output, emit, optimize, keep_asm }),
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
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: kann '{}' nicht lesen: {}", path.display(), e);
            return 2;
        }
    };
    let name = path.display().to_string();
    let mut dg = diag::Diags::new(&name, &src);

    // --- Lexer ---
    let toks = lexer::lex(&src, &mut dg);
    if opts.emit == Emit::Tokens {
        for t in &toks {
            println!("{:>4}:{:<4} {:?}", t.span.line, t.span.col, t.kind);
        }
        dg.print();
        return if dg.has_errors() { 1 } else { 0 };
    }
    if dg.has_errors() {
        dg.print();
        return 1;
    }

    // --- Parser ---
    let prog = parser::parse(&toks, &mut dg);
    if opts.emit == Emit::Ast && !dg.has_errors() {
        println!("{:#?}", prog);
        return 0;
    }
    if dg.has_errors() {
        dg.print();
        return 1;
    }

    // --- Typpruefer ---
    let info = match sema::check(&prog, &mut dg) {
        Some(i) => i,
        None => {
            dg.print();
            if !dg.has_errors() {
                eprintln!("error: interner Fehler im Typpruefer ohne Meldung");
            }
            return 1;
        }
    };
    if dg.has_errors() {
        dg.print();
        return 1;
    }

    // --- Lowering nach FIR ---
    let mut module = match lower::lower(&prog, &info, &mut dg) {
        Some(m) => m,
        None => {
            dg.print();
            if !dg.has_errors() {
                eprintln!("error: interner Fehler beim Lowering ohne Meldung");
            }
            return 1;
        }
    };
    if dg.has_errors() {
        dg.print();
        return 1;
    }

    if opts.emit == Emit::FirRaw {
        print!("{}", module.to_text());
        return 0;
    }

    // --- Optimierer ---
    if opts.optimize {
        let st = opt::optimize(&mut module);
        if std::env::var("FIRNC_OPT_STATS").is_ok() {
            eprintln!(
                "opt: {} Konstanten gefaltet, {} Instruktionen entfernt, {} Bloecke entfernt",
                st.folded, st.removed_insts, st.removed_blocks
            );
        }
    }

    if opts.emit == Emit::FirOpt {
        print!("{}", module.to_text());
        return 0;
    }

    // --- Codegen ---
    let asm = codegen_x86::emit(&module);

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
