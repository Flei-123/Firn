// SPDX-License-Identifier: GPL-2.0-only
//! Spruenge und RIP-relative Adressen gegen GNU `as` pruefen.
//!
//! Diese Faelle lassen sich nicht als einzelner Befehl pruefen, weil ihre
//! Oktette vom Abstand zum Ziel abhaengen. Die Wahrheitsdatei enthaelt darum
//! den gemessenen Abstand `rel` (ab dem ENDE des Befehls) und die Oktette,
//! die `as` dafuer geschrieben hat -- samt der Entscheidung rel8 oder rel32.

use std::collections::BTreeMap;
use std::env;
use std::fs;

#[path = "../encode_x86.rs"]
mod encode_x86;
use encode_x86::*;

struct Sprung {
    mnem: String,
    rel: i64,
    hex: String,
    laenge: usize,
}

struct Rip {
    rel: i64,
    hex: String,
}

/// Winziger Leser fuer die flachen Objekte aus sprung_wahrheit.py.
fn lesen(pfad: &str) -> (Vec<Sprung>, Vec<Rip>) {
    let roh = fs::read_to_string(pfad).expect("Wahrheitsdatei fehlt");
    let mut spr = Vec::new();
    let mut rip = Vec::new();

    let teil_spr = roh.find("\"spruenge\"").map(|i| &roh[i..]).unwrap_or("");
    let ende_spr = teil_spr.find("\"rip\"").unwrap_or(teil_spr.len());
    let teil_rip = roh.find("\"rip\"").map(|i| &roh[i..]).unwrap_or("");

    let objekte = |s: &str| -> Vec<String> {
        let mut out = Vec::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] != b'{' { i += 1; continue; }
            let start = i;
            let mut t = 0;
            while i < b.len() {
                if b[i] == b'{' { t += 1; }
                else if b[i] == b'}' { t -= 1; if t == 0 { i += 1; break; } }
                i += 1;
            }
            out.push(s[start..i].to_string());
        }
        out
    };
    let str_feld = |o: &str, k: &str| -> Option<String> {
        let m = format!("\"{}\":", k);
        let p = o.find(&m)? + m.len();
        let r = o[p..].trim_start();
        let r = r.strip_prefix('"')?;
        let e = r.find('"')?;
        Some(r[..e].to_string())
    };
    let zahl_feld = |o: &str, k: &str| -> Option<i64> {
        let m = format!("\"{}\":", k);
        let p = o.find(&m)? + m.len();
        let r = o[p..].trim_start();
        let e = r.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(r.len());
        r[..e].parse::<i64>().ok()
    };

    for o in objekte(&teil_spr[..ende_spr]) {
        if let (Some(m), Some(r), Some(h), Some(l)) =
            (str_feld(&o, "mnem"), zahl_feld(&o, "rel"),
             str_feld(&o, "hex"), zahl_feld(&o, "laenge")) {
            spr.push(Sprung { mnem: m, rel: r, hex: h, laenge: l as usize });
        }
    }
    for o in objekte(teil_rip) {
        if let (Some(r), Some(h)) = (zahl_feld(&o, "rel"), str_feld(&o, "hex")) {
            rip.push(Rip { rel: r, hex: h });
        }
    }
    (spr, rip)
}

/// Einen Sprung so kodieren, wie GNU as es tut: kuerzeste Form, die passt.
///
/// `rel` ist der Abstand ab dem ENDE des Befehls. Weil die Befehlslaenge
/// selbst davon abhaengt, wird hier mit der Laenge der JEWEILIGEN Form
/// gerechnet -- das ist genau die Fixpunktfrage, die ein Assembler in zwei
/// Durchlaeufen loest.
fn sprung_kodieren(mnem: &str, rel: i64) -> Option<Vec<u8>> {
    let mut p = Puffer::neu();
    if mnem == "call" {
        // Es gibt keine kurze Form.
        X86::call_rel32(&mut p, rel as i32);
        return Some(p.okt);
    }
    if mnem == "jmp" {
        if rel >= -128 && rel <= 127 {
            X86::jmp_rel8(&mut p, rel as i8);
        } else {
            X86::jmp_rel32(&mut p, rel as i32);
        }
        return Some(p.okt);
    }
    let cc = X86::cc_nr(mnem.strip_prefix('j')?)?;
    if rel >= -128 && rel <= 127 {
        X86::jcc_rel8(&mut p, cc, rel as i8);
    } else {
        X86::jcc_rel32(&mut p, cc, rel as i32);
    }
    Some(p.okt)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let pfad = args.get(1).cloned()
        .unwrap_or_else(|| "tools/asmstudie/wahrheit_sprung.json".to_string());
    let (spr, rip) = lesen(&pfad);

    let mut gleich = 0usize;
    let mut falsch = 0usize;
    let mut offen = 0usize;
    let mut je_mnem: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut beispiele = Vec::new();

    for s in &spr {
        let e = je_mnem.entry(s.mnem.clone()).or_insert((0, 0));
        match sprung_kodieren(&s.mnem, s.rel) {
            None => offen += 1,
            Some(okt) => {
                let ist: String = okt.iter().map(|x| format!("{:02x}", x)).collect();
                if ist == s.hex && okt.len() == s.laenge {
                    gleich += 1; e.0 += 1;
                } else {
                    falsch += 1; e.1 += 1;
                    if beispiele.len() < 20 {
                        beispiele.push((s.mnem.clone(), s.rel, s.hex.clone(), ist));
                    }
                }
            }
        }
    }

    // RIP-relativ: lea rax, [rip+ZIEL]
    let mut rip_gleich = 0usize;
    let mut rip_falsch = 0usize;
    for r in &rip {
        let mut p = Puffer::neu();
        X86::lea(&mut p, Breite::B64, Reg::Rax, &Mem::rip(r.rel as i32));
        let ist = p.hex();
        if ist == r.hex { rip_gleich += 1; } else {
            rip_falsch += 1;
            beispiele.push(("lea rip".into(), r.rel, r.hex.clone(), ist));
        }
    }

    println!("=====================================================================");
    println!("SPRUNGWEITEN und RIP  --  {}", pfad);
    println!("=====================================================================");
    println!("  Spruenge byteweise GLEICH : {}", gleich);
    println!("  Spruenge FALSCH           : {}", falsch);
    println!("  Spruenge nicht abgedeckt  : {}", offen);
    println!("  RIP GLEICH / FALSCH       : {} / {}", rip_gleich, rip_falsch);
    let ges = gleich + falsch + offen;
    if ges > 0 {
        println!("  Quote                     : {:.1} % von {}",
                 100.0 * gleich as f64 / ges as f64, ges);
    }
    println!("\n--- je Befehl ---");
    for (k, v) in &je_mnem {
        println!("  {:6} gleich {:4}  falsch {:4}", k, v.0, v.1);
    }
    if !beispiele.is_empty() {
        println!("\n--- Abweichungen ---");
        for (m, rel, soll, ist) in &beispiele {
            println!("  {:6} rel {:8}\n      soll {}\n      ist  {}", m, rel, soll, ist);
        }
    }
}
