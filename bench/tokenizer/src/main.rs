//! Benchmark bar for the HTML5 tokenizer from `lib/html/` (in Firn).
//!
//! SEPARATE FROM THE COMPILER: this directory is its own Cargo project and
//! never a dependency of `compiler/`. Its only purpose is to tokenize the same
//! input corpus with `html5ever` and to measure the time.
//!
//!     cargo build --release --manifest-path bench/tokenizer/Cargo.toml
//!     bench/tokenizer/target/release/html5ever_bench .tokenizer-work/korpus.html
//!
//! Output: one line `tokens=<n> bytes=<n> sekunden=<x.xxx>`.
//! `tools/tokenizer/throughput.sh` calls the binary automatically as soon as it
//! is built and puts its MB/s next to those of the Firn tokenizer.
//!
//! Comparability — honestly stated:
//!   * Both sides read the SAME corpus (.tokenizer-work/korpus.html).
//!   * Both sides run the full state machine incl. character references.
//!   * html5ever gets the text as `StrTendril` (UTF-8), the Firn tokenizer
//!     decodes WTF-8 itself into code points -- this step is part of
//!     its measured time.
//!   * The Firn driver additionally writes html5lib JSON to the output;
//!     this sink only counts the tokens here. The measured Firn time therefore contains
//!     work that html5ever does not do -- the reported factor is
//!     computed more BAD than favourable for Firn.

use std::time::Instant;

use html5ever::tendril::{ByteTendril, ReadExt, StrTendril};
use html5ever::tokenizer::{
    BufferQueue, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

/// Counts tokens -- the counterpart of the Firn tokenizer's output sink.
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
