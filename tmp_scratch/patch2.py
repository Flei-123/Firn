d = '/root/firn-wt-db/lib/db/'
p = open(d + 'sqlparse.fi').read()
old = '''                let n: i32 = try mk(ps, ax.N_INT)
                if code == tk.K_TRUE {
                    (*ax.ast_n((*ps).ast, n)).ival = 1
                }
                advance(ps)
                return n'''
new = '''                let n: i32 = try mk(ps, ax.N_INT)
                (*ax.ast_n((*ps).ast, n)).flag = 1
                if code == tk.K_TRUE {
                    (*ax.ast_n((*ps).ast, n)).ival = 1
                }
                advance(ps)
                return n'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)

e = open(d + 'expr.fi').read()
old = '''fn eval_is(cx: *mut Ctx, nd: *mut ax.Node) -> DbError!rc.Val {
    let x: rc.Val = try eval(cx, (*nd).a)'''
new = '''fn eval_is(cx: *mut Ctx, nd: *mut ax.Node) -> DbError!rc.Val {
    let x: rc.Val = try eval(cx, (*nd).a)
    // x IS TRUE / x IS FALSE: the truth of x, NULL counts as neither
    let rn: *mut ax.Node = ax.ast_n((*cx).ast, (*nd).b)
    if (*rn).kind == ax.N_INT && (*rn).flag == 1 {
        var xt: rc.Val = x
        let tr: i64 = vl.val_truth(&xt)
        var hit: bool = false
        if (*rn).ival != 0 {
            hit = tr == 1
        } else {
            hit = tr == 0
        }
        if (*nd).op == 1 {
            return bool_val(!hit)
        }
        return bool_val(hit)
    }'''
assert old in e
e = e.replace(old, new)
open(d + 'expr.fi', 'w').write(e)
print("ok2")
