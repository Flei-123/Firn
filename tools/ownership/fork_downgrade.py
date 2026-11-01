import os
import sys

root = sys.argv[1]

EDITS = [
    ("walk.rs", " | ExprKind::Named(_, a, _)", "", 2),
    ("walk.rs", "        ExprKind::IfElse(a, b, c) => vec![a, b, c],\n", "", 2),
    ("reuse.rs", " | UnOp::InoutOf", "", 1),
    ("reuse.rs", """        ExprKind::IfElse(c, a, b) => {
            find_candidate(rw, c, cond, out);
            find_candidate(rw, a, true, out);
            find_candidate(rw, b, true, out);
        }
""", "", 1),
    ("borrow.rs", "p.refk != 0 || ", "", 1),
    ("fir/flow.rs", "            | Op::CheckedIdx { .. }\n            | Op::CheckedIdxV { .. } => {", "            | Op::CheckedIdx { .. } => {", 1),
]

for name, old, new, count in EDITS:
    path = os.path.join(root, name)
    text = open(path).read()
    if text.count(old) != count:
        sys.exit("%s: expected %d x %r, found %d" % (name, count, old[:40], text.count(old)))
    open(path, "w").write(text.replace(old, new))

mod_path = os.path.join(root, "mod.rs")
mod_text = open(mod_path).read()
if "fn apply_env" not in mod_text:
    env_fn = """pub(crate) fn apply_env() -> Result<(), String> {
    match std::env::var("FIRNC_OWNERSHIP") {
        Ok(v) if !v.is_empty() => match parse(&v) {
            Some(m) => {
                set_mode(m);
                Ok(())
            }
            None => Err(format!("unknown FIRNC_OWNERSHIP value '{}' (allowed: off, on, check)", v)),
        },
        _ => Ok(()),
    }
}

"""
    marker = "pub(crate) fn mode() -> Mode {"
    if marker not in mod_text:
        sys.exit("mod.rs: marker for apply_env not found")
    open(mod_path, "w").write(mod_text.replace(marker, env_fn + marker, 1))
