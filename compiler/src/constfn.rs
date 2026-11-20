use crate::ast::{ExprKind, Program, UnOp};
use crate::diag::Diags;

pub(crate) fn check(prog: &Program, dg: &mut Diags) {
    for f in prog.funcs.iter().filter(|f| f.is_const) {
        crate::refparam::walk_exprs(&f.body, &mut |e| match &e.kind {
            ExprKind::Call(name, _, _) => {
                if prog.funcs.iter().any(|g| g.name == *name && !g.is_const) {
                    dg.error(
                        e.span,
                        format!("'const fn {}' calls '{}', which is not a 'const fn'", f.name, name),
                    );
                }
            }
            ExprKind::Unary(UnOp::AddrOf | UnOp::InoutOf | UnOp::Deref, _) => {
                dg.error(e.span, format!("'const fn {}' must not use pointers", f.name));
            }
            ExprKind::Float(..) => {
                dg.error(e.span, format!("'const fn {}' must not use floating point yet", f.name));
            }
            _ => {}
        });
    }
}
