//! Driver of the stage 0 compiler: command line, pipeline, assembling/linking.
//!
//! Pipeline: source -> lexer -> parser -> AST -> type checker -> FIR -> optimizer
//!           -> x86_64 assembler -> `as` -> `ld` -> executable file.
//! `as` and `ld` get used EXCLUSIVELY as assembler/linker.

mod abi;
mod ast;
mod ast_canon;
mod layout_canon;
mod atomic;
mod thread;
mod attrs;
mod codegen_a64;
mod codegen_switch;
mod codegen_x86;
mod comptime;
mod config;
mod ct;
mod diag;
mod dwarf;
mod dwarf_info;
mod lsp;
mod errors;
mod extfn;
mod threading;
mod fir;
mod fnval;
mod gc;
mod gc_lower;
mod iface;
mod impls;
mod core;
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
mod package;
mod package_world;
mod prof;
mod parser;
mod regalloc;
mod sema;
mod sizeof;
mod sema_generic;
mod sema_match;
mod strings;
mod strtype;
mod syscalls;
mod target;
mod types;

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Emit {
    Exe,
    Asm,
    Tokens,
    AstCanon,
    LayoutCanon,
    TypesCanon,
    Ast,
    /// FIR after lowering (unoptimized)
    FirRaw,
    /// FIR after the optimizer
    FirOpt,
    /// print the source text produced by `comptime` only
    Comptime,
}

struct Options {
    /// Source file; dropped with `--package`.
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    /// `--package <dir>`: compile the project by way of its manifest.
    package: Option<String>,
    /// `--package-info <dir>`: read the manifest and report.
    package_info: Option<String>,
    emit: Emit,
    optimize: bool,
    keep_asm: bool,
    stats: bool,
    /// Build level and passes switched off one by one (DESIGN_GOALS.md §5)
    optcfg: opt::OptConfig,
    /// `-c` / `--object`: only assemble, do NOT link (round 52).
    /// Always on under the `kernel` profile anyway (SPEC §2: target is ELF object code).
    only_object: bool,
}

