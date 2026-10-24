use crate::cli::{Format, Options};
use crate::model::{Case, Status, Summary};
use crate::{report_json, report_junit};

fn text(cases: &[Case], o: &Options) -> String {
    let mut s = String::new();
    for c in cases {
        match c.status {
            Status::Pass if !o.quiet => s.push_str(&format!("PASS  {} [{}]\n", c.name, c.level)),
            Status::Skip if !o.quiet => s.push_str(&format!("SKIP  {} [{}]: {}\n", c.name, c.level, c.message)),
            Status::Fail => s.push_str(&format!("FAIL  {} [{}] {}:{}:{}: {}\n", c.name, c.level, c.file, c.line, c.col, c.message)),
            _ => {}
        }
    }
    let sum = Summary::of(cases);
    s.push_str(&format!(
        "{} of {} passed ({:.1} %), {} failed, {} skipped\n",
        sum.passed,
        sum.total(),
        sum.rate() * 100.0,
        sum.failed,
        sum.skipped
    ));
    s
}

pub fn render(cases: &[Case], o: &Options) -> String {
    match o.format {
        Format::Text => text(cases, o),
        Format::Json => report_json::whole(cases),
        Format::Jsonl => report_json::lines(cases),
        Format::Junit => report_junit::render(cases),
    }
}
