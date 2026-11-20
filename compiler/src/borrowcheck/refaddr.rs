use super::{place_root, Finding};
use crate::ast::{ExprKind, Program, UnOp};
use crate::fx::{HashMap, HashSet};
use crate::refparam::walk_exprs;

pub(super) fn check(prog: &Program, out: &mut Vec<Finding>) {
    let mut sigs: HashMap<&str, &[crate::ast::Param]> = HashMap::default();
    for f in &prog.funcs {
        sigs.entry(f.name.as_str()).or_insert(f.params.as_slice());
    }
    for func in &prog.funcs {
        if crate::escape::has_allow_escape(func) {
            continue;
        }
        let refs: Vec<&str> =
            func.params.iter().filter(|p| p.refk != 0).map(|p| p.name.as_str()).collect();
        if refs.is_empty() {
            continue;
        }
        let mut lent: HashSet<crate::ast::ExprId> = HashSet::default();
        walk_exprs(&func.body, &mut |e| {
            if let ExprKind::Call(name, args, _) = &e.kind {
                for (i, a) in args.iter().enumerate() {
                    let plain = sigs
                        .get(name.as_str())
                        .map(|p| p.get(i).map_or(true, |p| p.refk == 0))
                        .unwrap_or(false);
                    if !plain {
                        lent.insert(a.id);
                    }
                }
            }
        });
        walk_exprs(&func.body, &mut |e| {
            let inner = match &e.kind {
                ExprKind::Unary(UnOp::AddrOf | UnOp::InoutOf, inner) => inner,
                _ => return,
            };
            let name = match place_root(inner) {
                Some((n, true)) if refs.contains(&n) => n,
                _ => return,
            };
            if lent.contains(&(e.id)) {
                return;
            }
            out.push(Finding {
                span: e.span,
                msg: format!(
                    "a raw pointer into the target of the reference parameter '{name}' cannot be created"
                ),
                note: format!(
                    "'{name}' is a reference: the pointer would outlive the borrow and the caller's exclusivity ends with the call"
                ),
                help: format!(
                    "pass the place on as '&' / 'inout' argument of a function with a reference parameter, or copy the value; '#[allow_escape]' on '{}' vouches for the function",
                    func.name.replace("__", ".")
                ),
            });
        });
    }
}
