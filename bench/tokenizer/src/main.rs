// SPDX-License-Identifier: MIT
//! Messlatte fuer den HTML5-Tokenizer aus `lib/html/` (in Firn).
//!
//! GETRENNT VOM COMPILER: dieses Verzeichnis ist ein eigenes Cargo-Projekt und
//! niemals eine Abhaengigkeit von `compiler/`. Es dient allein dazu, denselben
//! Eingabekorpus mit `html5ever` zu tokenisieren und die Zeit zu messen.
//!
//!     cargo build --release --manifest-path bench/tokenizer/Cargo.toml
//!     bench/tokenizer/target/release/html5ever_bench .tokenizer-work/korpus.html
//!
//! Ausgabe: eine Zeile `tokens=<n> bytes=<n> sekunden=<x.xxx>`.
//! `tools/tokenizer/throughput.sh` ruft das Binary automatisch auf, sobald es
//! gebaut ist, und stellt die MB/s neben die des Firn-Tokenizers.
//!
//! Vergleichbarkeit — ehrlich benannt:
//!   * Beide Seiten lesen DENSELBEN Korpus (.tokenizer-work/korpus.html).
//!   * Beide Seiten laufen die volle Zustandsmaschine inkl. Zeichenreferenzen.
//!   * html5ever bekommt den Text als `StrTendril` (UTF-8), der Firn-Tokenizer
//!     dekodiert WTF-8 selbst nach Codepunkten — dieser Schritt gehoert bei
//!     ihm zur gemessenen Zeit.
//!   * Der Firn-Treiber schreibt zusaetzlich html5lib-JSON auf die Ausgabe;
//!     diese Senke zaehlt hier nur die Token. Die gemessene Firn-Zeit enthaelt
//!     also Arbeit, die html5ever nicht leistet — der ausgewiesene Faktor ist
//!     fuer Firn eher zu SCHLECHT als zu guenstig gerechnet.

use std::time::Instant;

use html5ever::tendril::{ByteTendril, ReadExt, StrTendril};
use html5ever::tokenizer::{
    BufferQueue, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

/// Zaehlt Token — das Gegenstueck zur Ausgabesenke des Firn-Tokenizers.
struct Counter {
    tokens: u64,
    chars: u64,
}

impl TokenSink for Counter {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line: u64) -> TokenSinkResult<()> {
        self.tokens += 1;
        if let Token::CharacterTokens(ref s) = token {
            self.chars += s.len() as u64;
        }
        TokenSinkResult::Continue
    }
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("aufruf: html5ever_bench <datei.html>");
            std::process::exit(2);
        }
    };

    let mut raw = ByteTendril::new();
    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {}", path, e);
            std::process::exit(2);
        }
    };
    if let Err(e) = file.read_to_tendril(&mut raw) {
        eprintln!("{}: {}", path, e);
        std::process::exit(2);
    }
    let bytes = raw.len();
    let text: StrTendril = match raw.try_reinterpret() {
        Ok(t) => t,
        Err(_) => {
            eprintln!("{}: kein gueltiges UTF-8", path);
            std::process::exit(2);
        }
    };

    let start = Instant::now();
    let sink = Counter {
        tokens: 0,
        chars: 0,
    };
    let mut tok = Tokenizer::new(sink, TokenizerOpts::default());
    let mut queue = BufferQueue::default();
    queue.push_back(text);
    let _ = tok.feed(&mut queue);
    tok.end();
    let duration = start.elapsed();

    println!(
        "tokens={} zeichen={} bytes={} sekunden={:.6}",
        tok.sink.tokens,
        tok.sink.chars,
        bytes,
        duration.as_secs_f64()
    );
}
