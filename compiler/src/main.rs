// SPDX-License-Identifier: GPL-2.0-only
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
mod testrun;
mod attrs;
mod codegen_a64;
mod codegen_switch;
mod codegen_x86;
mod x86enc;
mod asm_intern;
mod asm_x86;
mod a64enc;
mod asm_a64;
mod elfobj;
mod comptime;
mod env;
mod config;
mod checkmode;
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
mod phi;
mod lower;
mod lower_errors;
mod lower_match;
mod modules;
mod mono;
mod mem2reg;
mod escape;
mod nogc;
mod opt;
mod panic_rt;
mod panic_rt_a64;
mod lock;
mod package;
mod package_world;
mod prof;
mod parser;
mod regalloc;
mod regalloc_a64;
mod sema;
mod simd;
mod simd_a64;
mod sizeof;
mod statics;
mod sema_generic;
mod sema_match;
mod peephole;
mod rangecheck;
mod strings;
mod strtype;
mod syscalls;
mod archsel;
mod target;
mod types;
mod win;
mod win_seam;

use std::path::{Path, PathBuf};
use std::time::Instant;
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
    /// ROUND 93: `--lock` writes `firn.lock`, `--locked` demands that it
    /// fits. Both only together with `--package`.
    lock: bool,
    locked: bool,
    emit: Emit,
    optimize: bool,
    keep_asm: bool,
    stats: bool,
    /// Build level and passes switched off one by one (DESIGN_GOALS.md §5)
    optcfg: opt::OptConfig,
    /// `-c` / `--object`: only assemble, do NOT link (round 52).
    /// Always on under the `kernel` profile anyway (SPEC §2: target is ELF object code).
    only_object: bool,
    /// **ROUND 82** — `--timings`: wall clock per phase to stderr.
    timings: bool,
    /// **ROUND 94** — `--test`: the entry point of the binary is the test
    /// runner, not the program's own `main` (`testrun.rs`).
    test_mode: bool,
    /// Output format of the runner: JSON (default) or TAP.
    test_format: testrun::Format,
    /// Time limit per case in seconds; 0 = none.
    test_limit: u32,
    /// `--no-run`: build the test binary, do not start it.
    no_run: bool,
    /// **ROUND FIRN-ENV** — `--env-allow=<prefix>`: which environment
    /// variables `__env_or`/`__env_has` may read. Adds to the default
    /// prefix `FIRN_`; may be given several times and may carry a comma
    /// separated list (env.rs).
    env_allow: Vec<String>,
    /// `--env-log`: print every variable read at build time, with its value.
    env_log: bool,
}

