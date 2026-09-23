// SPDX-License-Identifier: MPL-2.0
//! **Round GAPS** — `__include_str("file")`: the contents of a file as a
//! text literal, read at build time.
//!
//! ```firn
//! const HELP: str = __include_str("help.txt")
//! var table: [u8; 256] = __include_str("table.bin")   // an array context works too
//! ```
//!
//! Until this round a program that wanted a file's text in its binary had
//! to generate Firn source from it (`tools/gen_gctext.sh` packs
//! `lib/gc/gc.fi` into u64 words so that firnc1 can carry it;
//! docs/LUECKEN.md B21). firnc0 itself does exactly this with Rust's
//! `include_str!`.
//!
//! The rules:
//!
//! * The argument is a text literal -- the path is read at build time, no
//!   expression could compute it.
//! * A relative path is relative to the DIRECTORY OF THE SOURCE FILE that
//!   contains the call (like Rust), not to the working directory of the
//!   compiler: the same source builds the same way from anywhere.
//! * The result is exactly the node a written literal of these octets would
//!   be (`Parser::text_from_octets`), so the context decides as always:
//!   `str` (its octets in `.rodata`, statics.rs::intern_text) or `[u8; N]`.
//!   The octets are taken as they are -- no UTF-8 check, a `str` is octets.
//! * At most 1 MiB: every octet is an expression node until the lowering,
//!   and a build that silently needs gigabytes is worse than a clear limit.
//! * A file that cannot be read is an error with the path that was tried.
//!
//! firnc1 counts a file that uses it as "not core" (parser.fi prescan);
//! porting needs the source path in its parser.

use crate::ast::Expr;
use crate::diag::Span;
use crate::parser::Parser;

pub(crate) const FN_INCLUDE: &str = "__include_str";

/// Upper bound of an included file.
const MAX_OCTETS: usize = 1 << 20;

pub(crate) fn is_include_call(name: &str) -> bool {
    name == FN_INCLUDE
}

impl<'a> Parser<'a> {
    /// `__include_str("path")` -> the text literal of the file's octets.
    pub(crate) fn include_call(&mut self, args: &[Expr], sp: Span) -> Expr {
        if args.len() != 1 {
            self.dg.error_note(
                sp,
                format!("'{}' expects 1 argument, found {}", FN_INCLUDE, args.len()),
                "call: __include_str(\"file.txt\")",
            );
            return self.broken_expr(sp);
        }
        let raw = match Parser::literal_octets(&args[0]) {
            Some(b) => b,
            None => {
                self.dg.error_note(
                    args[0].span,
                    format!("the argument of '{}' has to be a text literal", FN_INCLUDE),
                    "the path is read at build time; there is no expression yet that could compute it",
                );
                return self.broken_expr(sp);
            }
        };
        let rel = match String::from_utf8(raw) {
            Ok(s) if !s.is_empty() => s,
            _ => {
                self.dg.error(args[0].span, "the path of '__include_str' has to be non-empty UTF-8");
                return self.broken_expr(sp);
            }
        };
        let rel_path = std::path::Path::new(&rel);
        let full = if rel_path.is_absolute() {
            rel_path.to_path_buf()
        } else {
            let src = std::path::Path::new(self.dg.file_name(self.file));
            match src.parent() {
                Some(dir) => dir.join(rel_path),
                None => rel_path.to_path_buf(),
            }
        };
        let bytes = match std::fs::read(&full) {
            Ok(b) => b,
            Err(e) => {
                self.dg.error(
                    args[0].span,
                    format!("'{}': cannot read '{}': {}", FN_INCLUDE, full.display(), e),
                );
                return self.broken_expr(sp);
            }
        };
        if bytes.len() > MAX_OCTETS {
            self.dg.error_note(
                args[0].span,
                format!(
                    "'{}': '{}' has {} octets, the limit is {}",
                    FN_INCLUDE,
                    full.display(),
                    bytes.len(),
                    MAX_OCTETS
                ),
                "every octet is a node of the tree until the lowering; split the file or generate a table",
            );
            return self.broken_expr(sp);
        }
        self.text_from_octets(sp, &bytes)
    }
}
