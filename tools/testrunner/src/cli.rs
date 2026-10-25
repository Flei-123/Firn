use std::path::PathBuf;

pub const ALL_LEVELS: [&str; 4] = ["opt", "noopt", "devfast", "safe"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
    Jsonl,
    Junit,
}

pub struct Options {
    pub root: PathBuf,
    pub compiler: PathBuf,
    pub format: Format,
    pub filter: Option<String>,
    pub levels: Vec<String>,
    pub out: Option<PathBuf>,
    pub jobs: usize,
    pub timeout_secs: u64,
    pub retries: usize,
    pub progress: bool,
    pub quiet: bool,
}

pub fn usage() -> &'static str {
    "usage: testrunner [--root DIR] [--compiler PATH] [--format text|json|jsonl|junit]\n\
     \x20                 [--out FILE] [--filter TEXT] [--levels opt,noopt,devfast,safe]\n\
     \x20                 [--jobs N] [--timeout SECONDS] [--retries N] [--progress] [--quiet]\n"
}

fn take_value(args: &[String], i: &mut usize, inline: Option<String>, key: &str) -> Result<String, String> {
    if let Some(v) = inline {
        return Ok(v);
    }
    *i += 1;
    args.get(*i).cloned().ok_or(format!("{} expects a value", key))
}

fn parse_format(v: &str) -> Result<Format, String> {
    match v {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        "jsonl" => Ok(Format::Jsonl),
        "junit" => Ok(Format::Junit),
        other => Err(format!("unknown format '{}' (text, json, jsonl, junit)", other)),
    }
}

fn parse_levels(v: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for l in v.split(',').filter(|l| !l.is_empty()) {
        if !ALL_LEVELS.contains(&l) {
            return Err(format!("unknown level '{}' ({})", l, ALL_LEVELS.join(", ")));
        }
        out.push(l.to_string());
    }
    if out.is_empty() {
        return Err("--levels is empty".to_string());
    }
    Ok(out)
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut root = PathBuf::from(".");
    let mut compiler: Option<PathBuf> = None;
    let mut o = Options {
        root: PathBuf::new(),
        compiler: PathBuf::new(),
        format: Format::Text,
        filter: None,
        levels: ALL_LEVELS.iter().map(|l| l.to_string()).collect(),
        out: None,
        jobs: 4,
        timeout_secs: 120,
        retries: 0,
        progress: false,
        quiet: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        let (key, inline) = match a.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        match key.as_str() {
            "-h" | "--help" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            "--quiet" => o.quiet = true,
            "--progress" => o.progress = true,
            "--root" => root = PathBuf::from(take_value(args, &mut i, inline, &key)?),
            "--compiler" => compiler = Some(PathBuf::from(take_value(args, &mut i, inline, &key)?)),
            "--filter" => o.filter = Some(take_value(args, &mut i, inline, &key)?),
            "--out" => o.out = Some(PathBuf::from(take_value(args, &mut i, inline, &key)?)),
            "--format" => o.format = parse_format(&take_value(args, &mut i, inline, &key)?)?,
            "--levels" => o.levels = parse_levels(&take_value(args, &mut i, inline, &key)?)?,
            "--jobs" => {
                let v = take_value(args, &mut i, inline, &key)?;
                o.jobs = v.parse::<usize>().ok().filter(|n| *n > 0).ok_or(format!("--jobs '{}' is no positive number", v))?;
            }
            "--timeout" => {
                let v = take_value(args, &mut i, inline, &key)?;
                o.timeout_secs = v.parse::<u64>().ok().filter(|n| *n > 0).ok_or(format!("--timeout '{}' is no positive number", v))?;
            }
            "--retries" => {
                let v = take_value(args, &mut i, inline, &key)?;
                o.retries = v.parse::<usize>().map_err(|_| format!("--retries '{}' is no number", v))?;
            }
            _ => return Err(format!("unknown option '{}'", a)),
        }
        i += 1;
    }
    o.compiler = compiler.unwrap_or_else(|| root.join("compiler/target/release/firnc"));
    o.root = root;
    Ok(o)
}