fn usage() -> String {
    let c = config::compiler_name();
    format!(
        "{name} {ver} — compiler for {lang} (.{ext})\n\
         \n\
         Usage: {c} [OPTIONS] file.{ext}\n\
         \n\
         Options:\n  \
         -o <path>          output file (default: input name without extension)\n  \
         --package <dir>      compile the project from <dir>/firn.package\n  \
         --package-info <dir> read the manifest of <dir> and report\n  \
         --emit=exe         produce an executable (default, calls as/ld)\n  \
         --emit=asm         write x86_64 assembler to the output\n  \
         --emit=fir         FIR text form (after optimization, if active)\n  \
         --emit=fir-raw     FIR right after lowering, without optimization\n  \
         --emit=fir-opt     FIR after the optimizer\n  \
         --emit=comptime    only the source text produced by comptime\n  \
         --emit=tokens      token stream (troubleshooting)\n  \
         --emit=ast-canon   AST in canonical, language neutral form\n  \
         --emit=layout      memory layout and calling convention (canonical)\n  \
         --emit=types       AST with the type at every expression (canonical)\n  \
         --emit=ast         AST as debug text (troubleshooting)\n  \
         --lsp              language server over standard input/output\n  \
         -c, --object       only assemble: ELF object file, no ld\n  \
         --profile=<name>   kernel | app (SPEC 2), forces the profile\n  \
         --target=<name>    x86_64-linux (default) | aarch64-linux (round 80)\n  \
         --no-opt           switch off the optimizer (= --opt-level=dev)\n  \
         --opt-level=<lvl>  dev | dev-fast | release-safe | release-fast\n  \
                              (\'dev-fast\' = only debug preserving passes)\n  \
         --no-pass=<name>   switch off a single optimization pass\n  \
         --list-passes      print the pass register with its labels\n  \
         --list-attrs       print the known attributes and their state\n  \
         --strlit=<lit>     decode a string literal (\"..\", b\"..\", u\"..\")\n  \
         --stats            print the size of the FIR (instructions/blocks)\n  \
         --keep-asm         keep the generated .s file\n  \
         --version          print the version\n  \
         -h, --help         this help\n",
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
    let mut package: Option<String> = None;
    let mut package_info: Option<String> = None;
    let mut emit = Emit::Exe;
    let mut optimize = true;
    let mut keep_asm = false;
    let mut stats = false;
    let mut optcfg = opt::OptConfig::default();
    let mut only_object = false;
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
                            "unknown build level '{}' (allowed: dev, dev-fast, release-safe, release-fast)",
                            v
                        ))
                    }
                }
            }
            _ if a.starts_with("--no-pass=") => {
                let v = &a["--no-pass=".len()..];
                if !opt::OptConfig::is_known(v) {
                    return Err(format!(
                        "unknown optimization pass '{}' — '--list-passes' shows all",
                        v
                    ));
                }
                optcfg.disabled.push(v.to_string());
            }
            _ if a.starts_with("--strlit=") => {
                // Module str: make the literal path (Bytes/Str/Str16, escapes, WTF-16)
                // checkable without a source file.
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
            "-c" | "--object" => only_object = true,
            _ if a.starts_with("--profile=") => {
                if let Err(e) = prof::flag_set(&a["--profile=".len()..]) {
                    return Err(e);
                }
            }
            // ROUND 80: the second machine. Without this option nothing
            // changes -- `target::active()` answers `x86_64-linux` and every
            // path below is the one that has always been walked.
            _ if a.starts_with("--target=") => {
                if let Err(e) = target::flag_set(&a["--target=".len()..]) {
                    return Err(e);
                }
            }
            "--keep-asm" => keep_asm = true,
            "--stats" => stats = true,
            "-o" => {
                i += 1;
                match args.get(i) {
                    Some(p) => output = Some(PathBuf::from(p)),
                    None => return Err("-o expects a path".to_string()),
                }
            }
            // Round 48: the build driver. Both options take their
            // directory as a SEPARATE argument — `firnc1` reads the
            // command line the same way.
            "--package" => {
                i += 1;
                match args.get(i) {
                    Some(p) => package = Some(p.clone()),
                    None => return Err("--package expects a directory".to_string()),
                }
            }
            "--package-info" => {
                i += 1;
                match args.get(i) {
                    Some(p) => package_info = Some(p.clone()),
                    None => return Err("--package-info expects a directory".to_string()),
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
                        "ast-canon" => Emit::AstCanon,
                        "layout" => Emit::LayoutCanon,
                        "types" => Emit::TypesCanon,
                        "ast" => Emit::Ast,
                        other => return Err(format!("unknown output target '{}'", other)),
                    };
                } else if let Some(p) = a.strip_prefix("-o") {
                    if !p.is_empty() {
                        output = Some(PathBuf::from(p));
                    }
                } else if a.starts_with('-') {
                    return Err(format!("unknown option '{}'", a));
                } else if input.is_none() {
                    input = Some(PathBuf::from(a));
                } else {
                    return Err("more than one input file given".to_string());
                }
            }
        }
        i += 1;
    }
    if input.is_none() && package.is_none() && package_info.is_none() {
        return Err(format!("no input file given (.{})", config::FILE_EXT));
    }
    Ok(Options {
        input,
        output,
        package,
        package_info,
        emit,
        optimize,
        keep_asm,
        stats,
        optcfg,
        only_object,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print!("{}", usage());
        std::process::exit(2);
    }
    // ROUND 64: the language server. It has no input file and no output
    // file -- it speaks the Language Server Protocol over standard
    // input/output and lives as long as the editor does.
    if args.len() == 1 && args[0] == "--lsp" {
        std::process::exit(lsp::serve());
    }
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("note: '{} --help' shows the options", config::compiler_name());
            std::process::exit(2);
        }
    };
    std::process::exit(run(&opts));
}