/// **ROUND 82** — the wall clock per compiler phase (`--timings`).
///
/// The round asked where the time of the self compile goes. `perf` answers a
/// different question (which instruction), is not installed everywhere, and
/// needs permissions that a container does not always have. The question here
/// is about the PIPELINE, and one `Instant` per phase answers it exactly,
/// costs nothing and needs no tool.
///
/// The output goes to stderr, sorted by cost, so that
/// `tools/bench82/run.sh` can grep it and a regression limit can hang on it.
struct Timings {
    on: bool,
    start: Instant,
    last: Instant,
    rows: Vec<(&'static str, f64)>,
}

impl Timings {
    fn new(on: bool) -> Timings {
        let now = Instant::now();
        Timings { on, start: now, last: now, rows: Vec::new() }
    }
    /// Closes the phase that has just run. Several passes through the same
    /// name add up — that is what makes `as`/`ld` comparable with the rest.
    fn mark(&mut self, what: &'static str) {
        if !self.on {
            return;
        }
        let now = Instant::now();
        let ms = now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
        match self.rows.iter_mut().find(|r| r.0 == what) {
            Some(r) => r.1 += ms,
            None => self.rows.push((what, ms)),
        }
    }
    fn print(&self) {
        if !self.on {
            return;
        }
        let total = self.start.elapsed().as_secs_f64() * 1000.0;
        let mut rows = self.rows.clone();
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        eprintln!("phase timings (milliseconds, sorted by cost)");
        for (name, ms) in &rows {
            eprintln!("  {:<18} {:9.1} ms  {:5.1} %", name, ms, ms / total * 100.0);
        }
        eprintln!("total {:.1} ms", total);
    }
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
         --lock             write <dir>/firn.lock (only with --package)\n  \
         --locked           build only if firn.lock fits (only with --package)\n  \
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
                              | x86_64-none | aarch64-none (freestanding:\n  \
                              no operating system, ELF object, no syscall)\n  \
                              | x86_64-windows (round WINDOWS: PE/COFF .exe,\n  \
                              Win64 at the boundary, syscall over Win32)\n  \
         --pic              position independent (shared library, round MOBIL)\n  \
         --no-opt           switch off the optimizer (= --opt-level=dev)\n  \
         --opt-level=<lvl>  dev | dev-fast | release-safe | release-fast\n  \
                              (\'dev-fast\' = only debug preserving passes)\n  \
         --no-pass=<name>   switch off a single optimization pass\n  \
         --list-passes      print the pass register with its labels\n  \
         --list-attrs       print the known attributes and their state\n  \
         --env-allow=<pre>  permit build time environment variables with this\n  \
                              prefix (__env_or/__env_has; default: FIRN_)\n  \
         --env-log          print which environment variables were read\n  \
         --strlit=<lit>     decode a string literal (\"..\", b\"..\", u\"..\")\n  \
         --stats            print the size of the FIR (instructions/blocks)\n  \
         --timings          wall clock per compiler phase (ROUND 82)\n  \
         --test             build and RUN the test cases (#[test], ROUND 94)\n  \
         --format=json|tap  report of --test (default: json)\n  \
         --test-limit=<s>   time limit per case in seconds (default 30, 0 = none)\n  \
         --no-run           with --test: only build, do not run\n  \
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
    let mut lock_write = false;
    let mut lock_check = false;
    let mut emit = Emit::Exe;
    let mut optimize = true;
    let mut keep_asm = false;
    let mut stats = false;
    let mut optcfg = opt::OptConfig::default();
    let mut only_object = false;
    let mut timings = false;
    let mut test_mode = false;
    let mut test_format = testrun::Format::Json;
    let mut test_limit: u32 = 30;
    let mut no_run = false;
    let mut env_allow: Vec<String> = Vec::new();
    let mut env_log = false;
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
            // ROUND MOBIL (Certus): position independent code for a
            // shared library. Without the flag every output stays
            // character for character what it was.
            "--pic" => target::pic_set(true),
            _ if a.starts_with("--target=") => {
                if let Err(e) = target::flag_set(&a["--target=".len()..]) {
                    return Err(e);
                }
            }
            "--keep-asm" => keep_asm = true,
            // RUNDE KODIERER: den eigenen Binaerkodierer statt `as` benutzen.
            // Vorgabe bleibt `as`, bis die Gegenprobe ueber den ganzen Baum
            // oktettgleich ist (tools/kodierer/run.sh).
            "--asm-intern" => asm_intern::set(true),
            "--stats" => stats = true,
            "--timings" => timings = true,
            "--test" => test_mode = true,
            "--no-run" => no_run = true,
            _ if a.starts_with("--format=") => {
                let v = &a["--format=".len()..];
                match testrun::Format::from_str(v) {
                    Some(fm) => test_format = fm,
                    None => return Err(format!("unknown format '{}' (json, tap)", v)),
                }
            }
            _ if a.starts_with("--test-limit=") => {
                let v = &a["--test-limit=".len()..];
                match v.parse::<u32>() {
                    Ok(n) => test_limit = n,
                    Err(_) => return Err(format!("'--test-limit={}' is no number", v)),
                }
            }
            // ROUND FIRN-ENV: the allow list of the build time environment
            // (env.rs). Deliberately an option of the BUILD and not of the
            // program: the environment belongs to whoever translates, so
            // the permission does too.
            "--env-log" => env_log = true,
            _ if a.starts_with("--env-allow=") => {
                let v = &a["--env-allow=".len()..];
                if v.is_empty() {
                    return Err("--env-allow expects a prefix, e.g. --env-allow=FV_".to_string());
                }
                env_allow.push(v.to_string());
            }
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
            // ROUND 93: the two lock options take no value — they belong to
            // the `--package` build and are read the same way in `firnc1`.
            "--lock" => lock_write = true,
            "--locked" => lock_check = true,
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
        env_allow,
        env_log,
        input,
        output,
        lock: lock_write,
        locked: lock_check,
        package,
        package_info,
        emit,
        optimize,
        keep_asm,
        stats,
        optcfg,
        only_object,
        timings,
        test_mode,
        test_format,
        test_limit,
        no_run,
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
    // RUNDE KODIERER: `--nur-obj` nimmt eine fertige .s-Datei und macht
    // daraus eine Objektdatei -- der Weg, den `tools/kodierer/vergleich.py`
    // benutzt, um denselben Text einmal durch `as` und einmal durch den
    // eigenen Kodierer zu schicken. Ohne Sprachvorderteil, damit der
    // Vergleich wirklich nur den Kodierer misst.
    if args.iter().any(|a| a == "--nur-obj") {
        std::process::exit(nur_obj(&args));
    }
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("note: '{} --help' shows the options", config::compiler_name());
            std::process::exit(2);
        }
    };
    let rc = run(&opts);
    // ROUND FIRN-ENV: the manifest stands HERE and not inside `run`, so that
    // it is printed on every path -- also when the translation stops with an
    // error. Which variables were read is exactly the question one asks when
    // two builds came out different.
    if opts.env_log {
        eprint!("{}", env::manifest());
    }
    std::process::exit(rc);
}

