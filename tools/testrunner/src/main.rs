mod check_neg;
mod check_run;
mod check_unit;
mod cli;
mod discover;
mod exec;
mod json;
mod location;
mod model;
mod report;
mod report_json;
mod report_junit;

use cli::Options;
use discover::Job;
use model::{Case, Status, Summary};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

fn execute_once(o: &Options, work: &Path, job: &Job) -> Vec<Case> {
    match job {
        Job::Run { rel, levels } => levels.iter().map(|l| check_run::run_program(o, work, rel, l)).collect(),
        Job::Negative { rel } => vec![check_neg::run_negative(o, work, rel)],
        Job::Unit { rel } => check_unit::run_unit(o, work, rel),
        Job::Skip { rel, level, reason } => vec![Case::new(rel, level, "run").skipped(reason)],
    }
}

fn execute(o: &Options, work: &Path, job: &Job) -> Vec<Case> {
    let mut cases = execute_once(o, work, job);
    for attempt in 1..=o.retries {
        if !cases.iter().any(|c| c.status == Status::Fail) {
            break;
        }
        let again = execute_once(o, work, job);
        if o.progress {
            eprintln!("RETRY {} of {}", attempt, o.retries);
        }
        cases = again;
    }
    cases
}

fn run_all(o: &Options, work: &Path, jobs: &[Job]) -> Vec<Case> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Vec<Case>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..o.jobs.min(jobs.len()).max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= jobs.len() {
                    return;
                }
                let cases = execute(o, work, &jobs[i]);
                if o.progress {
                    for c in &cases {
                        eprintln!("{}  {} [{}]", c.status.name().to_uppercase(), c.name, c.level);
                    }
                }
                done.lock().unwrap().push((i, cases));
            });
        }
    });
    let mut all = done.into_inner().unwrap();
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().flat_map(|(_, c)| c).collect()
}

fn fail_usage(msg: &str) -> ! {
    eprintln!("error: {}", msg);
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = match cli::parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprint!("error: {}\n{}", e, cli::usage());
            std::process::exit(2);
        }
    };
    opts.compiler = std::fs::canonicalize(&opts.compiler)
        .unwrap_or_else(|_| fail_usage(&format!("compiler '{}' not found (run 'cargo build --release' first)", opts.compiler.display())));
    let jobs = discover::collect(&opts);
    if jobs.iter().all(|j| matches!(j, Job::Skip { .. })) {
        fail_usage("no test cases selected");
    }
    let work: PathBuf = std::env::temp_dir().join(format!("firn-testrunner-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&work) {
        fail_usage(&format!("cannot create '{}': {}", work.display(), e));
    }
    let cases = run_all(&opts, &work, &jobs);
    let _ = std::fs::remove_dir_all(&work);
    let report = report::render(&cases, &opts);
    match &opts.out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &report) {
                fail_usage(&format!("cannot write '{}': {}", path.display(), e));
            }
        }
        None => print!("{}", report),
    }
    let failed = cases.iter().any(|c| c.status == Status::Fail);
    let sum = Summary::of(&cases);
    if opts.out.is_some() && !opts.quiet {
        eprintln!("{} of {} passed, {} failed, {} skipped", sum.passed, sum.total(), sum.failed, sum.skipped);
    }
    std::process::exit(if failed { 1 } else { 0 });
}
