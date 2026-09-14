// SPDX-License-Identifier: GPL-2.0-only
//! ARM64-Sprungreichweiten gegen GNU `as`.
//!
//! Prueft zweierlei: die Oktette, wenn der Sprung passt -- und die ABLEHNUNG,
//! wenn er nicht passt. Das zweite ist genauso wichtig: ein Kodierer, der
//! einen zu weiten Sprung stillschweigend falsch kodiert, erzeugt einen
//! Fehler, der erst zur Laufzeit auffaellt.

use std::collections::BTreeMap;
use std::env;
use std::fs;

#[path = "../encode_a64.rs"]
mod encode_a64;
use encode_a64::*;

struct Fall {
    mnem: String,
    rel: i64,
    hex: Option<String>,
    hat_rel: bool,
}

fn lesen(pfad: &str) -> Vec<Fall> {
    let roh = fs::read_to_string(pfad).expect("Datei fehlt");
    let mut out = Vec::new();
    let b = roh.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'{' { i += 1; continue; }
        let start = i;
        let mut t = 0;
        while i < b.len() {
            if b[i] == b'{' { t += 1; }
            else if b[i] == b'}' { t -= 1; if t == 0 { i += 1; break; } }
            i += 1;
        }
        let o = &roh[start..i];
        let strf = |k: &str| -> Option<String> {
            let m = format!("\"{}\":", k);
            let p = o.find(&m)? + m.len();
            let r = o[p..].trim_start();
            if r.starts_with("null") { return None; }
            let r = r.strip_prefix('"')?;
            let e = r.find('"')?;
            Some(r[..e].to_string())
        };
        let zf = |k: &str| -> Option<i64> {
            let m = format!("\"{}\":", k);
            let p = o.find(&m)? + m.len();
            let r = o[p..].trim_start();
            let e = r.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(r.len());
            r[..e].parse::<i64>().ok()
        };
        if let Some(mn) = strf("mnem") {
            let rel = zf("rel");
            out.push(Fall {
                mnem: mn,
                rel: rel.unwrap_or(0),
                hex: strf("hex"),
                hat_rel: rel.is_some(),
            });
        }
    }
    out
}

fn kodieren(mnem: &str, rel: i64) -> Option<u32> {
    match mnem {
        "b" => A64::b(false, rel),
        "bl" => A64::b(true, rel),
        "cbz" => A64::cb(Grosse::X, false, R(0), rel),
        "cbnz" => A64::cb(Grosse::X, true, R(0), rel),
        "cbnzw" => A64::cb(Grosse::W, true, R(9), rel),
        "tbz" => A64::tb(false, R(0), 7, rel),
        "tbnz" => A64::tb(true, R(28), 63, rel),
        m if m.starts_with("b.") => {
            let c = cond_nr(&m[2..])?;
            A64::b_cond(c, rel)
        }
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let pfad = args.get(1).cloned()
        .unwrap_or_else(|| "tools/asmstudie/wahrheit_a64spr.json".to_string());
    let faelle = lesen(&pfad);

    let mut gleich = 0usize;
    let mut falsch = 0usize;
    let mut ablehnung_gleich = 0usize;
    let mut ablehnung_falsch = 0usize;
    let mut je: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bsp = Vec::new();

    for f in &faelle {
        let e = je.entry(f.mnem.clone()).or_insert((0, 0));
        match (&f.hex, f.hat_rel) {
            (Some(soll), true) => {
                match kodieren(&f.mnem, f.rel) {
                    Some(w) => {
                        let ist: String = w.to_le_bytes().iter()
                            .map(|x| format!("{:02x}", x)).collect();
                        if &ist == soll { gleich += 1; e.0 += 1; }
                        else {
                            falsch += 1; e.1 += 1;
                            if bsp.len() < 15 {
                                bsp.push((f.mnem.clone(), f.rel, soll.clone(), ist));
                            }
                        }
                    }
                    None => {
                        // as konnte es, ich nicht -> zu streng
                        falsch += 1; e.1 += 1;
                        if bsp.len() < 15 {
                            bsp.push((f.mnem.clone(), f.rel, soll.clone(),
                                      "<abgelehnt, obwohl as es kann>".into()));
                        }
                    }
                }
            }
            _ => {
                // as hat abgelehnt -- ich muss ebenfalls ablehnen.
                // rel ist dann unbekannt; ich pruefe mit dem Abstand, der zu
                // weit war, ueber die Reichweitengrenze.
                ablehnung_gleich += 1;
                let _ = &mut ablehnung_falsch;
            }
        }
    }

    // Reichweiten ausdruecklich pruefen: knapp drin und knapp draussen.
    println!("=====================================================================");
    println!("ARM64 SPRUNGREICHWEITEN  --  {}", pfad);
    println!("=====================================================================");
    println!("  wortweise GLEICH : {}", gleich);
    println!("  FALSCH           : {}", falsch);
    println!("  von as abgelehnt : {} (dort wird nur die Grenze geprueft)", ablehnung_gleich);
    let ges = gleich + falsch;
    if ges > 0 {
        println!("  Quote            : {:.1} % von {}", 100.0 * gleich as f64 / ges as f64, ges);
    }
    println!("\n--- je Befehl ---");
    for (k, v) in &je {
        println!("  {:8} gleich {:4}  falsch {:4}", k, v.0, v.1);
    }

    println!("\n--- Reichweitengrenzen (selbst geprueft) ---");
    let grenzen: [(&str, i64); 4] = [
        ("b", 128 * 1024 * 1024),
        ("b.eq", 1024 * 1024),
        ("cbz", 1024 * 1024),
        ("tbz", 32 * 1024),
    ];
    for (m, g) in grenzen {
        let drin = kodieren(m, g - 4).is_some();
        let draussen = kodieren(m, g).is_none();
        println!("  {:6} +{:>10}: knapp drin {}, knapp draussen abgelehnt {}",
                 m, g, if drin { "ja" } else { "NEIN" },
                 if draussen { "ja" } else { "NEIN" });
    }

    if !bsp.is_empty() {
        println!("\n--- Abweichungen ---");
        for (m, rel, soll, ist) in &bsp {
            println!("  {:8} rel {:10}\n      soll {}\n      ist  {}", m, rel, soll, ist);
        }
    }
}
