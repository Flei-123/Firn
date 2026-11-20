use crate::diag::Span;
use crate::sema_match::{EnumDef, Pattern, VariantDef};
use crate::types::Type;

type Rows = Vec<Vec<Pattern>>;
pub(crate) type EnumLookup<'a> = &'a dyn Fn(&Type) -> Option<EnumDef>;

fn expand_or(rows: Rows) -> Rows {
    let mut out = Vec::new();
    for row in rows {
        match &row[0] {
            Pattern::Or(alts, _) => {
                for alt in alts {
                    let mut r = vec![alt.clone()];
                    r.extend_from_slice(&row[1..]);
                    out.extend(expand_or(vec![r]));
                }
            }
            _ => out.push(row),
        }
    }
    out
}

fn wild_row(arity: usize, rest: &[Pattern]) -> Vec<Pattern> {
    let mut r = vec![Pattern::Wild(Span::none()); arity];
    r.extend_from_slice(rest);
    r
}

fn specialize(rows: &Rows, vname: &str, arity: usize) -> Rows {
    let mut out = Vec::new();
    for row in rows {
        match &row[0] {
            Pattern::Variant { vname: n, subs, .. } if n == vname => {
                let mut r: Vec<Pattern> = subs.iter().take(arity).cloned().collect();
                while r.len() < arity {
                    r.push(Pattern::Wild(Span::none()));
                }
                r.extend_from_slice(&row[1..]);
                out.push(r);
            }
            p if p.is_irrefutable() => out.push(wild_row(arity, &row[1..])),
            _ => {}
        }
    }
    out
}

fn specialize_bool(rows: &Rows, value: bool) -> Rows {
    rows.iter()
        .filter(|row| match &row[0] {
            Pattern::Bool(x, _) => *x == value,
            p => p.is_irrefutable(),
        })
        .map(|row| row[1..].to_vec())
        .collect()
}

fn covers_all(rows: Rows, tys: &[Type], enum_of: EnumLookup) -> bool {
    let Some((first, rest)) = tys.split_first() else {
        return !rows.is_empty();
    };
    if rows.is_empty() {
        return false;
    }
    let rows = expand_or(rows);
    if let Some(def) = enum_of(first) {
        return def.variants.iter().all(|v| variant_cols(&rows, v, rest, enum_of));
    }
    if matches!(first, Type::Bool) {
        return [true, false]
            .iter()
            .all(|b| covers_all(specialize_bool(&rows, *b), rest, enum_of));
    }
    let default: Rows = rows
        .iter()
        .filter(|row| row[0].is_irrefutable())
        .map(|row| row[1..].to_vec())
        .collect();
    covers_all(default, rest, enum_of)
}

fn variant_cols(rows: &Rows, v: &VariantDef, rest: &[Type], enum_of: EnumLookup) -> bool {
    let mut cols = v.fields.clone();
    cols.extend_from_slice(rest);
    covers_all(specialize(rows, &v.name, v.fields.len()), &cols, enum_of)
}

pub(crate) fn variant_covered(patterns: &[&Pattern], v: &VariantDef, enum_of: EnumLookup) -> bool {
    let rows: Rows = expand_or(patterns.iter().map(|p| vec![(*p).clone()]).collect());
    variant_cols(&rows, v, &[], enum_of)
}
