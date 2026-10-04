d = '/root/firn-wt-db/lib/db/'
e = open(d + 'expr.fi').read()

# round with digits: SQLite adds half a unit in the last place before cutting
old = '''        var m: f64 = 1.0
        var i: i64 = 0
        while i < digits {
            m = m * 10.0
            i = i + 1
        }
        if x > 1.0e15 || x < -1.0e15 {
            return rc.val_real(x)
        }
        return rc.val_real(round_half_away(x * m) / m)'''
new = '''        var m: f64 = 1.0
        var rounder: f64 = 0.5
        var i: i64 = 0
        while i < digits {
            m = m * 10.0
            rounder = rounder * 0.1
            i = i + 1
        }
        if x > 1.0e15 || x < -1.0e15 {
            return rc.val_real(x)
        }
        if x < 0.0 {
            return rc.val_real(0.0 - math.floor(((0.0 - x) + rounder) * m) / m)
        }
        return rc.val_real(math.floor((x + rounder) * m) / m)'''
assert old in e
e = e.replace(old, new)

old = '''                if (f == F_MIN_S && c < 0) || (f == F_MAX_S && c > 0) {'''
new = '''                if (f == F_MIN_S && c <= 0) || (f == F_MAX_S && c > 0) {'''
assert old in e
e = e.replace(old, new)
open(d + 'expr.fi', 'w').write(e)

l = open(d + 'sqllex.fi').read()
old = '''            if !over {
                (*t).kind = TK_INT
                (*t).ival = v as i64
                return
            }'''
new = '''            if !over {
                (*t).kind = TK_INT
                (*t).ival = v as i64
                return
            }
            if v == 922337203685477580 {
                // 9223372036854775808 exactly: INTEGER only after a minus sign
            }'''
# the exact 2^63 case: detect separately by text
old2 = '''        if !is_real {
            // integer if it fits an i64, else a real'''
new2 = '''        if !is_real && i - s == 19 && rt.ld8(p, s) == 57 as u8 && rt.ld8(p, s + 1) == 50 as u8
            && rt.ld8(p, s + 2) == 50 as u8 && rt.ld8(p, s + 3) == 51 as u8 && rt.ld8(p, s + 4) == 51 as u8
            && rt.ld8(p, s + 5) == 55 as u8 && rt.ld8(p, s + 6) == 50 as u8 && rt.ld8(p, s + 7) == 48 as u8
            && rt.ld8(p, s + 8) == 51 as u8 && rt.ld8(p, s + 9) == 54 as u8 && rt.ld8(p, s + 10) == 56 as u8
            && rt.ld8(p, s + 11) == 53 as u8 && rt.ld8(p, s + 12) == 52 as u8 && rt.ld8(p, s + 13) == 55 as u8
            && rt.ld8(p, s + 14) == 55 as u8 && rt.ld8(p, s + 15) == 53 as u8 && rt.ld8(p, s + 16) == 56 as u8
            && rt.ld8(p, s + 17) == 48 as u8 && rt.ld8(p, s + 18) == 56 as u8 {
            (*t).kind = TK_INT
            (*t).ival = 0 - 9223372036854775807 - 1
            (*t).esc = true
            return
        }
        if !is_real {
            // integer if it fits an i64, else a real'''
assert old2 in l
l = l.replace(old2, new2)
open(d + 'sqllex.fi', 'w').write(l)

p = open(d + 'sqlparse.fi').read()
# parse_primary TK_INT: remember the 2^63 marker in flag 2
old = '''    if k == TK_INT {
        let n: i32 = try mk(ps, ax.N_INT)
        (*ax.ast_n((*ps).ast, n)).ival = (*t).ival
        advance(ps)
        return n
    }'''
assert old.replace("TK_INT", "tk.TK_INT").replace("N_INT", "N_INT") or True
p = p.replace('''    if k == tk.TK_INT {
        let n: i32 = try mk(ps, ax.N_INT)
        (*ax.ast_n((*ps).ast, n)).ival = (*t).ival
        advance(ps)
        return n
    }''', '''    if k == tk.TK_INT {
        let n: i32 = try mk(ps, ax.N_INT)
        (*ax.ast_n((*ps).ast, n)).ival = (*t).ival
        if (*t).esc {
            (*ax.ast_n((*ps).ast, n)).flag = 2
        }
        advance(ps)
        return n
    }''')
# parse_unary with a negated flag
p = p.replace("fn parse_unary(ps: *mut Parser) -> DbError!i32 {", "fn parse_unary(ps: *mut Parser, negated: bool) -> DbError!i32 {")
p = p.replace("let e: i32 = try parse_unary(ps)\n        // -literal folds into the literal",
              "let e: i32 = try parse_unary(ps, c == tk.P_MINUS)\n        // -literal folds into the literal")
p = p.replace('''        let p: i32 = try parse_primary(ps)
        r = p
    }
    // postfix COLLATE''', '''        let p: i32 = try parse_primary(ps)
        r = p
        let pn: *mut ax.Node = ax.ast_n((*ps).ast, p)
        if (*pn).kind == ax.N_INT && (*pn).flag == 2 && !negated {
            // 9223372036854775808 without a minus sign is a REAL
            (*pn).kind = ax.N_REAL
            (*pn).flag = 0
            (*pn).fval = 9223372036854775808.0
        }
    }
    // postfix COLLATE''')
p = p.replace('''        if c == tk.P_MINUS && (*en).kind == ax.N_INT && (*en).ival != 0 - 9223372036854775807 - 1 {
            (*en).ival = 0 - (*en).ival
            r = e''', '''        if c == tk.P_MINUS && (*en).kind == ax.N_INT && (*en).flag == 2 {
            (*en).flag = 0
            r = e
        } else if c == tk.P_MINUS && (*en).kind == ax.N_INT && (*en).ival != 0 - 9223372036854775807 - 1 {
            (*en).ival = 0 - (*en).ival
            r = e''')
p = p.replace("let u: i32 = try parse_unary(ps)\n        return u", "let u: i32 = try parse_unary(ps, false)\n        return u")
p = p.replace("let u: i32 = try parse_unary(ps)\n        left = u", "let u: i32 = try parse_unary(ps, false)\n        left = u")
p = p.replace("let e: i32 = try parse_unary(ps)\n                (*ax.ast_n((*ps).ast, s)).a = e", "let e: i32 = try parse_unary(ps, false)\n                (*ax.ast_n((*ps).ast, s)).a = e")
# like( and glob( as function names
old = '''            if code != 0 && hard_reserved(code) {
                return syntax(ps)
            }'''
new = '''            if code != 0 && hard_reserved(code) {
                // LIKE( and GLOB( are also function names
                var fn_ok: bool = false
                if code == tk.K_LIKE || code == tk.K_GLOB {
                    let sl: tk.Lexer = (*ps).lx
                    var nt: tk.Token = (*ps).tok
                    tk.lex_next(&(*ps).lx, &nt)
                    fn_ok = nt.kind == tk.TK_OP && nt.code == tk.P_LPAREN
                    (*ps).lx = sl
                }
                if !fn_ok {
                    return syntax(ps)
                }
            }'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)
print("ok5")
