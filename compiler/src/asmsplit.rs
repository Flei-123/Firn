// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND TEMPO — the assembler on more than one core.**
//!
//! ## What was measured
//!
//! `--timings` over the three real builds of this repository, on an
//! otherwise idle machine with twenty cores:
//!
//! ```text
//!                    firnc1 self-build   Certus (b4_main)   Osum kernel
//!   as + ld               959 ms (32 %)     2,235 ms (39 %)    ~1,163 ms
//! ```
//!
//! and `as` alone, timed outside the compiler, is **all** of it: `ld` costs
//! 10-20 ms. So a third of every build is one external, strictly
//! single-threaded process reading a text file of 9-19 MB.
//!
//! ## What this module does
//!
//! It cuts the assembly text into `n` parts at FUNCTION boundaries, so that
//! `n` copies of `as` can run at the same time and `ld` puts the objects
//! together — `as` and `ld` stay the only tools, exactly as
//! `DESIGN_GOALS.md` demands.
//!
//! ## The one real problem, and how it is solved
//!
//! Everything the code generator writes into `.rodata`/`.data` carries a
//! **local** label: `.Lpanicmsg7`, `.L__gc_typetable`, `.Lstatic_count`.
//! `as` throws local labels away, so a function in part 3 that says
//! `lea rax, [rip + .Lpanicmsg7]` cannot reach a definition sitting in
//! part 7 — `ld` answers with `undefined reference to '.Lpanicmsg7'`.
//! (Measured: the first attempt at this failed on exactly that line.)
//!
//! Three ways out were tried:
//!
//! 1. **`as -L`** (keep local symbols). Does not help: the symbols stay
//!    STB_LOCAL and the linker will not resolve them across objects.
//! 2. **Copy the data block into every part.** Works, but the collector's
//!    state block and every `static` live in `.data` and are WRITABLE —
//!    two copies of a mutable word is not an optimisation, it is a bug.
//! 3. **`.globl` on the label.** `as` keeps an explicitly global `.L`
//!    symbol in the symbol table and `ld` resolves it. Verified on a two
//!    object test case before a line of this was written.
//!
//! The third one is used. The data block stays in ONE part (the last, so
//! that `.Ltext_end` really lies behind all the text for DWARF), and every
//! label it defines is announced global.
//!
//! ## What that costs
//!
//! The symbol table of the binary grows by those labels (5,084 of them for
//! Certus, about 150 KB of `.symtab`, which no loader ever reads). The
//! program text does not change by one octet. Because of that the split is
//! **opt-in**: without `-j` the compiler writes one file and calls `as`
//! once, exactly as before, and `tools/repro/run.sh` keeps comparing
//! octet for octet.

/// The pieces one assembly text falls apart into.
pub struct Split {
    /// The finished assembly texts, in link order.
    pub parts: Vec<String>,
}

/// Is this line the beginning of a function?
fn is_func_start(line: &str) -> bool {
    line.starts_with(".globl ")
}

/// Cuts `asm` into at most `n` parts.
///
/// `tag` is the answer to a bug the Osum kernel found within minutes of the
/// first version of this module: the kernel build compiles `kmain.fi` and
/// `uprog.fi` into TWO objects and links them together. Both carry a
/// `.Lpanicmsg0`, and as long as those labels are local that is fine —
/// announced global, `ld` says `multiple definition of '.Lpanicmsg0'` a
/// hundred times over. So whenever the result is an OBJECT (`-c`,
/// `profile kernel`) the caller hands in a tag that is unique to this
/// compilation, and every announced label carries it. For an executable
/// there is only ever one such object and the tag stays `None`.
///
/// Returns `None` when there is nothing to gain (fewer than two parts, or
/// too few functions) — the caller then walks the old, single path.
pub fn split(asm: &str, n: usize, tag: Option<&str>) -> Option<Split> {
    if n < 2 {
        return None;
    }
    let lines: Vec<&str> = asm.split('\n').collect();
    // The header: everything in front of the first `.globl`. It carries
    // `.intel_syntax`, the `.file` directives of the line table and the
    // `.text` that puts us into the code section — every part needs it.
    let first = lines.iter().position(|l| is_func_start(l))?;
    let header: String = lines[..first].join("\n");

    // Where the functions are.
    let starts: Vec<usize> = (first..lines.len())
        .filter(|&i| is_func_start(lines[i]))
        .collect();
    if starts.len() < n * 2 {
        return None;
    }

    // The trailing data block: from the first section switch AFTER the last
    // function to the end. Everything in it is data the whole program
    // shares — panic messages, method tables, function records, `static`s,
    // and (without the optimizer) `.debug_info`.
    let last = *starts.last().unwrap();
    let mut tail_at = lines.len();
    for i in last..lines.len() {
        let t = lines[i].trim();
        if t.starts_with(".section .rodata")
            || t.starts_with(".section .data")
            || t.starts_with(".section .bss")
            || t == ".Ltext_end:"
        {
            tail_at = i;
            break;
        }
    }
    let tail: Vec<&str> = lines[tail_at..].to_vec();

    // Every label the tail DEFINES has to become globally visible, because
    // the functions that use it now sit in other object files.
    let mut announce = String::new();
    let mut names: crate::fasthash::HashSet<&str> = Default::default();
    for l in &tail {
        let t = l.trim();
        if t.starts_with(".L") && t.ends_with(':') && !t.contains(' ') {
            let name = &t[..t.len() - 1];
            names.insert(name);
            announce.push_str(".globl ");
            announce.push_str(name);
            if let Some(g) = tag {
                announce.push_str(g);
            }
            announce.push('\n');
        }
    }

    // Cut the body into parts of roughly equal LENGTH, never inside a
    // function. Length, not function count: one function of
    // `bin/firnc1.fi` is 131,662 of the 353,944 lines on its own.
    let body_len = tail_at - first;
    let target = body_len / n;
    let mut cuts: Vec<usize> = vec![first];
    for &s in &starts {
        if s - *cuts.last().unwrap() >= target && cuts.len() < n {
            cuts.push(s);
        }
    }
    cuts.push(tail_at);
    if cuts.len() < 3 {
        return None;
    }

    let mut parts = Vec::new();
    let count = cuts.len() - 1;
    for k in 0..count {
        let mut out = String::with_capacity(
            (cuts[k + 1] - cuts[k]) * 24 + header.len() + 64,
        );
        out.push_str(&header);
        out.push('\n');
        // A part must not inherit the section of the part in front of it.
        out.push_str(".text\n");
        for l in &lines[cuts[k]..cuts[k + 1]] {
            push_tagged(&mut out, l, tag, &names);
            out.push('\n');
        }
        if k + 1 == count {
            out.push_str(&announce);
            for l in &tail {
                push_tagged(&mut out, l, tag, &names);
                out.push('\n');
            }
        } else {
            // Every object needs its own `.note.GNU-stack`, otherwise `ld`
            // says "missing .note.GNU-stack section implies executable
            // stack" for each part -- the directive stands once at the very
            // end of the whole text and would land in the last part alone.
            out.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        }
        parts.push(out);
    }
    Some(Split { parts })
}

