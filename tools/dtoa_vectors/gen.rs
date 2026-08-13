//! Testvektoren fuer lib/num/dtoa.fi und lib/num/strtod.fi (Modul str).
//!
//! Dieses Werkzeug ist die WERKBANK, nicht das Produkt: es erzeugt zufaellige
//! Doubles und prueft, was das Firn-Programm daraus gemacht hat. Der Compiler
//! selbst benutzt es nicht, es haengt an keiner Bibliothek ausser `std`.
//!
//!   gen bits <N> <seed>     N zufaellige, endliche Doubles als 8-Byte-Muster
//!                           (little endian) auf die Standardausgabe
//!   gen check <N> <seed>    liest N Textzeilen (die Ausgabe des
//!                           Firn-Programms) und vergleicht sie mit der
//!                           kuerzesten Darstellung von Rust
//!
//! Uebersetzen: rustc -O -o gen gen.rs

use std::io::{Read, Write};

/// xorshift64* — reproduzierbar, ohne fremde Bibliothek.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
}

/// Erzeugt die N Bitmuster der Reihe nach (dieselbe Folge in beiden Betriebsarten).
fn vectors(n: usize, seed: u64) -> Vec<u64> {
    // Feste Haertefaelle zuerst, danach Zufall ueber den gesamten Wertebereich.
    let mut out: Vec<u64> = vec![
        0x3FB999999999999A, // 0.1
        0x44B52D02C7E14AF6, // 1e23
        0x0000000000000001, // 5e-324 (kleinstes Denormal)
        0x000FFFFFFFFFFFFF, // groesstes Denormal
        0x0010000000000000, // kleinstes Normal
        0x7FEFFFFFFFFFFFFF, // groesster Double
        0x4340000000000000, // 9007199254740992
        0x3FF0000000000000, // 1
        0xBFF0000000000000, // -1
        0x4024000000000000, // 10
        0x0000000000000002,
        0x0008000000000000,
    ];
    let mut rng = Rng(seed | 1);
    while out.len() < n {
        let bits = rng.next();
        let exp = (bits >> 52) & 0x7FF;
        if exp == 0x7FF {
            continue; // NaN/Inf haben keine Zifferndarstellung
        }
        if bits << 1 == 0 {
            continue; // +-0 ist gesondert geprueft (305_dtoa_hardcases)
        }
        out.push(bits);
    }
    out.truncate(n);
    out
}

/// Kuerzeste Darstellung nach ECMAScript `Number::toString`.
fn ecma(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity".into() } else { "Infinity".into() };
    }
    if x == 0.0 {
        return "0".to_string();
    }
    let neg = x.is_sign_negative();
    // {:e} liefert die kuerzeste Darstellung mit Rueckwandlungsgarantie.
    let s = format!("{:e}", x.abs());
    let (mant, exp) = s.split_once('e').expect("{:e} hat immer ein e");
    let exp: i64 = exp.parse().expect("exponent ist eine zahl");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let k = digits.len() as i64;
    let n = exp + 1; // wert = 0.digits * 10^n
    let body = if k <= n && n <= 21 {
        format!("{}{}", digits, "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else if k == 1 {
        format!("{}e{}{}", digits, if n - 1 >= 0 { "+" } else { "-" }, (n - 1).abs())
    } else {
        format!(
            "{}.{}e{}{}",
            &digits[..1],
            &digits[1..],
            if n - 1 >= 0 { "+" } else { "-" },
            (n - 1).abs()
        )
    };
    if neg {
        format!("-{}", body)
    } else {
        body
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("Aufruf: gen bits|check <N> <seed>");
        std::process::exit(2);
    }
    let n: usize = args[2].parse().unwrap_or(0);
    let seed: u64 = args[3].parse().unwrap_or(1);
    if n == 0 {
        eprintln!("N muss > 0 sein");
        std::process::exit(2);
    }
    let v = vectors(n, seed);
    match args[1].as_str() {
        "bits" => {
            let mut buf = Vec::with_capacity(n * 8);
            for b in &v {
                buf.extend_from_slice(&b.to_le_bytes());
            }
            std::io::stdout().write_all(&buf).expect("schreiben");
        }
        "check" => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).expect("lesen");
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() != n {
                eprintln!("erwartet {} zeilen, bekommen {}", n, lines.len());
                std::process::exit(1);
            }
            let mut bad = 0usize;
            let mut roundtrip_bad = 0usize;
            for (i, (bits, line)) in v.iter().zip(lines.iter()).enumerate() {
                let x = f64::from_bits(*bits);
                let want = ecma(x);
                if *line != want {
                    bad += 1;
                    if bad <= 10 {
                        eprintln!("  #{} bits={:#018x}: firn '{}' != rust '{}'", i, bits, line, want);
                    }
                }
                // Rueckwandlung des FIRN-Textes mit Rusts strtod
                match line.parse::<f64>() {
                    Ok(back) if back.to_bits() == *bits => {}
                    _ => {
                        roundtrip_bad += 1;
                        if roundtrip_bad <= 10 {
                            eprintln!("  #{} bits={:#018x}: '{}' faellt nicht zurueck", i, bits, line);
                        }
                    }
                }
            }
            println!("geprueft: {}", n);
            println!("kuerzeste darstellung gleich rust: {}", n - bad);
            println!("rueckwandlung f64->text->f64 bitgleich: {}", n - roundtrip_bad);
            if bad != 0 || roundtrip_bad != 0 {
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("unbekannte betriebsart '{}'", other);
            std::process::exit(2);
        }
    }
}
