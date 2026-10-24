use crate::model::{Case, Status, Summary};

fn xml(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn seconds(micros: u128) -> String {
    format!("{:.3}", micros as f64 / 1_000_000.0)
}

fn classname(c: &Case) -> String {
    let dir = c.file.rsplit_once('/').map(|x| x.0).unwrap_or("");
    dir.replace('/', ".")
}

fn testcase(c: &Case) -> String {
    let head = format!(
        "    <testcase classname=\"{}\" name=\"{}\" file=\"{}\" line=\"{}\" time=\"{}\"",
        xml(&classname(c)),
        xml(&c.name),
        xml(&c.file),
        c.line,
        seconds(c.micros)
    );
    match c.status {
        Status::Pass => format!("{}/>\n", head),
        Status::Skip => format!("{}>\n      <skipped message=\"{}\"/>\n    </testcase>\n", head, xml(&c.message)),
        Status::Fail => format!(
            "{}>\n      <failure message=\"{}\" type=\"{}\">{}:{}:{}: {}</failure>\n    </testcase>\n",
            head,
            xml(&c.message),
            c.kind,
            xml(&c.file),
            c.line,
            c.col,
            xml(&c.message)
        ),
    }
}

fn levels(cases: &[Case]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for c in cases {
        if !seen.contains(&c.level) {
            seen.push(c.level.clone());
        }
    }
    seen
}

pub fn render(cases: &[Case]) -> String {
    let sum = Summary::of(cases);
    let total_time: u128 = cases.iter().map(|c| c.micros).sum();
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!(
        "<testsuites name=\"firn\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" time=\"{}\">\n",
        cases.len(),
        sum.failed,
        sum.skipped,
        seconds(total_time)
    ));
    for level in levels(cases) {
        let group: Vec<&Case> = cases.iter().filter(|c| c.level == level).collect();
        let failures = group.iter().filter(|c| c.status == Status::Fail).count();
        let skipped = group.iter().filter(|c| c.status == Status::Skip).count();
        let time: u128 = group.iter().map(|c| c.micros).sum();
        s.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" time=\"{}\">\n",
            xml(&level),
            group.len(),
            failures,
            skipped,
            seconds(time)
        ));
        for c in group {
            s.push_str(&testcase(c));
        }
        s.push_str("  </testsuite>\n");
    }
    s.push_str("</testsuites>\n");
    s
}
