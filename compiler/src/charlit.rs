const MAX_SCALAR: u32 = 0x10FFFF;
const SURROGATE_LOW: u32 = 0xD800;
const SURROGATE_HIGH: u32 = 0xDFFF;

fn simple_escape(e: char) -> Option<u32> {
    match e {
        'n' => Some(0x0A),
        'r' => Some(0x0D),
        't' => Some(0x09),
        '0' => Some(0x00),
        '\\' => Some(0x5C),
        '"' => Some(0x22),
        '\'' => Some(0x27),
        _ => None,
    }
}

fn fixed_unicode_escape(src: &[char], at: usize) -> Option<(u32, usize)> {
    let mut value: u32 = 0;
    for k in 0..4 {
        value = value * 16 + src.get(at + k)?.to_digit(16)?;
    }
    if (SURROGATE_LOW..=SURROGATE_HIGH).contains(&value) {
        return None;
    }
    Some((value, 4))
}

fn unicode_escape(src: &[char], at: usize) -> Option<(u32, usize)> {
    if src.get(at) != Some(&'{') {
        return fixed_unicode_escape(src, at);
    }
    let mut value: u32 = 0;
    let mut digits = 0;
    let mut i = at + 1;
    loop {
        let c = *src.get(i)?;
        i += 1;
        if c == '}' {
            break;
        }
        value = value * 16 + c.to_digit(16)?;
        digits += 1;
        if digits > 6 || value > MAX_SCALAR {
            return None;
        }
    }
    if digits == 0 || (SURROGATE_LOW..=SURROGATE_HIGH).contains(&value) {
        return None;
    }
    Some((value, i - at))
}

fn body_char(src: &[char], at: usize) -> Option<(u32, usize)> {
    let c = *src.get(at)?;
    if c == '\'' || c == '\n' {
        return None;
    }
    if c != '\\' {
        return Some((c as u32, 1));
    }
    let e = *src.get(at + 1)?;
    if e == 'u' {
        let (value, used) = unicode_escape(src, at + 2)?;
        return Some((value, 2 + used));
    }
    Some((simple_escape(e)?, 2))
}

pub fn lex_char_literal(src: &[char], pos: usize) -> Option<(u32, usize)> {
    if src.get(pos) != Some(&'\'') {
        return None;
    }
    let (value, used) = body_char(src, pos + 1)?;
    if src.get(pos + 1 + used) != Some(&'\'') {
        return None;
    }
    Some((value, used + 2))
}
