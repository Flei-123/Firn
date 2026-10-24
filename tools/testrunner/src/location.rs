fn parse_number(s: &str) -> Option<(u32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s[..end].parse().ok()?, &s[end..]))
}

fn scan_line(line: &str, out: &mut Vec<(String, u32, u32)>) {
    for (idx, _) in line.match_indices(".fi:") {
        let head = &line[..idx + 3];
        let start = head
            .rfind(|c: char| c == ' ' || c == '\t' || c == '"' || c == '\'' || c == '(' || c == '`' || c == '[')
            .map(|p| p + 1)
            .unwrap_or(0);
        let rest = &line[idx + 4..];
        if let Some((l, tail)) = parse_number(rest) {
            let col = tail.strip_prefix(':').and_then(parse_number).map(|(c, _)| c).unwrap_or(1);
            out.push((head[start..].to_string(), l, col));
        }
    }
}

pub fn scan(text: &str) -> Vec<(String, u32, u32)> {
    let mut out = Vec::new();
    for line in text.lines() {
        scan_line(line, &mut out);
    }
    out
}

pub fn first(text: &str) -> Option<(String, u32, u32)> {
    scan(text).into_iter().next()
}

pub fn last(text: &str) -> Option<(String, u32, u32)> {
    scan(text).into_iter().last()
}