/// Writes one line, appending `tag` to every `.L…` name that the trailing
/// data block defines. One pass over the line, no allocation in the common
/// case (a line without a `.L` is copied straight through).
fn push_tagged(
    out: &mut String,
    line: &str,
    tag: Option<&str>,
    names: &crate::fasthash::HashSet<&str>,
) {
    let tag = match tag {
        Some(t) if line.contains(".L") => t,
        _ => {
            out.push_str(line);
            return;
        }
    };
    let b = line.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'.' && i + 1 < b.len() && b[i + 1] == b'L' {
            let mut j = i + 2;
            while j < b.len()
                && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'.' || b[j] == b'$')
            {
                j += 1;
            }
            let name = &line[i..j];
            out.push_str(name);
            if names.contains(name) {
                out.push_str(tag);
            }
            i = j;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(funcs: usize) -> String {
        let mut s = String::from(".intel_syntax noprefix\n.file 1 \"x.fi\"\n.text\n");
        for i in 0..funcs {
            s.push_str(&format!(".globl _F0.f{}\n_F0.f{}:\n", i, i));
            for _ in 0..20 {
                s.push_str("    mov rax, rbx\n");
            }
            s.push_str("    lea rax, [rip + .Lpanicmsg0]\n    ret\n");
        }
        s.push_str(".section .rodata\n.Lpanicmsg0:\n    .ascii \"boom\"\n");
        s
    }

    #[test]
    fn nothing_to_gain() {
        assert!(split(&sample(40), 1, None).is_none());
        assert!(split(&sample(2), 8, None).is_none());
    }

    #[test]
    fn every_function_lands_exactly_once() {
        let asm = sample(64);
        let sp = split(&asm, 4, None).expect("split");
        assert!(sp.parts.len() >= 2);
        for i in 0..64 {
            let needle = format!("\n_F0.f{}:\n", i);
            let n: usize = sp.parts.iter().filter(|p| p.contains(&needle)).count();
            assert_eq!(n, 1, "function f{} appears in {} parts", i, n);
        }
    }

    #[test]
    fn the_data_is_announced_and_appears_once() {
        let sp = split(&sample(64), 4, None).expect("split");
        let defs: usize = sp
            .parts
            .iter()
            .filter(|p| p.contains("\n.Lpanicmsg0:\n"))
            .count();
        assert_eq!(defs, 1, "the data block must exist exactly once");
        let ann: usize = sp
            .parts
            .iter()
            .filter(|p| p.contains(".globl .Lpanicmsg0\n"))
            .count();
        assert_eq!(ann, 1, "the label has to be announced global");
        // and it has to be in the LAST part, so that the text of the other
        // parts lies in front of it.
        assert!(sp.parts.last().unwrap().contains("\n.Lpanicmsg0:\n"));
    }

    /// The Osum case: two objects, both with a `.Lpanicmsg0`. With a tag
    /// the names cannot collide any more, and every USE is renamed with the
    /// definition -- otherwise the object would not even assemble.
    #[test]
    fn the_tag_renames_definition_and_use_together() {
        let sp = split(&sample(64), 4, Some(".u7")).expect("split");
        let all: String = sp.parts.join("\n");
        assert!(all.contains(".globl .Lpanicmsg0.u7"));
        assert!(all.contains("\n.Lpanicmsg0.u7:\n"));
        assert!(all.contains("lea rax, [rip + .Lpanicmsg0.u7]"));
        // and the untagged name must be gone everywhere
        assert!(!all.contains(".Lpanicmsg0]"));
        assert!(!all.contains(".Lpanicmsg0:"));
        // block labels of a function are NOT in the tail and stay untouched
        assert!(!all.contains(".u7.u7"));
    }

    #[test]
    fn every_part_starts_in_the_text_section() {
        let sp = split(&sample(64), 4, None).expect("split");
        for p in &sp.parts {
            assert!(p.contains(".intel_syntax noprefix"));
            assert!(p.contains("\n.text\n"));
        }
    }
}
