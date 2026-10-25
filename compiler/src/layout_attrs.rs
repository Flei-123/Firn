// SPDX-License-Identifier: MPL-2.0
use crate::ast::StructDecl;
use crate::diag::Diags;
use crate::types::Type;

pub(crate) const MAX_ALIGN: u64 = 4096;
pub(crate) const FRAME_ALIGN: u64 = 16;

pub(crate) struct LayoutAttrs {
    pub(crate) packed: bool,
    pub(crate) min_align: u64,
}

pub(crate) fn read(decl: &StructDecl, dg: &mut Diags) -> LayoutAttrs {
    let mut out = LayoutAttrs { packed: false, min_align: 1 };
    let mut seen_align = false;
    for a in &decl.attrs {
        match a.name.as_str() {
            "packed" if a.args.is_empty() => out.packed = true,
            "align" if a.args.len() == 1 => {
                if seen_align {
                    dg.error(a.span, "attribute 'align' is given twice");
                    continue;
                }
                seen_align = true;
                match a.args[0].parse::<u64>() {
                    Ok(n) if n.is_power_of_two() && n <= MAX_ALIGN => out.min_align = n,
                    _ => dg.error_note(
                        a.span,
                        format!("'#[align({})]' is not a valid alignment", a.args[0]),
                        format!("use a power of two from 1 to {}", MAX_ALIGN),
                    ),
                }
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn has_unaligned_field(ty: &Type, tcx: &crate::types::TypeCtx, base: u64) -> bool {
    match ty {
        Type::Struct(i) => match tcx.structs.get(*i) {
            Some(s) => s
                .fields
                .iter()
                .any(|f| has_unaligned_field(&f.ty, tcx, base + f.offset)),
            None => false,
        },
        Type::Array(e, n) => {
            let step = tcx.size_of(e);
            (0..(*n).min(2)).any(|k| has_unaligned_field(e, tcx, base + k * step))
        }
        _ => {
            let a = tcx.align_of(ty).max(1);
            base % a != 0
        }
    }
}
