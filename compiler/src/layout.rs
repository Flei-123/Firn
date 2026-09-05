// SPDX-License-Identifier: GPL-2.0-only
//! Intermediate layer between **field access** and **storage location**
//! (`DESIGN_GOALS.md` §8, foundation point from §10.4).
//!
//! # Why this module exists
//!
//! Today Firn knows exactly one arrangement: **array of structures** (AoS).
//! All fields of a value sit contiguously, the address of a field is
//! `base + offset`. That very equation, though, is the assumption that makes
//! **structure of arrays** (SoA, the planned `SoaVec[T]`) impossible: there
//! the contiguous value does not physically exist at all, every field has its
//! own array, and the address of field `f` of element `i` reads
//! `column_f + i * size(f)` — not `base + offset_f`.
//!
//! As long as `a.b` is spelled out across the whole tree as "base plus
//! offset", SoA cannot be retrofitted without touching every call site.
//! That is why **every** field and element access of the lowering runs
//! through the functions of this module. Adding a second arrangement then
//! means: extend a case split **here**.
//!
//! # Architecture rule
//!
//! Outside this module nobody in the lowering computes `field.offset` and
//! nobody builds element addresses by hand. `tools/schichten/run.sh` checks
//! that and is part of `test.sh`.
//!
//! # What is (still) NOT here
//!
//! The SoA arrangement itself. It needs a collection type `SoaVec[T]`, view
//! values rather than pointers, and generics — all of that is phase 3/4 of
//! the ROADMAP. This module is the **precondition** for it, not the work.

use crate::diag::Span;
use crate::fir::{BinOp as FBin, FTy, Op, Val};
use crate::lower::Lower;

impl Lower<'_> {
    /// Address of field `fname` of structure `sidx`, whose value sits at
    /// `base`.
    ///
    /// **The only** way to get at a field address. Under AoS that is
    /// `base + offset`; under SoA the column address would be computed here
    /// instead.
    pub(crate) fn field_addr(
        &mut self,
        base: Val,
        sidx: usize,
        fname: &str,
        span: Span,
    ) -> Option<Val> {
        let off = match self.info.tcx.structs.get(sidx).and_then(|s| s.field(fname)) {
            Some(f) => f.offset,
            None => return self.ice(span, "unknown field in lowering"),
        };
        Some(self.field_addr_at(base, off))
    }

    /// Address of a field whose offset is already known.
    ///
    /// Needed by `lower_match.rs` for the payload data of an enum variant:
    /// there the offset sits in `VariantDef::offsets`, not in a named
    /// field list. This path too runs through here deliberately, so that
    /// there is only **one** place where an offset is turned into a real
    /// address.
    pub(crate) fn field_addr_at(&mut self, base: Val, offset: u64) -> Val {
        self.ptradd_const(base, offset)
    }

    /// Address of the element with the **constant** index `index` in a field
    /// of elements of size `elem_size` starting at `base`.
    ///
    /// For literals (`[a, b, c]`) and unrolled repetitions.
    pub(crate) fn elem_addr_const(&mut self, base: Val, elem_size: u64, index: u64) -> Val {
        self.ptradd_const(base, elem_size * index)
    }

    /// Address of the element with the **computed** index `index`.
    ///
    /// `index` is brought to `u64`, multiplied by the element size and added
    /// to `base`. Under SoA `base` would instead be the column base of the
    /// respective field, and the multiplication would run per field separately.
    pub(crate) fn elem_addr(
        &mut self,
        base: Val,
        elem_size: u64,
        index: Val,
        index_ty: FTy,
    ) -> Val {
        let idx64 = if index_ty == FTy::U64 {
            index
        } else {
            self.push(FTy::U64, Op::Cast { src: index, from: index_ty })
        };
        let sz = self.constant(FTy::U64, elem_size as i128);
        let off = self.push(FTy::U64, Op::Bin(FBin::Mul, idx64, sz));
        self.push(FTy::Ptr, Op::PtrAdd { base, off })
    }
}
