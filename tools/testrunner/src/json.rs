use std::collections::HashMap;
use std::fmt::Write as _;

pub enum Value {
    Str(String),
    Num(i64),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

struct Cursor {
    chars: Vec<char>,
    pos: usize,
}

impl Cursor {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, want: char) -> Option<()> {
        self.skip_space();
        if self.peek() == Some(want) {
            self.pos += 1;
            Some(())
        } else {
            None
        }
    }

    fn string(&mut self) -> Option<String> {
        self.eat('"')?;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            self.pos += 1;
            match c {
                '"' => return Some(out),
                '\\' => {
                    let e = self.peek()?;
                    self.pos += 1;
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'u' => {
                            let hex: String = self.chars.get(self.pos..self.pos + 4)?.iter().collect();
                            self.pos += 4;
                            out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or('?'));
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
    }

    fn scalar(&mut self) -> Option<Value> {
        self.skip_space();
        if self.peek() == Some('"') {
            return self.string().map(Value::Str);
        }
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c != ',' && c != '}') {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let text = text.trim();
        let whole = text.split('.').next().unwrap_or("");
        Some(whole.parse::<i64>().map(Value::Num).unwrap_or(Value::Str(text.to_string())))
    }
}

pub fn parse_flat(line: &str) -> Option<HashMap<String, Value>> {
    let mut cur = Cursor { chars: line.trim().trim_end_matches(',').chars().collect(), pos: 0 };
    cur.eat('{')?;
    let mut map = HashMap::new();
    loop {
        cur.skip_space();
        if cur.peek() == Some('}') {
            return Some(map);
        }
        let key = cur.string()?;
        cur.eat(':')?;
        map.insert(key, cur.scalar()?);
        cur.skip_space();
        if cur.peek() == Some(',') {
            cur.pos += 1;
        }
    }
}
