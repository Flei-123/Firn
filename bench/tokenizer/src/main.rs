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
//! `tools/tokenizer/durchsatz.sh` ruft das Binary automatisch auf, sobald es
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
struct Zaehler {
    tokens: u64,
    zeichen: u64,
}

impl TokenSink for Zaehler {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line: u64) -> TokenSinkResult<()> {
        self.tokens += 1;
        if let Token::CharacterTokens(ref s) = token {
            self.zeichen += s.len() as u64;
        }
        TokenSinkResult::Continue
    }
}

fn main() {
    let pfad = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("aufruf: html5ever_bench <datei.html>");
            std::process::exit(2);
        }
    };

    let mut roh = ByteTendril::new();
    let mut datei = match std::fs::File::open(&pfad) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {}", pfad, e);
            std::process::exit(2);
        }
    };
    if let Err(e) = datei.read_to_tendril(&mut roh) {
        eprintln!("{}: {}", pfad, e);
        std::process::exit(2);
    }
    let bytes = roh.len();
    let text: StrTendril = match roh.try_reinterpret() {
        Ok(t) => t,
        Err(_) => {
            eprintln!("{}: kein gueltiges UTF-8", pfad);
            std::process::exit(2);
        }
    };

    let start = Instant::now();
    let sink = Zaehler {
        tokens: 0,
        zeichen: 0,
    };
    let mut tok = Tokenizer::new(sink, TokenizerOpts::default());
    let mut queue = BufferQueue::default();
    queue.push_back(text);
    let _ = tok.feed(&mut queue);
    tok.end();
    let dauer = start.elapsed();

    println!(
        "tokens={} zeichen={} bytes={} sekunden={:.6}",
        tok.sink.tokens,
        tok.sink.zeichen,
        bytes,
        dauer.as_secs_f64()
    );
}