fn run(opts: &Options) -> i32 {
    // Round 49: the marker "runtime included" belongs to the start of a
    // compilation (codegen_x86::emit prints the state block afterwards).
    crate::gc::runtime_reset();
    // The sentence stands here and not in `parse_args`, because `firnc1`
    // has to write it CHARACTER FOR CHARACTER and has no `--help` remark
    // there (round 48).
    if opts.package.is_some() && opts.input.is_some() {
        eprint!("error: --package and an input file are mutually exclusive\n");
        return 2;
    }
    // --- `--package-info`: read the manifest, check it, report (round 48) ---
    if let Some(dir) = &opts.package_info {
        match package_world::World::ab_root(dir) {
            Ok(w) => {
                print!("{}", package::info_text(&w.packages[0].manifest, dir));
                return 0;
            }
            Err(t) => {
                eprint!("{}", t);
                return 2;
            }
        }
    }
    // --- Package world: with `--package` the project named, otherwise the
    // manifest above the source file (without one the world is empty and
    // nothing changes compared to round 47).
    let (world, input, target_out_manifest) = match &opts.package {
        Some(dir) => {
            let w = match package_world::World::ab_root(dir) {
                Ok(w) => w,
                Err(t) => {
                    eprint!("{}", t);
                    return 2;
                }
            };
            let m = &w.packages[0].manifest;
            if m.start.is_empty() {
                eprintln!(
                    "error: {}: the manifest has no entry point ('start <path>')",
                    w.packages[0].manifestpfad
                );
                return 2;
            }
            let start = PathBuf::from(package::join(dir, &m.start));
            let target = PathBuf::from(package::join(dir, &m.name));
            (w, start, Some(target))
        }
        None => {
            let p = match &opts.input {
                Some(p) => p.clone(),
                None => {
                    eprintln!("error: no input file given (.{})", config::FILE_EXT);
                    return 2;
                }
            };
            let w = match package_world::World::ab_file(&p.display().to_string()) {
                Ok(w) => w,
                Err(t) => {
                    eprint!("{}", t);
                    return 2;
                }
            };
            (w, p, None)
        }
    };
    let path = &input;
    // --- Resolve modules (root file + all 'import' modules) ---
    let files = match modules::resolve(path, &world) {
        Ok(f) => f,
        Err(modules::Error::Package(t)) => {
            eprint!("{}", t);
            return 2;
        }
        Err(modules::Error::Diag(d)) => {
            // Print errors of the module resolution using the usual format.
            let src = std::fs::read_to_string(path).unwrap_or_default();
            let mut dg = diag::Diags::new(&path.display().to_string(), &src);
            dg.report(d);
            return report(&dg);
        }
    };
    let root = match files.first() {
        Some(f) => f,
        None => {
            eprintln!("error: no source file");
            return 2;
        }
    };
    let mut dg = diag::Diags::new(&root.path.display().to_string(), &root.src);
    for f in files.iter().skip(1) {
        dg.add_file(&f.path.display().to_string(), &f.src);
    }
    // Line table for .debug_line: instruction-exact only without the optimizer.
    dwarf::reset(
        files.iter().map(|f| f.path.display().to_string()).collect(),
        !opts.optimize,
    );

    if opts.emit == Emit::TypesCanon {
        let toks = lexer::lex(&root.src, &mut dg);
        let prog = parser::parse(&toks, &mut dg);
        if dg.has_errors() {
            dg.print();
            return 1;
        }
        match sema::check(&prog, &mut dg) {
            Some(info) => {
                print!("{}", ast_canon::render_typed(&prog, &info));
                0
            }
            None => {
                dg.print();
                1
            }
        };
        return if dg.has_errors() { 1 } else { 0 };
    }

    if opts.emit == Emit::LayoutCanon {
        let toks = lexer::lex(&root.src, &mut dg);
        let prog = parser::parse(&toks, &mut dg);
        if dg.has_errors() {
            dg.print();
            return 1;
        }
        print!("{}", layout_canon::render(&prog));
        return 0;
    }

    if opts.emit == Emit::AstCanon {
        // The root file ONLY, BEFORE merging the modules and before
        // monomorphization: the parser written in Firn sees exactly one
        // file too. Anything else would be no comparison but a comparison with
        // something else.
        let toks = lexer::lex(&root.src, &mut dg);
        let prog = parser::parse(&toks, &mut dg);
        if dg.has_errors() {
            dg.print();
            return 1;
        }
        print!("{}", ast_canon::render(&prog));
        return 0;
    }

    if opts.emit == Emit::Tokens {
        let toks = lexer::lex(&root.src, &mut dg);
        for t in &toks {
            // ROUND 71: a float token carries two bit patterns since this
            // round. The dump shows the binary64 alone, exactly as before --
            // the token stream is a fixed interface (tools/lex_compare.sh),
            // and the second pattern is derivable from the first one anyway
            // for everybody who wants it.
            match &t.kind {
                lexer::TokKind::Float(bits, _) => {
                    println!("{:>4}:{:<4} Float({})", t.span.line, t.span.col, bits)
                }
                k => println!("{:>4}:{:<4} {:?}", t.span.line, t.span.col, k),
            }
        }
        dg.print();
        return if dg.has_errors() { 1 } else { 0 };
    }

    // --- Lexer + parser per module, merged afterwards ---
    let mut prog = match modules::build_program(&files, &mut dg) {
        Some(p) => p,
        None => return report(&dg),
    };
    // --- comptime: compile the produced source text in the SAME run (SPEC §6.4)
    //
    // The `comptime { … }` blocks run BEFORE the type check. What they write
    // through `emit_*` is lexed here, parsed and appended to the program —
    // after that the type checker sees no difference to hand written source
    // text. Exactly that is what acceptance point 6 demands for the Unicode,
    // Web IDL and CSS tables of a browser.
    let base = root
        .path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let generated = comptime::run_blocks_out(&prog, &mut dg, &base);
    if !generated.is_empty() && !dg.has_errors() {
        let file = dg.add_file("<comptime>", &generated);
        // The line table has to know that same file too, otherwise the code
        // generator produces `.loc` directives with a number that `as` does
        // not know ("unassigned file number").
        dwarf::add_file("<comptime>");
        let toks = lexer::lex_file(&generated, file, &mut dg);
        let mut extra = parser::parse(&toks, &mut dg);
        // The expression ids of the addition start at 0 and have to move behind
        // those of the main program.
        let mut next = prog.expr_count;
        for f in extra.funcs.iter_mut() {
            crate::mono::renumber_block(&mut f.body, &mut next);
        }
        for c in extra.consts.iter_mut() {
            crate::mono::renumber_expr(&mut c.value, &mut next);
        }
        prog.expr_count = next;
        prog.funcs.extend(extra.funcs);
        prog.structs.extend(extra.structs);
        prog.consts.extend(extra.consts);
        if opts.emit == Emit::Comptime {
            print!("{}", generated);
            return if dg.has_errors() { report(&dg) } else { 0 };
        }
    }

    // --- Monomorphization of generic templates (module types) ---
    mono::expand(&mut prog, &mut dg);
    if opts.emit == Emit::Ast && !dg.has_errors() {
        println!("{:#?}", prog);
        println!("\n// statement overview (line:column kind)");
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

    // --- Type checker ---
    let info = match sema::check(&prog, &mut dg) {
        Some(i) => i,
        None => {
            if !dg.has_errors() {
                eprintln!("error: internal error in the type checker without message");
                return 1;
            }
            return report(&dg);
        }
    };
    if dg.has_errors() {
        return report(&dg);
    }

    // --- Lowering to FIR ---
    let mut module = match lower::lower(&prog, &info, &mut dg) {
        Some(m) => m,
        None => {
            if !dg.has_errors() {
                eprintln!("error: internal error during lowering without message");
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
            "profile:    {}{}",
            prof::name(),
            if core::block_count() > 0 {
                format!("  ({} asm blocks)", core::block_count())
            } else {
                String::new()
            }
        );
        eprintln!(
            "fir (raw):  {} functions, {} blocks, {} instructions",
            module.funcs.len(),
            module.block_count(),
            module.inst_count()
        );
    }

    if opts.emit == Emit::FirRaw {
        print!("{}", module.to_text());
        return 0;
    }

    // --- Optimizer ---
    if opts.optimize {
        let st = opt::optimize_with(&mut module, &opts.optcfg);
        if std::env::var(format!("{}_OPT_STATS", config::compiler_name().to_uppercase())).is_ok() {
            eprintln!(
                "opt: {} constants folded, {} instructions removed, {} blocks removed",
                st.folded, st.removed_insts, st.removed_blocks
            );
        }
    }

    if opts.stats {
        eprintln!(
            "fir (opt):  {} functions, {} blocks, {} instructions",
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
    //
    // ROUND 80: the ONE place at which the machine is chosen. Everything
    // above this line -- lexer, parser, checker, lowering, optimizer -- has
    // no idea which machine it is working for, and that is the whole point
    // of the round.
    let emitted = match target::active() {
        target::Target::X86_64 => codegen_x86::emit(&module),
        target::Target::Aarch64 => codegen_a64::emit(&module),
    };
    let asm = match emitted {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {}", e);
            return 1;
        }
    };

    let out = opts
        .output
        .clone()
        .or_else(|| target_out_manifest.clone())
        .unwrap_or_else(|| default_output(path));
    if opts.emit == Emit::Asm {
        if let Err(e) = std::fs::write(&out, asm.as_bytes()) {
            eprintln!("error: cannot write '{}': {}", out.display(), e);
            return 2;
        }
        return 0;
    }

    // --- Assemble, and link only under the app profile ---
    //
    // ROUND 52 (SPEC §2): the kernel profile produces a freestanding
    // ELF OBJECT FILE. No `ld`, no `_start`, no libc contact — linking
    // happens later at the kernel build with its own linker script.
    let object = opts.only_object || prof::is_kernel();
    let asm_path = out.with_extension("s");
    if let Err(e) = std::fs::write(&asm_path, asm.as_bytes()) {
        eprintln!("error: cannot write '{}': {}", asm_path.display(), e);
        return 2;
    }
    if object {
        // Without `-o` the result is called `<input>.o`; with `-o` exactly as
        // written there (the name may then stay without a suffix).
        let obj_path = match &opts.output {
            Some(p) => p.clone(),
            None => out.with_extension("o"),
        };
        if let Err(code) = assemble(&asm_path, &obj_path) {
            return code;
        }
        if !opts.keep_asm {
            let _ = std::fs::remove_file(&asm_path);
        }
        return 0;
    }
    let obj_path = out.with_extension("o");
    if let Err(code) = assemble_and_link(&asm_path, &obj_path, &out) {
        return code;
    }
    let _ = std::fs::remove_file(&obj_path);
    if !opts.keep_asm {
        let _ = std::fs::remove_file(&asm_path);
    }
    0
}

/// Prints all collected errors and yields the exit code.
fn report(dg: &diag::Diags) -> i32 {
    dg.print();
    if dg.is_full() {
        eprintln!(
            "note: further errors in '{}' were suppressed ({} shown)",
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

/// Assemble only (`as --64 -o x.o x.s`) — the freestanding output.
fn assemble(asm: &Path, obj: &Path) -> Result<(), i32> {
    let t = target::active();
    let st = Command::new(t.assembler())
        .args(t.as_flags())
        .arg("-o")
        .arg(obj)
        .arg(asm)
        .status();
    match st {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("error: '{}' failed ({})", t.assembler(), s);
            Err(3)
        }
        Err(e) => {
            eprintln!(
                "error: cannot run '{}': {} (binutils installed?)",
                t.assembler(),
                e
            );
            Err(3)
        }
    }
}

fn assemble_and_link(asm: &Path, obj: &Path, out: &Path) -> Result<(), i32> {
    let t = target::active();
    assemble(asm, obj)?;
    let st = Command::new(t.linker()).arg("-n").arg("-o").arg(out).arg(obj).status();
    match st {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("error: '{}' failed ({})", t.linker(), s);
            Err(3)
        }
        Err(e) => {
            eprintln!(
                "error: cannot run '{}': {} (binutils installed?)",
                t.linker(),
                e
            );
            Err(3)
        }
    }
}
