use crate::ast::TypeExpr;
use crate::diag::Span;
use crate::types::Type;

pub(crate) type Error = (Span, String);

pub(crate) const MAX_ARRAY_LEN: i128 = 1 << 22;

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Int(i128),
    Arr(Vec<Value>),
}

#[derive(Clone, Debug)]
pub(crate) enum Shape {
    Any,
    Bool,
    Int(Type),
    Arr(Box<Shape>, u64),
}

pub(crate) fn shape_of(t: &TypeExpr) -> Shape {
    match t {
        TypeExpr::Named(name, _) => match crate::sema::prim_type(name) {
            Some(Type::Bool) => Shape::Bool,
            Some(ty) if ty.is_concrete_int() => Shape::Int(ty),
            _ => Shape::Any,
        },
        TypeExpr::Array { elem, len, .. } => Shape::Arr(Box::new(shape_of(elem)), *len),
        _ => Shape::Any,
    }
}

pub(crate) fn elem_shape(shape: &Shape, depth: usize) -> Shape {
    let mut cur = shape;
    for _ in 0..depth {
        match cur {
            Shape::Arr(inner, _) => cur = inner,
            _ => return Shape::Any,
        }
    }
    cur.clone()
}

pub(crate) fn coerce(v: Value, shape: &Shape) -> Value {
    match (v, shape) {
        (Value::Int(x), Shape::Int(t)) => Value::Int(crate::sema::comptime_wrap(x, t)),
        (Value::Int(x), Shape::Bool) => Value::Int((x != 0) as i128),
        (Value::Arr(items), Shape::Arr(inner, _)) => {
            Value::Arr(items.into_iter().map(|it| coerce(it, inner)).collect())
        }
        (v, _) => v,
    }
}

pub(crate) fn to_int(v: Value, span: Span) -> Result<i128, Error> {
    match v {
        Value::Int(x) => Ok(x),
        Value::Arr(_) => Err((span, "comptime: an array cannot be used as a number here".to_string())),
    }
}

pub(crate) fn follow<'v>(root: &'v Value, path: &[i128], span: Span) -> Result<&'v Value, Error> {
    let mut cur = root;
    for &i in path {
        match cur {
            Value::Arr(items) if i >= 0 && (i as usize) < items.len() => cur = &items[i as usize],
            Value::Arr(items) => return Err(out_of_range(i, items.len(), span)),
            Value::Int(_) => return Err((span, "comptime: only an array can be indexed".to_string())),
        }
    }
    Ok(cur)
}

pub(crate) fn follow_mut<'v>(
    root: &'v mut Value,
    path: &[i128],
    span: Span,
) -> Result<&'v mut Value, Error> {
    let Some((&first, rest)) = path.split_first() else {
        return Ok(root);
    };
    match root {
        Value::Arr(items) => {
            let len = items.len();
            match usize::try_from(first).ok().filter(|i| *i < len) {
                Some(i) => follow_mut(&mut items[i], rest, span),
                None => Err(out_of_range(first, len, span)),
            }
        }
        Value::Int(_) => Err((span, "comptime: only an array can be indexed".to_string())),
    }
}

fn out_of_range(i: i128, len: usize, span: Span) -> Error {
    (span, format!("comptime: index {} is out of range for an array of {} elements", i, len))
}

pub(crate) fn encode(v: &Value, ty: &Type, span: Span, out: &mut Vec<u8>) -> Result<(), Error> {
    match (v, ty) {
        (Value::Arr(items), Type::Array(el, n)) => {
            if items.len() as u64 != *n {
                return Err((
                    span,
                    format!("the array has {} elements, the value has {}", n, items.len()),
                ));
            }
            items.iter().try_for_each(|it| encode(it, el, span, out))
        }
        (Value::Int(x), t) if t.is_concrete_int() || *t == Type::Bool => {
            let wrapped = crate::sema::comptime_wrap(*x, t);
            let bytes = (wrapped as u128).to_le_bytes();
            out.extend_from_slice(&bytes[..(t.bits().max(8) / 8) as usize]);
            Ok(())
        }
        _ => Err((
            span,
            "a 'static' computed by a call must be an integer, a bool or an array of those".to_string(),
        )),
    }
}