/// `--nur-obj [--asm-intern] [--target=…] -o <aus.o> <ein.s>`
fn nur_obj(args: &[String]) -> i32 {
    let mut out: Option<PathBuf> = None;
    let mut inp: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--nur-obj" => {}
            "--asm-intern" => asm_intern::set(true),
            "-o" => {
                i += 1;
                out = args.get(i).map(PathBuf::from);
            }
            a if a.starts_with("--target=") => {
                if let Err(e) = target::flag_set(&a["--target=".len()..]) {
                    eprintln!("error: {}", e);
                    return 2;
                }
            }
            a if a.starts_with('-') => {}
            a => inp = Some(PathBuf::from(a)),
        }
        i += 1;
    }
    let (inp, out) = match (inp, out) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            eprintln!("error: --nur-obj braucht <ein.s> und -o <aus.o>");
            return 2;
        }
    };
    match assemble(&inp, &out) {
        Ok(()) => 0,
        Err(c) => c,
    }
}

fn run(opts: &Options) -> i32 {
    // Round 49: the marker "runtime included" belongs to the start of a
    // compilation (codegen_x86::emit prints the state block afterwards).
    crate::gc::runtime_reset();
    // ROUND 89: the table of global variables belongs to ONE compilation
    // (statics.rs), like the panic message table of round 72.
    crate::statics::reset();
    // ROUND 72 (SPEC section 13, item L9): does THIS build level check
    // integer arithmetic? Set once, read by `lower.rs` for every "+ - * /"
    // and narrowing "as".
    crate::checkmode::set_from_level(opts.optcfg.level);
    // ROUND FIRN-ENV (env.rs): WHICH environment variables this translation
    // may read, and whether it says so. Set once, before the first file is
    // parsed -- the parser is where `__env_or` is resolved.
    crate::env::configure(&opts.env_allow, opts.env_log);
    // The sentence stands here and not in `parse_args`, because `firnc1`
    // has to write it CHARACTER FOR CHARACTER and has no `--help` remark
    // there (round 48).
    if opts.package.is_some() && opts.input.is_some() {
        eprint!("error: --package and an input file are mutually exclusive\n");
        return 2;
    }
    // ROUND 93: `--lock`/`--locked` are options OF THE BUILD DRIVER. Without
    // `--package` there is no manifest that a lock file could belong to, and
    // silently doing nothing would be the worst of the three possible
    // answers.
    if opts.package.is_none() && (opts.lock || opts.locked) {
        let which = if opts.locked { "--locked" } else { "--lock" };
        eprint!("{}", lock::text_needs_package(which));
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
    // --- ROUND 93: the lock file. It sits HERE, between resolving and
    // compiling: the input of the build is complete (every module is found
    // and read), and not one instruction has been emitted yet. So
    // `--locked` refuses BEFORE the work, the way `cargo build --locked`
    // does, and never silently builds something else than the file says.
    if opts.lock || opts.locked {
        let dir = opts.package.clone().unwrap_or_default();
        let lockpath = package::join(&dir, lock::LOCKFILE);
        let cwd = package_world::cwd();
        let computed = match lock::text(&world, &files, &cwd) {
            Ok(t) => t,
            Err(e) => {
                eprint!("{}", e);
                return 2;
            }
        };
        if opts.locked {
            match std::fs::read_to_string(&lockpath) {
                Ok(found) => {
                    if let Some(note) = lock::difference(&found, &computed) {
                        eprint!("{}", lock::text_mismatch(&lockpath, &note));
                        return 2;
                    }
                }
                Err(_) => {
                    eprint!("{}", lock::text_missing(&lockpath));
                    return 2;
                }
            }
        } else if std::fs::write(&lockpath, computed.as_bytes()).is_err() {
            // Without the reason of the operating system: `firnc1` has no
            // `strerror`, and the two compilers have to say the same sentence.
            eprint!("error: cannot write '{}'\n", lockpath);
            return 2;
        }
    }
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
    // The file names come out of `modules::resolve` and are, since round 93,
    // already relative to the working directory
    // (`package_world::build_path`) — the artifact must not name the
    // machine it was built on.
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

    // ROUND 82: from here on the phases are measured (`--timings`).
    let mut tm = Timings::new(opts.timings);
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
    tm.mark("lex+parse");
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

    // --- ROUND 94: `--test` -- the runner becomes the entry point --------
    //
    // The same shape as the `comptime` injection above, and for the same
    // reason: source text that arises DURING the compilation is lexed,
    // parsed and appended, after which the type checker sees no difference
    // to hand written text. What is generated here is the runtime half
    // (`lib/test/runner.fi`, embedded) plus one `main` that names every
    // `#[test]` function with its position.
    if opts.test_mode && !dg.has_errors() {
        let found = testrun::tests_of(&prog);
        if found.is_empty() {
            eprintln!(
                "error: no '#[test]' function in '{}' -- nothing to run",
                path.display()
            );
            return 2;
        }
        // Only cases with the right signature; a wrong one is reported by
        // the type checker below, and generating a call to it would produce a
        // SECOND error about generated text the programmer never wrote.
        let cases: Vec<&crate::ast::FnDecl> = found
            .into_iter()
            .filter(|f| f.params.is_empty() && f.ret.is_none() && f.extern_info.is_none())
            .collect();
        if let Some(m) = prog.funcs.iter().find(|f| f.name == "main") {
            // The runner IS the entry point. Two of them would be a linker
            // error a hundred lines further down, with no mention of --test.
            dg.error(
                m.span,
                "with '--test' the entry point is the test runner -- this file must not declare 'main' itself",
            );
            return report(&dg);
        }
        let names: Vec<String> = cases
            .iter()
            .map(|c| dg.file_name(c.span.file).to_string())
            .collect();
        let src = testrun::harness(&cases, &names, opts.test_format, opts.test_limit);
        let count = cases.len();
        let file = dg.add_file("<test runner>", &src);
        dwarf::add_file("<test runner>");
        let toks = lexer::lex_file(&src, file, &mut dg);
        let mut extra = parser::parse(&toks, &mut dg);
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
        prog.statics.extend(extra.statics);
        if opts.stats {
            eprintln!("test: {} cases", count);
        }
    }

    // --- ROUND WINDOWS: the system seam -------------------------------
    //
    // The same shape as the `comptime` injection above and the `--test`
    // one: source text that arises DURING the compilation is lexed,
    // parsed and appended, after which the type checker sees no
    // difference to hand written text.
    //
    // What is injected is `win_seam.rs` -- the layer that answers a
    // `syscall(...)` over Win32, written in Firn. It goes in whenever the
    // target is Windows, because `_start` itself calls into it (the
    // standard handles, the command line) even in a program that never
    // says `syscall` at all.
    if target::windows() && !dg.has_errors() {
        let src = win_seam::source();
        let file = dg.add_file("<windows seam>", &src);
        dwarf::add_file("<windows seam>");
        let toks = lexer::lex_file(&src, file, &mut dg);
        // `parser::parse` would call `reset_hooks` and thereby throw away
        // everything the MAIN parse registered -- the `gc class`es, the
        // interfaces, the error sets, the builtin `str` of round 70. The
        // seam declares none of those, so it is parsed as a further MODULE
        // of the same compilation, which is what it is.
        let mut extra = parser::parse_module(&toks, &mut dg, file, 0);
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
        prog.statics.extend(extra.statics);
        win::note_baseline();
    }

    tm.mark("comptime");
    // ROUND ARM-FREESTANDING: `#[arch(...)]` -- throw away every function
    // that belongs to another machine, BEFORE anything has looked at a type
    // or at a register name (archsel.rs explains why the order matters).
    archsel::select(&mut prog, &mut dg);
    if dg.has_errors() {
        return report(&dg);
    }
    // --- Monomorphization of generic templates (module types) ---
    mono::expand(&mut prog, &mut dg);
    tm.mark("mono");
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
    tm.mark("sema");
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

    tm.mark("lower");
    if opts.stats {
        eprintln!(
            "target:     {}\nprofile:    {}{}",
            target::active().name(),
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

    // ROUND 92 -- there is deliberately NO phi check here.
    //
    // Between two passes the entry lists are allowed to be out of date: a
    // `brcond` that `simplify-term` turns into a `br` removes an edge and
    // leaves an entry behind, and the next `mem2reg` round trims it. Making
    // that an error here would report normal work as a fault.
    //
    // The check that BINDS sits in `phi.rs`, after `simplify_phis` and
    // before a single instruction is emitted, and it runs in every build,
    // not behind an environment variable. `FIRN_VERIFY_PHI=2` is the
    // debugging aid on top of it (`opt.rs::phi_check`): it names the pass
    // that broke something, and a "TWO entries for bb..." line from it is
    // always a bug, while a count mismatch may be one of the transients
    // described above.

    tm.mark("optimizer");
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
    // ROUND 92 -- PHI ELIMINATION, and it happens exactly once, here.
    //
    // `mem2reg.rs` builds phi nodes; no machine has one. Every backend could
    // take them apart for itself, and there are three of them -- that is the
    // shape round 90's bug had (one question, three answers, two of them
    // silently out of date). So the phis become copies ONCE, on FIR, and
    // every code generator below this line reads the same phi-free
    // instruction list it read before round 92.
    if let Err(e) = phi::eliminate(&mut module) {
        eprintln!("error: {}", e);
        return 1;
    }
    // ROUND ARM-FREESTANDING: the code generator is chosen by the
    // INSTRUCTION SET alone. Whether an operating system lies underneath is
    // the other axis of `target.rs`, and it is answered inside the two
    // generators (no `_start`, no system calls), not by picking a third one.
    let emitted = match target::arch() {
        target::Arch::X86_64 => codegen_x86::emit(&module),
        target::Arch::Aarch64 => codegen_a64::emit(&module),
    };
    let asm = match emitted {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {}", e);
            return 1;
        }
    };

    tm.mark("codegen");
    let out = opts
        .output
        .clone()
        .or_else(|| target_out_manifest.clone())
        .unwrap_or_else(|| {
            // ROUND 94: a test binary is not the program, so it does not take
            // the program's name -- `x.fi` becomes `x.test`.
            let d = default_output(path);
            if opts.test_mode {
                d.with_extension("test")
            } else {
                d
            }
        });
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
    tm.mark("write .s");
    if let Err(code) = assemble_and_link(&asm_path, &obj_path, &out) {
        return code;
    }
    tm.mark("as + ld");
    let _ = std::fs::remove_file(&obj_path);
    if !opts.keep_asm {
        let _ = std::fs::remove_file(&asm_path);
    }
    tm.print();
    // ROUND 94: `firnc --test x.fi` builds AND runs, like every other test
    // runner. The exit code is the runner's own (0 = everything passed), so
    // a build server needs nothing but this one command.
    if opts.test_mode && !opts.no_run {
        let exe = if out.is_absolute() {
            out.clone()
        } else {
            std::path::PathBuf::from(".").join(&out)
        };
        match Command::new(&exe).status() {
            Ok(s) => return s.code().unwrap_or(70),
            Err(e) => {
                eprintln!("error: cannot start '{}': {}", exe.display(), e);
                return 2;
            }
        }
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
    // ROUND WINDOWS: an image without `.exe` is not startable there, and a
    // name without a suffix would collide with the Linux build in the same
    // directory.
    if target::windows() {
        p.set_extension("exe");
    }
    p
}

/// Assemble only (`as --64 -o x.o x.s`) — the freestanding output.
fn assemble(asm: &Path, obj: &Path) -> Result<(), i32> {
    let t = target::active();
    // RUNDE KODIERER: der eigene Weg -- kein Prozess, kein `as`.
    if asm_intern::get() {
        let text = match std::fs::read_to_string(asm) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: cannot read '{}': {}", asm.display(), e);
                return Err(3);
            }
        };
        // RUNDE SAMMELN: der Kodierer entscheidet nach dem BEFEHLSSATZ, nicht
        // nach dem Ziel. Auf seiner eigenen Linie gab es nur zwei Ziele; die
        // Linie von BILLIG bringt drei weitere mit (x86_64-none,
        // aarch64-none, x86_64-windows). Freistehend ist eine ELF-Datei wie
        // Linux auch -- dieselben Oktette, nur kein Betriebssystem darunter,
        // also derselbe Kodierer. Windows ist es NICHT: elfobj.rs schreibt
        // ELF, dort wird PE/COFF gebraucht.
        let res = if t.is_windows() {
            Err(String::from(
                "der eigene Kodierer schreibt ELF-Objekte; x86_64-windows \
                 braucht PE/COFF und bleibt auf dem Vorgabepfad ueber `as`",
            ))
        } else {
            match t.arch() {
                target::Arch::X86_64 => asm_x86::assemble_to_object(&text),
                target::Arch::Aarch64 => asm_a64::assemble_to_object(&text),
            }
        };
        return match res {
            Ok(bytes) => match std::fs::write(obj, &bytes) {
                Ok(()) => Ok(()),
                Err(e) => {
                    eprintln!("error: cannot write '{}': {}", obj.display(), e);
                    Err(3)
                }
            },
            Err(e) => {
                eprintln!("error: interner Kodierer: {}", e);
                Err(3)
            }
        };
    }
    // ROUND 93 (reproducibility, ACCEPTANCE item 5): `as` builds a
    // `.debug_line` out of our `.file`/`.loc` directives and puts ITS OWN
    // working directory into it as `DW_AT_comp_dir`. Two checkouts at
    // different places therefore produced binaries that differed in
    // thousands of octets — measured with `tools/repro/run.sh`: 3,562 of
    // 6,840 octets in `package_bin`. `--debug-prefix-map` (binutils, also
    // on the aarch64 side) maps that directory to `.`, and with it the
    // artifact stops knowing where it was built.
    let map = format!("{}=.", package_world::cwd());
    let st = Command::new(t.assembler())
        .args(t.as_flags())
        .arg("--debug-prefix-map")
        .arg(&map)
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
    // `-n` (`--nmagic`) switches OFF the page alignment of the sections and
    // puts everything into ONE loadable segment. That was free as long as
    // a Firn program had nothing but `.text` and `.rodata`: one segment,
    // read + execute, smaller image.
    //
    // ROUND 89 ends that for programs with a `static`. A writable `.data`
    // in the same segment makes the WHOLE segment writable AND executable
    // -- `ld` says so out loud ("LOAD segment with RWX permissions"), and
    // worse, it would make the `.rodata` of an immutable `static`
    // writable, which is exactly the guarantee the missing `mut` is
    // supposed to buy. So: a program with a global variable is linked with
    // page aligned segments, everything else stays bit for bit what it was
    // (`tools/repro`).
    let mut cmd = Command::new(t.linker());
    if t.is_windows() {
        // ROUND WINDOWS. The linker gets exactly three things and no
        // library at all: the entry point (ours, `_start`), the subsystem
        // (a console program, so that stdout is a console and not a
        // window), and a fixed image base so that no relocation section is
        // needed. The import table comes out of our own object file
        // (`win.rs::idata_asm`) -- `-lkernel32` never appears here, and no
        // foreign object file enters the image.
        cmd.arg("-e").arg("_start");
        cmd.arg("--subsystem").arg("console");
    } else if !crate::statics::any() {
        cmd.arg("-n");
    }
    let st = cmd.arg("-o").arg(out).arg(obj).status();
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
