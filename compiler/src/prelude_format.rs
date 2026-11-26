// SPDX-License-Identifier: MPL-2.0
use crate::prelude::{skip_block, skip_line, skip_quoted};

pub const MAX_ARGS: usize = 8;
pub const SHOW_WRAPPER: &str = "fn __fmt_show[T](x: T) -> str { return io.fmt_string(f\"{x}\") }";
const CALLEE: &str = "format";

pub(crate) struct Call {
    pub(crate) close: usize,
    pub(crate) args: Vec<(usize, usize)>,
}

fn open_paren(b: &[u8], from: usize) -> Option<usize> {
    let mut i = from + CALLEE.len();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    (b.get(i) == Some(&b'(')).then_some(i)
}

pub(crate) fn split_args(b: &[u8], open: usize) -> Option<Call> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = open + 1;
    let mut i = start;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i + 2),
            b'"' | b'\'' => i = skip_quoted(b, i, b[i]),
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' if depth == 0 => {
                let rest = &b[start..i];
                if !rest.iter().all(|c| c.is_ascii_whitespace()) || args.is_empty() {
                    args.push((start, i));
                }
                return Some(Call { close: i, args });
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b',' if depth == 0 => {
                args.push((start, i));
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    None
}

fn emit(src: &str, from: usize, to: usize, calls: &[usize], out: &mut String) {
    let b = src.as_bytes();
    let mut pos = from;
    for &p in calls {
        if p < pos {
            continue;
        }
        if p >= to {
            break;
        }
        let Some(call) = open_paren(b, p).and_then(|o| split_args(b, o)) else {
            continue;
        };
        if call.args.is_empty() || call.args.len() > MAX_ARGS + 1 || call.close >= to {
            continue;
        }
        out.push_str(&src[pos..p]);
        out.push_str(&format!("format.format{}(", call.args.len() - 1));
        for (k, &(s, e)) in call.args.iter().enumerate() {
            if k > 0 {
                out.push(',');
                out.push_str("__fmt_show(");
            }
            emit(src, s, e, calls, out);
            if k > 0 {
                out.push(')');
            }
        }
        out.push(')');
        pos = call.close + 1;
    }
    out.push_str(&src[pos..to]);
}

pub fn rewrite(src: &str, calls: &[usize]) -> String {
    let mut out = String::with_capacity(src.len() + calls.len() * 48);
    emit(src, 0, src.len(), calls, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_arguments_and_nests() {
        let src = "let s = format(\"{0}{1}\", a, format(\"{0}\", b + 1))";
        let calls = vec![8, 28];
        assert_eq!(
            rewrite(src, &calls),
            "let s = format.format2(\"{0}{1}\",__fmt_show( a),__fmt_show( format.format1(\"{0}\",__fmt_show( b + 1))))"
        );
    }

    #[test]
    fn too_many_arguments_stay() {
        let src = "format(\"x\", 1, 2, 3, 4, 5, 6, 7, 8, 9)";
        assert_eq!(rewrite(src, &[0]), src);
    }
}
