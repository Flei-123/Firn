use crate::json::escape;
use crate::model::{Case, Summary};

fn case_object(c: &Case) -> String {
    format!(
        "{{\"name\":\"{}\",\"mode\":\"{}\",\"level\":\"{}\",\"kind\":\"{}\",\"status\":\"{}\",\"message\":\"{}\",\"duration_ms\":{},\"duration_us\":{},\"file\":\"{}\",\"line\":{},\"col\":{}}}",
        escape(&c.name),
        escape(&c.level),
        escape(&c.level),
        c.kind,
        c.status.name(),
        escape(&c.message),
        c.millis(),
        c.micros,
        escape(&c.file),
        c.line,
        c.col
    )
}

fn summary_fields(sum: &Summary) -> String {
    format!(
        "\"total\":{},\"passed\":{},\"failed\":{},\"skipped\":{},\"rate\":{:.4}",
        sum.total(),
        sum.passed,
        sum.failed,
        sum.skipped,
        sum.rate()
    )
}

pub fn whole(cases: &[Case]) -> String {
    let objects: Vec<String> = cases.iter().map(case_object).collect();
    format!(
        "{{\"suite\":\"firn\",{},\"cases\":[{}]}}\n",
        summary_fields(&Summary::of(cases)),
        objects.join(",")
    )
}

pub fn lines(cases: &[Case]) -> String {
    let mut s = String::new();
    for c in cases {
        s.push_str(&case_object(c));
        s.push('\n');
    }
    s.push_str(&format!("{{\"summary\":{{\"suite\":\"firn\",{}}}}}\n", summary_fields(&Summary::of(cases))));
    s
}
